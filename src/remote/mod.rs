//! Remote access: use this desktop's panes from a phone. Started on demand only (command,
//! palette or keybind), never at launch. See
//! `docs/superpowers/specs/2026-10-16-remote-mobile-design.md`.
//!
//! - [`host`]: relay connection and per-phone Noise sessions (tokio, own runtime)
//! - [`bridge`]: hand-off to the GUI thread that owns the terminals
//! - [`store`]: host key, paired devices, status file
//! - [`control`]: `petruterm remote ...` CLI and the control-socket commands

pub mod bridge;
pub mod control;
pub mod host;
pub mod store;

#[cfg(test)]
mod tests;

use crate::config::schema::RemoteConfig;
use remote_proto::msg::HostMsg;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// How long a pairing code stays valid.
pub const OFFER_TTL: Duration = Duration::from_secs(120);

pub type Wakeup = Arc<dyn Fn() + Send + Sync>;

struct Running {
    runtime: tokio::runtime::Runtime,
    shutdown: tokio::sync::watch::Sender<bool>,
    ctx: Arc<host::Ctx>,
    /// Relay address written into pairing codes (read by the gpui binary only).
    #[allow(dead_code)]
    relay_public: String,
}

#[derive(Default)]
pub struct Manager {
    config: Mutex<RemoteConfig>,
    wakeup: Mutex<Option<Wakeup>>,
    running: Mutex<Option<Running>>,
}

static MANAGER: OnceLock<Manager> = OnceLock::new();

pub fn manager() -> &'static Manager {
    MANAGER.get_or_init(Manager::default)
}

/// Provide the settings and the GUI wakeup. Safe to call again on config reload; a running
/// session keeps the relay it started with.
pub fn configure(config: &RemoteConfig, wakeup: Wakeup) {
    let m = manager();
    *m.config.lock().unwrap() = config.clone();
    *m.wakeup.lock().unwrap() = Some(wakeup);
}

/// Forward an agent state change to connected phones. Cheap no-op when remote is off.
pub fn notify_attention(pane_uid: u64, state: &str, message: &str) {
    if let Some(running) = manager().running.lock().unwrap().as_ref() {
        let _ = running.ctx.attention.send(HostMsg::Attention {
            pane: pane_uid,
            state: state.to_string(),
            message: message.to_string(),
        });
    }
}

impl Manager {
    pub fn is_active(&self) -> bool {
        self.running.lock().unwrap().is_some()
    }

    /// Start remote access. Idempotent.
    pub fn start(&self) -> Result<(), String> {
        let dir = store::dir().ok_or("cannot locate the state directory")?;
        self.start_in(dir)
    }

