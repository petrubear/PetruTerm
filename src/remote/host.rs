//! The desktop end of a remote session: dials the relay, runs one Noise session per phone,
//! and bridges requests to the GUI thread.
//!
//! Threat model: the relay and the network are untrusted. A phone is admitted only if its
//! static key is in the device list (reconnect, Noise IK) or it proves knowledge of the
//! one-time pairing secret (IKpsk2). Input is accepted only for panes the phone attached to,
//! and never from read-only devices.

use super::bridge::{self, Attached, Request};
use super::store::{self, Device};
use crate::term::tee::RemoteChunk;
use futures_util::{SinkExt, StreamExt};
use remote_proto::msg::{ClientMsg, DataKind, HostMsg, Packet};
use remote_proto::noise::{self, Keypair, Session};
use remote_proto::PROTOCOL_VERSION;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

// Relay framing, see the `relay` crate.
const FROM_RELAY_DATA: u8 = 1;
const FROM_RELAY_GONE: u8 = 2;
const FROM_RELAY_NEW: u8 = 3;
const TO_RELAY_DATA: u8 = 1;
const TO_RELAY_DROP: u8 = 2;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_UNAUTHENTICATED: usize = 4;
const MAX_PAIRING_FAILURES: u32 = 5;
const MAX_INPUT: usize = 256 * 1024;
const MAX_NAME_CHARS: usize = 40;

pub struct Offer {
    pub secret: [u8; 32],
    pub expires: u64,
    pub failures: u32,
}

pub struct Ctx {
    pub dir: PathBuf,
    pub identity: Keypair,
    /// Relay URL the desktop dials.
    pub relay_url: String,
    pub host_name: String,
    pub offer: Mutex<Option<Offer>>,
    pub clients: AtomicUsize,
    pub relay_connected: AtomicBool,
    pub attention: broadcast::Sender<HostMsg>,
    pub wakeup: Arc<dyn Fn() + Send + Sync>,
    unauthenticated: AtomicUsize,
}

impl Ctx {
    pub fn new(
        dir: PathBuf,
        identity: Keypair,
        relay_url: String,
        host_name: String,
        wakeup: Arc<dyn Fn() + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            dir,
            identity,
            relay_url,
            host_name,
            offer: Mutex::new(None),
            clients: AtomicUsize::new(0),
            relay_connected: AtomicBool::new(false),
            attention: broadcast::channel(64).0,
            wakeup,
            unauthenticated: AtomicUsize::new(0),
        })
    }

    /// The secret of the current pairing offer, if it is still usable.
    fn offer_secret(&self) -> Option<[u8; 32]> {
        let offer = self.offer.lock().unwrap();
        offer
            .as_ref()
            .filter(|o| store::now_unix() < o.expires && o.failures < MAX_PAIRING_FAILURES)
            .map(|o| o.secret)
    }

    fn offer_failed(&self) {
        if let Some(o) = self.offer.lock().unwrap().as_mut() {
            o.failures += 1;
        }
    }

    /// Consume the offer (single use). False when it expired or was already used.
    fn take_offer(&self) -> bool {
        let mut offer = self.offer.lock().unwrap();
        let valid = offer
            .as_ref()
            .is_some_and(|o| store::now_unix() < o.expires && o.failures < MAX_PAIRING_FAILURES);
        if valid {
            *offer = None;
        }
        valid
    }
}

fn relay_frame(kind: u8, conn: u64, payload: &[u8]) -> Message {
    let mut out = Vec::with_capacity(payload.len() + 9);
    out.push(kind);
    out.extend(conn.to_be_bytes());
    out.extend_from_slice(payload);
    Message::Binary(out.into())
}

