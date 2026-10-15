//! End to end: a fake phone talks Noise through a real relay to the host module, which
//! serves a real PTY terminal through the bridge.

use super::bridge::{self, PaneView};
use super::host::{self, Ctx, Offer};
use super::store;
use crate::config::Config;
use crate::term::Terminal;
use futures_util::{SinkExt, StreamExt};
use remote_proto::msg::{ClientMsg, DataKind, HostMsg, Packet, PaneInfo};
use remote_proto::noise::{self, Keypair, Mode, Session};
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

struct View(Terminal);

impl PaneView for View {
    fn panes(&self) -> Vec<PaneInfo> {
        vec![PaneInfo {
            uid: self.0.pane_uid,
            title: "test".into(),
            workspace: "ws".into(),
            cols: self.0.cols.get(),
            rows: self.0.rows.get(),
            state: None,
            unread: false,
        }]
    }

    fn terminal(&self, uid: u64) -> Option<&Terminal> {
        (self.0.pane_uid == uid).then_some(&self.0)
    }
}

/// Owns a terminal on its own thread and services the bridge, like a GUI event loop.
fn spawn_gui(dir: &std::path::Path) -> (u64, Arc<std::sync::atomic::AtomicBool>) {
    let script = dir.join("fake-shell.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf 'ready\\n'\nread l\nprintf 'got:%s\\n' \"$l\"\nsleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = Config {
        shell: script.to_string_lossy().into_owned(),
        ..Config::default()
    };
    let cwd = dir.to_path_buf();
    let (uid_tx, uid_rx) = std::sync::mpsc::channel();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    std::thread::spawn(move || {
        let terminal = Terminal::new(
            &config,
            80,
            24,
            8,
            16,
            Arc::new(|| {}),
            Arc::new(crate::app::pty_schedule::WakeupGate::new()),
            Some(cwd),
        )
        .unwrap();
        uid_tx.send(terminal.pane_uid).unwrap();
        let view = View(terminal);
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            bridge::service(&view);
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    (uid_rx.recv().unwrap(), stop)
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Phone {
    ws: Ws,
    session: Session,
}

impl Phone {
    async fn connect(
        relay: &str,
        host: &Keypair,
        me: &Keypair,
        mode: Mode,
    ) -> Result<Phone, String> {
        let id = remote_proto::endpoint_id(&host.public);
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("{relay}/client/{id}"))
            .await
            .map_err(|e| e.to_string())?;
        let (init, first) = noise::initiate(me, &host.public, mode).map_err(|e| e.to_string())?;
        ws.send(Message::Binary(first.into())).await.unwrap();
        let reply = match recv_raw(&mut ws).await {
            Some(r) => r,
            None => return Err("closed before reply".into()),
        };
        let session = init.finish(&reply).map_err(|e| e.to_string())?;
        Ok(Phone { ws, session })
    }

    async fn send(&mut self, packet: Packet) {
        for frame in self.session.seal(&packet.encode().unwrap()).unwrap() {
            self.ws.send(Message::Binary(frame.into())).await.unwrap();
        }
    }

    /// Next packet from the host, or None if the connection closed.
    async fn recv(&mut self) -> Option<Packet> {
        loop {
            let frame = recv_raw(&mut self.ws).await?;
            if let Some(msg) = self.session.open(&frame).ok()? {
                return Some(Packet::decode_from_host(&msg).unwrap());
            }
        }
    }

    async fn hello(&mut self) -> Option<Packet> {
        self.send(Packet::Client(ClientMsg::Hello {
            version: remote_proto::PROTOCOL_VERSION,
            name: "test phone".into(),
        }))
        .await;
        self.recv().await
    }
}

async fn recv_raw(ws: &mut Ws) -> Option<Vec<u8>> {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
            Ok(Some(Ok(Message::Binary(b)))) => return Some(b.to_vec()),
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => return None,
            Ok(Some(Ok(_))) => continue,
        }
    }
}

