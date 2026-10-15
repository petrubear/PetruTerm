//! Relay between one desktop host and its phones.
//!
//! Paths: `/host/<64 hex>` (the desktop) and `/client/<64 hex>` (a phone). The id is the
//! hex of the host's static public key. All payloads are Noise ciphertext produced end to
//! end, so the relay is untrusted: the worst it can do is drop, delay or refuse traffic.
//!
//! Frames, host side of the relay (WebSocket binary):
//! - relay to host: `[1][conn u64 BE][payload]` client data, `[2][conn]` client gone,
//!   `[3][conn]` new client
//! - host to relay: `[1][conn][payload]` data for a client, `[2][conn]` drop that client
//!
//! Client side carries bare payloads.

use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::Message;

const TO_HOST_DATA: u8 = 1;
const TO_HOST_GONE: u8 = 2;
const TO_HOST_NEW: u8 = 3;
const FROM_HOST_DATA: u8 = 1;
const FROM_HOST_DROP: u8 = 2;

#[derive(Clone, Debug)]
pub struct Limits {
    /// Largest WebSocket message accepted (Noise frames are at most ~64 KiB).
    pub max_frame: usize,
    pub max_clients_per_host: usize,
    pub max_connections_per_ip: usize,
    pub max_connections: usize,
    /// Sustained client-to-host frames per second, per client.
    pub client_frames_per_sec: u32,
    /// Connections silent for this long are dropped (the relay pings every 30s).
    pub idle: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame: 70_000,
            max_clients_per_host: 8,
            max_connections_per_ip: 32,
            max_connections: 4096,
            client_frames_per_sec: 200,
            idle: Duration::from_secs(90),
        }
    }
}

struct HostEntry {
    generation: u64,
    tx: mpsc::UnboundedSender<Message>,
    clients: HashMap<u64, mpsc::UnboundedSender<Message>>,
}

#[derive(Default)]
struct Registry {
    hosts: HashMap<String, HostEntry>,
    per_ip: HashMap<IpAddr, usize>,
    total: usize,
}

struct State {
    registry: Mutex<Registry>,
    limits: Limits,
    next_conn: AtomicU64,
    next_generation: AtomicU64,
}

pub async fn serve(listener: TcpListener, limits: Limits) -> std::io::Result<()> {
    let state = Arc::new(State {
        registry: Mutex::new(Registry::default()),
        limits,
        next_conn: AtomicU64::new(1),
        next_generation: AtomicU64::new(1),
    });
    loop {
        let (stream, peer) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let ip = peer.ip();
            if !admit(&state, ip) {
                log::warn!("connection limit reached for {ip}");
                return;
            }
            handle(stream, &state).await;
            release(&state, ip);
        });
    }
}

fn admit(state: &State, ip: IpAddr) -> bool {
    let mut reg = state.registry.lock().unwrap();
    let per_ip = reg.per_ip.entry(ip).or_insert(0);
    if *per_ip >= state.limits.max_connections_per_ip || reg.total >= state.limits.max_connections {
        return false;
    }
    *reg.per_ip.get_mut(&ip).unwrap() += 1;
    reg.total += 1;
    true
}

fn release(state: &State, ip: IpAddr) {
    let mut reg = state.registry.lock().unwrap();
    reg.total = reg.total.saturating_sub(1);
    if let Some(n) = reg.per_ip.get_mut(&ip) {
        *n -= 1;
        if *n == 0 {
            reg.per_ip.remove(&ip);
        }
    }
}

#[derive(Clone)]
enum Role {
    Host(String),
    Client(String),
}

fn parse_path(path: &str) -> Option<Role> {
    let (role, id) = path.trim_start_matches('/').split_once('/')?;
    let id = id.split('?').next()?;
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let id = id.to_ascii_lowercase();
    match role {
        "host" => Some(Role::Host(id)),
        "client" => Some(Role::Client(id)),
        _ => None,
    }
}

async fn handle(stream: TcpStream, state: &Arc<State>) {
    let mut role = None;
    let config = WebSocketConfig::default()
        .max_message_size(Some(state.limits.max_frame))
        .max_frame_size(Some(state.limits.max_frame));
    #[allow(clippy::result_large_err)]
    let callback = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        match parse_path(req.uri().path()) {
            Some(r) => {
                role = Some(r);
                Ok(resp)
            }
            None => Err(ErrorResponse::new(Some("bad path".into()))),
        }
    };
    let Ok(ws) =
        tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config)).await
    else {
        return;
    };
    match role {
        Some(Role::Host(id)) => host_connection(ws, id, state).await,
        Some(Role::Client(id)) => client_connection(ws, id, state).await,
        None => {}
    }
}

type Ws = tokio_tungstenite::WebSocketStream<TcpStream>;

fn close_msg(code: CloseCode, reason: &str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: reason.to_string().into(),
    }))
}

/// Forward `rx` into the socket and keep it alive with pings. Ends when the channel closes.
async fn pump_out(
    mut sink: impl SinkExt<Message> + Unpin,
    mut rx: mpsc::UnboundedReceiver<Message>,
) {
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(m) => {
                    let closing = matches!(m, Message::Close(_));
                    if sink.send(m).await.is_err() || closing { break; }
                }
                None => break,
            },
            _ = ping.tick() => {
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
            }
        }
    }
    let _ = sink.close().await;
}