/// Keep a connection to the relay until `shutdown` flips, reconnecting with backoff.
pub async fn run(ctx: Arc<Ctx>, mut shutdown: watch::Receiver<bool>) {
    let url = format!(
        "{}/host/{}",
        ctx.relay_url.trim_end_matches('/'),
        remote_proto::endpoint_id(&ctx.identity.public)
    );
    let mut backoff = Duration::from_secs(1);
    loop {
        tokio::select! {
            _ = shutdown.changed() => return,
            result = tokio_tungstenite::connect_async(url.as_str()) => match result {
                Ok((ws, _)) => {
                    backoff = Duration::from_secs(1);
                    ctx.relay_connected.store(true, Ordering::Relaxed);
                    log::info!("remote: connected to relay");
                    tokio::select! {
                        _ = shutdown.changed() => return,
                        _ = relay_connection(&ctx, ws) => {}
                    }
                    ctx.relay_connected.store(false, Ordering::Relaxed);
                    log::info!("remote: relay connection lost");
                }
                Err(e) => log::warn!("remote: cannot reach relay: {e}"),
            }
        }
        tokio::select! {
            _ = shutdown.changed() => return,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn relay_connection(
    ctx: &Arc<Ctx>,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut sink, mut stream) = ws.split();
    let (out, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });
    let mut clients: HashMap<u64, mpsc::Sender<Vec<u8>>> = HashMap::new();
    while let Some(Ok(msg)) = stream.next().await {
        let Message::Binary(data) = msg else {
            if matches!(msg, Message::Close(_)) {
                break;
            }
            continue;
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
            FROM_RELAY_NEW => {
                let (tx, rx) = mpsc::channel(256);
                clients.insert(conn, tx);
                tokio::spawn(client_task(Arc::clone(ctx), conn, rx, out.clone()));
            }
            FROM_RELAY_DATA => {
                let overflow = clients
                    .get(&conn)
                    .is_some_and(|tx| tx.try_send(payload.to_vec()).is_err());
                if overflow {
                    clients.remove(&conn);
                    let _ = out.send(relay_frame(TO_RELAY_DROP, conn, &[]));
                }
            }
            FROM_RELAY_GONE => {
                clients.remove(&conn);
            }
            _ => {}
        }
    }
    drop(clients);
    writer.abort();
}

struct CountGuard<'a>(&'a AtomicUsize);

impl<'a> CountGuard<'a> {
    fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self(counter)
    }
}

impl Drop for CountGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn client_task(
    ctx: Arc<Ctx>,
    conn: u64,
    mut inbox: mpsc::Receiver<Vec<u8>>,
    out: mpsc::UnboundedSender<Message>,
) {
    if let Err(e) = run_client(&ctx, conn, &mut inbox, &out).await {
        log::info!("remote: client {conn} ended: {e}");
    }
    let _ = out.send(relay_frame(TO_RELAY_DROP, conn, &[]));
}

type ClientResult<T> = Result<T, String>;

fn send_packet(
    session: &mut Session,
    out: &mpsc::UnboundedSender<Message>,
    conn: u64,
    packet: &Packet,
) -> ClientResult<()> {
    let plain = packet.encode().map_err(|e| e.to_string())?;
    for frame in session.seal(&plain).map_err(|e| e.to_string())? {
        out.send(relay_frame(TO_RELAY_DATA, conn, &frame))
            .map_err(|_| "relay gone".to_string())?;
    }
    Ok(())
}

async fn next_message(
    session: &mut Session,
    inbox: &mut mpsc::Receiver<Vec<u8>>,
) -> ClientResult<Vec<u8>> {
    loop {
        let frame = inbox.recv().await.ok_or("disconnected")?;
        if let Some(msg) = session.open(&frame).map_err(|e| e.to_string())? {
            return Ok(msg);
        }
    }
}

async fn ask<T>(rx: oneshot::Receiver<T>) -> ClientResult<T> {
    match tokio::time::timeout(BRIDGE_TIMEOUT, rx).await {
        Ok(Ok(v)) => Ok(v),
        _ => Err("desktop did not answer".into()),
    }
}

fn sanitize_name(name: &str) -> String {
    let n: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    let n = n.trim();
    if n.is_empty() {
        "phone".into()
    } else {
        n.into()
    }
}