    pub(crate) fn start_in(&self, dir: std::path::PathBuf) -> Result<(), String> {
        let mut running = self.running.lock().unwrap();
        if running.is_some() {
            return Ok(());
        }
        let config = self.config.lock().unwrap().clone();
        let wakeup = self
            .wakeup
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Arc::new(|| {}));
        let identity = store::load_or_create_identity(&dir).map_err(|e| e.to_string())?;
        let host_name = hostname();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("remote")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (relay_dial, relay_public) = start_relay(&runtime, &config);
        let ctx = host::Ctx::new(dir.clone(), identity.clone(), relay_dial, host_name, wakeup);
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
        runtime.spawn(host::run(Arc::clone(&ctx), shutdown_rx));
        store::write_status(
            &dir,
            &store::Status {
                pid: std::process::id(),
                host_public: remote_proto::hex(&identity.public),
                relay_public: relay_public.clone(),
                socket: crate::ipc::server::socket_path().map(|p| p.to_string_lossy().into_owned()),
            },
        )
        .map_err(|e| e.to_string())?;
        *running = Some(Running {
            runtime,
            shutdown,
            ctx,
            relay_public,
        });
        log::info!("remote: started");
        Ok(())
    }

    /// Stop remote access: drops every phone session and the relay connection.
    pub fn stop(&self) {
        if let Some(r) = self.running.lock().unwrap().take() {
            let _ = r.shutdown.send(true);
            store::clear_status(&r.ctx.dir);
            r.runtime.shutdown_background();
            log::info!("remote: stopped");
        }
    }

    #[allow(dead_code)] // used by the gpui binary only
    /// Mint a one-time pairing code for a new phone (starting nothing: remote must be on).
    /// Returns the code and the relay address it points at.
    pub fn new_pairing_code(&self) -> Result<(String, String), String> {
        let (host_public, relay) = {
            let running = self.running.lock().unwrap();
            let r = running.as_ref().ok_or("remote access is not running")?;
            (r.ctx.identity.public, r.relay_public.clone())
        };
        let secret = remote_proto::random32().map_err(|e| e.to_string())?;
        let expires = store::now_unix() + OFFER_TTL.as_secs();
        self.install_offer(secret, expires)?;
        let code = remote_proto::pair::PairingCode {
            host_public,
            secret,
            expires_unix: expires,
            relay: relay.clone(),
        }
        .encode();
        Ok((code, relay))
    }

    /// Accept a pairing secret minted elsewhere (the CLI).
    pub fn install_offer(&self, secret: [u8; 32], expires: u64) -> Result<(), String> {
        let running = self.running.lock().unwrap();
        let ctx = &running.as_ref().ok_or("remote access is not running")?.ctx;
        if expires > store::now_unix() + 600 {
            return Err("pairing offers may not last more than 10 minutes".into());
        }
        *ctx.offer.lock().unwrap() = Some(host::Offer {
            secret,
            expires,
            failures: 0,
        });
        Ok(())
    }
}

/// Start remote access if it is off, stop it if it is on. Returns the new state.
pub fn toggle() -> Result<bool, String> {
    let m = manager();
    if m.is_active() {
        m.stop();
        Ok(false)
    } else {
        m.start().map(|_| true)
    }
}

/// Decide which relay to use. With `relay = "builtin"` this binds a relay on 0.0.0.0 inside the
/// app (it only forwards ciphertext) and returns `(url the desktop dials, url for phones)`.
/// If the port is already taken (a standalone relay is running) that one is used instead.
fn start_relay(runtime: &tokio::runtime::Runtime, config: &RemoteConfig) -> (String, String) {
    if config.relay != "builtin" && !config.relay.is_empty() {
        let public = config
            .relay_public
            .clone()
            .unwrap_or_else(|| config.relay.clone());
        return (config.relay.clone(), public);
    }
    // Bound with std so this works whether or not the caller is inside a runtime.
    let bound = std::net::TcpListener::bind(("0.0.0.0", config.port)).and_then(|l| {
        l.set_nonblocking(true)?;
        let _guard = runtime.enter();
        tokio::net::TcpListener::from_std(l)
    });
    let port = match bound {
        Ok(listener) => {
            let port = listener
                .local_addr()
                .map(|a| a.port())
                .unwrap_or(config.port);
            runtime.spawn(relay::serve(listener, relay::Limits::default()));
            log::info!("remote: built-in relay listening on 0.0.0.0:{port}");
            port
        }
        Err(e) => {
            log::warn!(
                "remote: port {} busy ({e}); using the relay already there",
                config.port
            );
            config.port
        }
    };
    let public = config.relay_public.clone().unwrap_or_else(|| {
        format!(
            "ws://{}:{port}",
            lan_ip().unwrap_or_else(|| "127.0.0.1".into())
        )
    });
    (format!("ws://127.0.0.1:{port}"), public)
}

/// The address of the primary network interface (the route to the internet), if any. No
/// packet is sent: connecting a UDP socket only selects the outgoing interface.
fn lan_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then(|| ip.to_string())
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0;
    if !ok {
        return "PetruTerm".into();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}