async fn host_connection(ws: Ws, id: String, state: &Arc<State>) {
    let (sink, mut stream) = ws.split();
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    let generation = state.next_generation.fetch_add(1, Ordering::Relaxed);
    {
        // Newest registration wins. A squatter can only kick the real host, never read it.
        let mut reg = state.registry.lock().unwrap();
        if let Some(old) = reg.hosts.insert(
            id.clone(),
            HostEntry {
                generation,
                tx: tx.clone(),
                clients: HashMap::new(),
            },
        ) {
            let _ = old.tx.send(close_msg(CloseCode::Policy, "replaced"));
            for c in old.clients.values() {
                let _ = c.send(close_msg(CloseCode::Away, "host replaced"));
            }
        }
    }
    log::info!("host online {}", &id[..8]);
    let out = tokio::spawn(pump_out(sink, rx));
    let idle = state.limits.idle;
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(idle, stream.next()).await {
        let Message::Binary(data) = msg else {
            if matches!(msg, Message::Close(_)) {
                break;
            }
            continue;
        };
        let reg = state.registry.lock().unwrap();
        let Some(entry) = reg.hosts.get(&id).filter(|e| e.generation == generation) else {
            break;
        };
        let Some((&kind, rest)) = data.split_first() else {
            continue;
        };
        let Some((conn, payload)) = rest
            .split_at_checked(8)
            .map(|(c, p)| (u64::from_be_bytes(c.try_into().unwrap()), p))
        else {
            continue;
        };
        match kind {
            FROM_HOST_DATA => {
                if let Some(client) = entry.clients.get(&conn) {
                    let _ = client.send(Message::Binary(payload.to_vec().into()));
                }
            }
            FROM_HOST_DROP => {
                if let Some(client) = entry.clients.get(&conn) {
                    let _ = client.send(close_msg(CloseCode::Normal, "dropped by host"));
                }
            }
            _ => {}
        }
    }
    {
        let mut reg = state.registry.lock().unwrap();
        if reg
            .hosts
            .get(&id)
            .is_some_and(|e| e.generation == generation)
        {
            if let Some(entry) = reg.hosts.remove(&id) {
                for c in entry.clients.values() {
                    let _ = c.send(close_msg(CloseCode::Away, "host offline"));
                }
            }
        }
    }
    drop(tx);
    out.abort();
    log::info!("host offline {}", &id[..8]);
}

async fn client_connection(ws: Ws, host_id: String, state: &Arc<State>) {
    let (sink, mut stream) = ws.split();
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    let conn = state.next_conn.fetch_add(1, Ordering::Relaxed);
    let refusal = {
        let mut reg = state.registry.lock().unwrap();
        match reg.hosts.get_mut(&host_id) {
            None => Some("host offline"),
            Some(e) if e.clients.len() >= state.limits.max_clients_per_host => {
                Some("too many clients")
            }
            Some(e) => {
                e.clients.insert(conn, tx.clone());
                let mut note = vec![TO_HOST_NEW];
                note.extend(conn.to_be_bytes());
                let _ = e.tx.send(Message::Binary(note.into()));
                None
            }
        }
    };
    if let Some(reason) = refusal {
        let _ = tx.send(close_msg(CloseCode::Again, reason));
        pump_out(sink, rx).await;
        return;
    }
    let out = tokio::spawn(pump_out(sink, rx));
    let idle = state.limits.idle;
    let mut bucket = Bucket::new(state.limits.client_frames_per_sec);
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(idle, stream.next()).await {
        match msg {
            Message::Binary(data) => {
                if !bucket.take() {
                    log::warn!("client {conn} rate limited");
                    break;
                }
                let reg = state.registry.lock().unwrap();
                let Some(entry) = reg.hosts.get(&host_id) else {
                    break;
                };
                let mut framed = Vec::with_capacity(data.len() + 9);
                framed.push(TO_HOST_DATA);
                framed.extend(conn.to_be_bytes());
                framed.extend_from_slice(&data);
                if entry.tx.send(Message::Binary(framed.into())).is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            Message::Text(_) => break,
            _ => {}
        }
    }
    {
        let mut reg = state.registry.lock().unwrap();
        if let Some(e) = reg.hosts.get_mut(&host_id) {
            if e.clients.remove(&conn).is_some() {
                let mut note = vec![TO_HOST_GONE];
                note.extend(conn.to_be_bytes());
                let _ = e.tx.send(Message::Binary(note.into()));
            }
        }
    }
    drop(tx);
    out.abort();
}

/// Token bucket: `rate` frames per second sustained, burst of one second's worth.
struct Bucket {
    rate: f64,
    tokens: f64,
    last: Instant,
}

impl Bucket {
    fn new(rate: u32) -> Self {
        Self {
            rate: rate as f64,
            tokens: rate as f64,
            last: Instant::now(),
        }
    }

    fn take(&mut self) -> bool {
        let now = Instant::now();
        self.tokens = (self.tokens + (now - self.last).as_secs_f64() * self.rate).min(self.rate);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests;