async fn run_client(
    ctx: &Arc<Ctx>,
    conn: u64,
    inbox: &mut mpsc::Receiver<Vec<u8>>,
    out: &mpsc::UnboundedSender<Message>,
) -> ClientResult<()> {
    // Handshake phase: bounded in number and time, since anyone reaching the relay can start one.
    if ctx.unauthenticated.load(Ordering::Relaxed) >= MAX_UNAUTHENTICATED {
        return Err("too many unauthenticated connections".into());
    }
    let (mut session, device, pairing) = {
        let _guard = CountGuard::new(&ctx.unauthenticated);
        let handshake = async {
            let first = inbox.recv().await.ok_or("disconnected")?;
            let responder = noise::respond_read(&ctx.identity, &first, ctx.offer_secret())
                .map_err(|e| e.to_string())?;
            let remote = responder.remote_static();
            let pairing = responder.is_pairing();
            let known = store::find_device(&ctx.dir, &remote);
            if !pairing && known.is_none() {
                return Err::<_, String>("unknown device".into());
            }
            let (reply, mut session) = responder.reply().map_err(|e| e.to_string())?;
            out.send(relay_frame(TO_RELAY_DATA, conn, &reply))
                .map_err(|_| "relay gone".to_string())?;
            let hello = match next_message(&mut session, inbox).await {
                Ok(m) => m,
                Err(e) => {
                    if pairing {
                        ctx.offer_failed();
                    }
                    return Err(e);
                }
            };
            let Packet::Client(ClientMsg::Hello { version, name }) =
                Packet::decode_from_client(&hello).map_err(|e| e.to_string())?
            else {
                return Err("first message must be hello".into());
            };
            if version != PROTOCOL_VERSION {
                let _ = send_packet(
                    &mut session,
                    out,
                    conn,
                    &Packet::Host(HostMsg::Error {
                        message: format!("unsupported protocol version {version}"),
                    }),
                );
                return Err("protocol version mismatch".into());
            }
            let device = if pairing {
                if !ctx.take_offer() {
                    return Err("pairing offer expired or already used".into());
                }
                store::add_device(&ctx.dir, &sanitize_name(&name), &remote)
                    .map_err(|e| e.to_string())?
            } else {
                known.unwrap()
            };
            Ok((session, device, pairing))
        };
        tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake)
            .await
            .map_err(|_| "handshake timed out".to_string())??
    };
    let remote: [u8; 32] = remote_proto::unhex(&device.public).ok_or("bad stored key")?;
    log::info!(
        "remote: {} {} ({})",
        if pairing { "paired" } else { "connected" },
        device.name,
        &device.public[..8]
    );
    let _clients = CountGuard::new(&ctx.clients);
    send_packet(
        &mut session,
        out,
        conn,
        &Packet::Host(HostMsg::Welcome {
            version: PROTOCOL_VERSION,
            host: ctx.host_name.clone(),
            read_only: device.read_only,
        }),
    )?;
    let result = session_loop(ctx, conn, inbox, out, session, device, remote).await;
    log::info!(
        "remote: device {} disconnected",
        &remote_proto::hex(&remote)[..8]
    );
    result
}

enum Pumped {
    Chunk(RemoteChunk),
    Closed,
}

