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
use remote_proto::pair::PairingCode;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// How long a pairing code stays valid.
pub const OFFER_TTL: Duration = Duration::from_secs(120);

pub type Wakeup = Arc<dyn Fn() + Send + Sync>;

struct Running {
    runtime: tokio::runtime::Runtime,
    shutdown: tokio::sync::watch::Sender<bool>,
    ctx: Arc<host::Ctx>,
}

#[derive(Default)]
pub struct Manager {
    config: Mutex<RemoteConfig>,
    wakeup: Mutex<Option<Wakeup>>,
    running: Mutex<Option<Running>>,
}

/// Snapshot for status displays (palette / indicator wiring).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Info {
    pub active: bool,
    pub relay_connected: bool,
    pub clients: usize,
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

#[cfg_attr(not(test), allow(dead_code))]
impl Manager {
    pub fn info(&self) -> Info {
        match self.running.lock().unwrap().as_ref() {
            Some(r) => Info {
                active: true,
                relay_connected: r.ctx.relay_connected.load(Ordering::Relaxed),
                clients: r.ctx.clients.load(Ordering::Relaxed),
            },
            None => Info::default(),
        }
    }

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
        let ctx = host::Ctx::new(
            dir.clone(),
            identity.clone(),
            config.relay.clone(),
            host_name,
            wakeup,
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("remote")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
        runtime.spawn(host::run(Arc::clone(&ctx), shutdown_rx));
        store::write_status(
            &dir,
            &store::Status {
                pid: std::process::id(),
                host_public: remote_proto::hex(&identity.public),
                relay_public: config.relay_public.clone().unwrap_or(config.relay),
                socket: crate::ipc::server::socket_path().map(|p| p.to_string_lossy().into_owned()),
            },
        )
        .map_err(|e| e.to_string())?;
        *running = Some(Running {
            runtime,
            shutdown,
            ctx,
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

    /// Mint a pairing code valid for [`OFFER_TTL`]; replaces any previous offer.
    pub fn new_offer(&self, relay_override: Option<&str>) -> Result<PairingCode, String> {
        let secret = remote_proto::random32().map_err(|e| e.to_string())?;
        let expires = store::now_unix() + OFFER_TTL.as_secs();
        self.install_offer(secret, expires)?;
        let running = self.running.lock().unwrap();
        let ctx = &running.as_ref().ok_or("remote access is not running")?.ctx;
        let config = self.config.lock().unwrap().clone();
        Ok(PairingCode {
            host_public: ctx.identity.public,
            secret,
            expires_unix: expires,
            relay: relay_override
                .map(str::to_string)
                .or(config.relay_public)
                .unwrap_or(config.relay),
        })
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

fn hostname() -> String {
    let mut buf = [0u8; 256];
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0;
    if !ok {
        return "PetruTerm".into();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}