struct Rig {
    relay: String,
    host: Keypair,
    ctx: Arc<Ctx>,
    uid: u64,
    stop: Arc<std::sync::atomic::AtomicBool>,
    _tmp: tempfile::TempDir,
    // The bridge queue is process-global: one rig at a time.
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn rig() -> Rig {
    let serial = SERIAL.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("ws://{}", listener.local_addr().unwrap());
    tokio::spawn(relay::serve(listener, relay::Limits::default()));
    let dir = tmp.path().join("remote");
    let host = store::load_or_create_identity(&dir).unwrap();
    let (uid, stop) = spawn_gui(tmp.path());
    let ctx = Ctx::new(
        dir,
        host.clone(),
        relay.clone(),
        "test-host".into(),
        Arc::new(|| {}),
    );
    let (_keep, shutdown_rx) = tokio::sync::watch::channel(false);
    std::mem::forget(_keep);
    tokio::spawn(host::run(Arc::clone(&ctx), shutdown_rx));
    // Wait for the host to register with the relay.
    for _ in 0..100 {
        if ctx
            .relay_connected
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    Rig {
        relay,
        host,
        ctx,
        uid,
        stop,
        _tmp: tmp,
        _serial: serial,
    }
}

fn offer(ctx: &Ctx, secret: [u8; 32], ttl: i64) {
    *ctx.offer.lock().unwrap() = Some(Offer {
        secret,
        expires: (store::now_unix() as i64 + ttl) as u64,
        failures: 0,
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pair_list_attach_stream_and_type() {
    let r = rig().await;
    let me = noise::generate_keypair().unwrap();
    let secret = remote_proto::random32().unwrap();
    offer(&r.ctx, secret, 60);

    let mut phone = Phone::connect(&r.relay, &r.host, &me, Mode::Pair { secret })
        .await
        .unwrap();
    match phone.hello().await {
        Some(Packet::Host(HostMsg::Welcome {
            host, read_only, ..
        })) => {
            assert_eq!(host, "test-host");
            assert!(!read_only);
        }
        other => panic!("expected welcome, got {other:?}"),
    }
    assert!(
        store::find_device(&r.ctx.dir, &me.public).is_some(),
        "device registered"
    );
    assert!(r.ctx.offer.lock().unwrap().is_none(), "offer is single use");

    phone.send(Packet::Client(ClientMsg::List)).await;
    match phone.recv().await {
        Some(Packet::Host(HostMsg::Panes { panes })) => {
            assert_eq!(panes.len(), 1);
            assert_eq!(panes[0].uid, r.uid);
        }
        other => panic!("expected panes, got {other:?}"),
    }

    phone
        .send(Packet::Client(ClientMsg::Attach { pane: r.uid }))
        .await;
    match phone.recv().await {
        Some(Packet::Host(HostMsg::Attached { pane, cols, rows })) => {
            assert_eq!((pane, cols, rows), (r.uid, 80, 24));
        }
        other => panic!("expected attached, got {other:?}"),
    }
    // Snapshot first, then whatever else; typing must reach the shell and come back.
    let mut saw_snapshot = false;
    let mut text = String::new();
    phone
        .send(Packet::Input {
            pane: r.uid,
            bytes: b"hello\n".to_vec(),
        })
        .await;
    while !text.contains("got:hello") {
        match phone.recv().await {
            Some(Packet::Data {
                kind: DataKind::Snapshot,
                bytes,
                ..
            }) => {
                assert!(!saw_snapshot, "exactly one snapshot");
                saw_snapshot = true;
                text.push_str(&String::from_utf8_lossy(&bytes));
            }
            Some(Packet::Data {
                kind: DataKind::Output,
                bytes,
                ..
            }) => {
                assert!(saw_snapshot, "snapshot precedes output");
                text.push_str(&String::from_utf8_lossy(&bytes));
            }
            Some(other) => panic!("unexpected {other:?}"),
            None => panic!("closed; got so far: {text:?}"),
        }
    }
    drop(phone);

    // The device is now known: a plain reconnect works without any offer.
    let mut again = Phone::connect(&r.relay, &r.host, &me, Mode::Reconnect)
        .await
        .unwrap();
    assert!(matches!(
        again.hello().await,
        Some(Packet::Host(HostMsg::Welcome { .. }))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_device_without_pairing_is_refused() {
    let r = rig().await;
    let stranger = noise::generate_keypair().unwrap();
    let result = Phone::connect(&r.relay, &r.host, &stranger, Mode::Reconnect).await;
    assert!(result.is_err(), "host must not answer unknown devices");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_or_expired_secret_never_registers_a_device() {
    let r = rig().await;
    let me = noise::generate_keypair().unwrap();
    let good = remote_proto::random32().unwrap();
    offer(&r.ctx, good, 60);
    let wrong = remote_proto::random32().unwrap();
    assert!(
        Phone::connect(&r.relay, &r.host, &me, Mode::Pair { secret: wrong })
            .await
            .is_err()
    );

    offer(&r.ctx, good, -5);
    let result = Phone::connect(&r.relay, &r.host, &me, Mode::Pair { secret: good }).await;
    assert!(result.is_err(), "expired offer is not even answered");
    assert!(store::load_devices(&r.ctx.dir).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn read_only_device_cannot_type_and_unattached_panes_refuse_input() {
    let r = rig().await;
    let me = noise::generate_keypair().unwrap();
    store::add_device(&r.ctx.dir, "ro", &me.public).unwrap();
    store::set_read_only(&r.ctx.dir, &remote_proto::hex(&me.public)[..8], true).unwrap();

    let mut phone = Phone::connect(&r.relay, &r.host, &me, Mode::Reconnect)
        .await
        .unwrap();
    assert!(matches!(
        phone.hello().await,
        Some(Packet::Host(HostMsg::Welcome {
            read_only: true,
            ..
        }))
    ));
    phone
        .send(Packet::Client(ClientMsg::Attach { pane: r.uid }))
        .await;
    let _ = phone.recv().await; // attached
    phone
        .send(Packet::Input {
            pane: r.uid,
            bytes: b"x\n".to_vec(),
        })
        .await;
    loop {
        match phone.recv().await {
            Some(Packet::Host(HostMsg::Error { message })) => {
                assert!(message.contains("read-only"));
                break;
            }
            Some(_) => continue,
            None => panic!("closed"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn input_to_a_pane_that_was_not_attached_is_refused() {
    let r = rig().await;
    let me = noise::generate_keypair().unwrap();
    store::add_device(&r.ctx.dir, "rw", &me.public).unwrap();
    let mut phone = Phone::connect(&r.relay, &r.host, &me, Mode::Reconnect)
        .await
        .unwrap();
    phone.hello().await;
    phone
        .send(Packet::Input {
            pane: r.uid,
            bytes: b"x\n".to_vec(),
        })
        .await;
    match phone.recv().await {
        Some(Packet::Host(HostMsg::Error { message })) => assert!(message.contains("refused")),
        other => panic!("expected error, got {other:?}"),
    }
}