async fn session_loop(
    ctx: &Arc<Ctx>,
    conn: u64,
    inbox: &mut mpsc::Receiver<Vec<u8>>,
    out: &mpsc::UnboundedSender<Message>,
    mut session: Session,
    mut device: Device,
    remote: [u8; 32],
) -> ClientResult<()> {
    let mut attachments: HashMap<u64, Arc<AtomicBool>> = HashMap::new();
    let (pump_tx, mut pump_rx) = mpsc::channel::<(u64, Pumped)>(256);
    let mut attention = ctx.attention.subscribe();
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    tick.tick().await;

    let result = loop {
        tokio::select! {
            frame = inbox.recv() => {
                let Some(frame) = frame else { break Ok(()) };
                let msg = match session.open(&frame) {
                    Ok(Some(m)) => m,
                    Ok(None) => continue,
                    Err(e) => break Err(e.to_string()),
                };
                let packet = match Packet::decode_from_client(&msg) {
                    Ok(p) => p,
                    Err(e) => break Err(e.to_string()),
                };
                if let Err(e) = handle_packet(
                    ctx, conn, out, &mut session, &device, packet, &mut attachments, &pump_tx,
                ).await {
                    break Err(e);
                }
            }
            Some((pane, pumped)) = pump_rx.recv() => {
                let sent = match pumped {
                    Pumped::Chunk(chunk) => {
                        let (kind, bytes) = match chunk {
                            RemoteChunk::Snapshot(b) => (DataKind::Snapshot, b),
                            RemoteChunk::Output(b) => (DataKind::Output, b),
                        };
                        send_packet(&mut session, out, conn, &Packet::Data { pane, kind, bytes })
                    }
                    Pumped::Closed => {
                        attachments.remove(&pane);
                        send_packet(&mut session, out, conn, &Packet::Host(HostMsg::Detached {
                            pane,
                            reason: "pane closed or the connection fell behind".into(),
                        }))
                    }
                };
                if let Err(e) = sent { break Err(e); }
            }
            msg = attention.recv() => {
                if let Ok(m) = msg {
                    if let Err(e) = send_packet(&mut session, out, conn, &Packet::Host(m)) {
                        break Err(e);
                    }
                }
            }
            _ = tick.tick() => {
                match store::find_device(&ctx.dir, &remote) {
                    None => break Err("device revoked".into()),
                    Some(d) => {
                        device.read_only = d.read_only;
                        store::touch_device(&ctx.dir, &remote);
                    }
                }
            }
        }
    };
    for stop in attachments.values() {
        stop.store(true, Ordering::Relaxed);
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn handle_packet(
    ctx: &Arc<Ctx>,
    conn: u64,
    out: &mpsc::UnboundedSender<Message>,
    session: &mut Session,
    device: &Device,
    packet: Packet,
    attachments: &mut HashMap<u64, Arc<AtomicBool>>,
    pump_tx: &mpsc::Sender<(u64, Pumped)>,
) -> ClientResult<()> {
    let reply =
        |session: &mut Session, m: HostMsg| send_packet(session, out, conn, &Packet::Host(m));
    match packet {
        Packet::Client(ClientMsg::Hello { .. }) => {}
        Packet::Client(ClientMsg::Ping) => reply(session, HostMsg::Pong)?,
        Packet::Client(ClientMsg::List) => {
            let (tx, rx) = oneshot::channel();
            if !bridge::submit(Request::List(tx)) {
                return reply(
                    session,
                    HostMsg::Error {
                        message: "desktop busy".into(),
                    },
                );
            }
            (ctx.wakeup)();
            let panes = ask(rx).await?;
            reply(session, HostMsg::Panes { panes })?;
        }
        Packet::Client(ClientMsg::Attach { pane }) => {
            if attachments.contains_key(&pane) {
                return Ok(());
            }
            let (tx, rx) = oneshot::channel();
            if !bridge::submit(Request::Attach { pane, reply: tx }) {
                return reply(
                    session,
                    HostMsg::Error {
                        message: "desktop busy".into(),
                    },
                );
            }
            (ctx.wakeup)();
            match ask(rx).await? {
                Ok(Attached { cols, rows, rx }) => {
                    // Announce first so the snapshot that follows is understood.
                    reply(session, HostMsg::Attached { pane, cols, rows })?;
                    attachments.insert(pane, spawn_pump(pane, rx, pump_tx.clone()));
                }
                Err(message) => reply(session, HostMsg::Error { message })?,
            }
        }
        Packet::Client(ClientMsg::Detach { pane }) => {
            if let Some(stop) = attachments.remove(&pane) {
                stop.store(true, Ordering::Relaxed);
            }
            reply(
                session,
                HostMsg::Detached {
                    pane,
                    reason: "detached".into(),
                },
            )?;
        }
        Packet::Input { pane, bytes } => {
            if device.read_only {
                return reply(
                    session,
                    HostMsg::Error {
                        message: "this device is read-only".into(),
                    },
                );
            }
            if !attachments.contains_key(&pane) || bytes.len() > MAX_INPUT {
                return reply(
                    session,
                    HostMsg::Error {
                        message: "input refused".into(),
                    },
                );
            }
            if bridge::submit(Request::Input { pane, bytes }) {
                (ctx.wakeup)();
            }
        }
        Packet::Host(_) | Packet::Data { .. } => return Err("unexpected packet".into()),
    }
    Ok(())
}

/// Move chunks from the (blocking) tee receiver to the async session. The pump stops when
/// `stop` is set or the session is gone; a full async queue blocks it, which fills the tee
/// queue, which then detaches this subscriber instead of stalling the PTY.
fn spawn_pump(
    pane: u64,
    rx: crossbeam_channel::Receiver<RemoteChunk>,
    tx: mpsc::Sender<(u64, Pumped)>,
) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let _ = std::thread::Builder::new()
        .name("remote-pump".into())
        .spawn(move || loop {
            if flag.load(Ordering::Relaxed) {
                return;
            }
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(chunk) => {
                    if tx.blocking_send((pane, Pumped::Chunk(chunk))).is_err() {
                        return;
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    let _ = tx.blocking_send((pane, Pumped::Closed));
                    return;
                }
            }
        });
    stop
}
