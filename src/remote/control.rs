//! Control of remote access: `petruterm remote ...` and the matching control-socket commands.
//!
//! The CLI never holds secrets of its own: `start`/`stop` are messages to the running app,
//! `pair` mints a one-time secret, hands it to the app and prints the code. Because anything
//! running in a pane (an agent, say) can run `petruterm`, `pair` refuses to print a code unless
//! stdout is a terminal.

use super::{manager, store};
use crate::ipc::{server, Message};
use remote_proto::pair::PairingCode;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Apply a `remote` command received on the control socket.
pub fn apply(msg: &Message) {
    match msg.remote.as_deref() {
        Some("start") => {
            if let Err(e) = manager().start() {
                log::warn!("remote start failed: {e}");
            }
        }
        Some("stop") => manager().stop(),
        Some("offer") => {
            let secret = msg.secret.as_deref().and_then(remote_proto::unhex::<32>);
            match (secret, msg.expires) {
                (Some(secret), Some(expires)) => {
                    if let Err(e) = manager().install_offer(secret, expires) {
                        log::warn!("remote offer refused: {e}");
                    }
                }
                _ => log::warn!("remote offer malformed"),
            }
        }
        other => log::warn!("unknown remote command {other:?}"),
    }
}

const USAGE: &str =
    "usage: petruterm remote <start|stop|status|pair|devices|revoke ID|readonly ID on|off>";

pub fn cli(args: &[String]) -> i32 {
    let Some(dir) = store::dir() else {
        eprintln!("petruterm remote: cannot locate the home directory");
        return 1;
    };
    match args.first().map(String::as_str) {
        Some("start") => start(&dir),
        Some("stop") => stop(&dir),
        Some("status") => status(&dir),
        Some("pair") => pair(&dir, &args[1..]),
        Some("devices") => devices(&dir),
        Some("revoke") => match args.get(1) {
            Some(id) => {
                report(store::revoke_device(&dir, id).map(|d| format!("Revoked {}", d.name)))
            }
            None => usage(),
        },
        Some("readonly") => match (args.get(1), args.get(2).map(String::as_str)) {
            (Some(id), Some(v @ ("on" | "off"))) => {
                report(store::set_read_only(&dir, id, v == "on").map(|d| {
                    format!(
                        "{} is now {}",
                        d.name,
                        if d.read_only {
                            "read-only"
                        } else {
                            "read-write"
                        }
                    )
                }))
            }
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    2
}

fn report(result: Result<String, String>) -> i32 {
    match result {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("petruterm remote: {e}");
            1
        }
    }
}

/// The control socket of a running instance: this pane's, or the newest live one.
fn find_socket() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PETRUTERM_SOCKET") {
        return Some(PathBuf::from(p));
    }
    let dir = server::state_dir()?;
    let mut sockets: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".sock"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    sockets.sort();
    sockets
        .into_iter()
        .rev()
        .map(|(_, p)| p)
        .find(|p| std::os::unix::net::UnixStream::connect(p).is_ok())
}

fn command(name: &str) -> Message {
    Message {
        remote: Some(name.to_string()),
        ..Message::default()
    }
}

fn wait_for(dir: &std::path::Path, want_running: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if store::read_status(dir).is_some() == want_running {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn start(dir: &std::path::Path) -> i32 {
    let Some(sock) = find_socket() else {
        eprintln!("petruterm remote: no running PetruTerm found");
        return 1;
    };
    if server::send_to(&sock, &command("start")).is_err() || !wait_for(dir, true) {
        eprintln!("petruterm remote: could not start remote access");
        return 1;
    }
    println!("Remote access started. Run `petruterm remote pair` to add a phone.");
    0
}

fn stop(dir: &std::path::Path) -> i32 {
    let Some(status) = store::read_status(dir) else {
        println!("Remote access is not running.");
        return 0;
    };
    let sock = status.socket.map(PathBuf::from).or_else(find_socket);
    if let Some(sock) = sock {
        let _ = server::send_to(&sock, &command("stop"));
    }
    if wait_for(dir, false) {
        println!("Remote access stopped.");
        0
    } else {
        eprintln!("petruterm remote: it did not stop");
        1
    }
}

fn status(dir: &std::path::Path) -> i32 {
    match store::read_status(dir) {
        Some(s) => println!(
            "Remote access: running (pid {}), relay {}",
            s.pid, s.relay_public
        ),
        None => println!("Remote access: not running"),
    }
    let n = store::load_devices(dir).len();
    println!("Paired devices: {n}");
    0
}

fn devices(dir: &std::path::Path) -> i32 {
    let devices = store::load_devices(dir);
    if devices.is_empty() {
        println!("No paired devices.");
    }
    for d in devices {
        println!(
            "{}  {}  {}  last seen {}",
            &d.public[..12],
            d.name,
            if d.read_only {
                "read-only"
            } else {
                "read-write"
            },
            if d.last_seen == 0 {
                "never".to_string()
            } else {
                format!("{} s ago", store::now_unix().saturating_sub(d.last_seen))
            },
        );
    }
    0
}

fn pair(dir: &std::path::Path, args: &[String]) -> i32 {
    let mut relay_override = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--relay" => relay_override = it.next().cloned(),
            other => {
                eprintln!("petruterm remote pair: unknown argument `{other}`");
                return 2;
            }
        }
    }
    if !std::io::stdout().is_terminal() {
        eprintln!("petruterm remote pair: refusing to print a pairing code to a non-terminal");
        return 1;
    }
    let Some(status) = store::read_status(dir) else {
        eprintln!("petruterm remote: not running. Run `petruterm remote start` first.");
        return 1;
    };
    let (Some(host_public), Some(sock)) = (
        remote_proto::unhex::<32>(&status.host_public),
        status
            .socket
            .as_ref()
            .map(PathBuf::from)
            .or_else(find_socket),
    ) else {
        eprintln!("petruterm remote: the running instance is not reachable");
        return 1;
    };
    let Ok(secret) = remote_proto::random32() else {
        return 1;
    };
    let expires = store::now_unix() + super::OFFER_TTL.as_secs();
    let offer = Message {
        remote: Some("offer".into()),
        secret: Some(remote_proto::hex(&secret)),
        expires: Some(expires),
        ..Message::default()
    };
    if server::send_to(&sock, &offer).is_err() {
        eprintln!("petruterm remote: could not reach the running instance");
        return 1;
    }
    let relay = relay_override.unwrap_or(status.relay_public);
    let code = PairingCode {
        host_public,
        secret,
        expires_unix: expires,
        relay: relay.clone(),
    }
    .encode();
    print_qr(&code);
    println!("Relay the phone will connect to: {relay}");
    if relay.contains("127.0.0.1") || relay.contains("localhost") {
        println!("Warning: that is this computer's loopback; a phone cannot reach it. Use --relay ws://<this computer's LAN IP>:8787.");
    }
    println!("\n{code}\n");
    println!(
        "Scan the QR or paste the code in the PetruTerm app within {} minutes. It works once.",
        super::OFFER_TTL.as_secs() / 60
    );
    0
}

fn print_qr(data: &str) {
    use qrcode::render::unicode;
    if let Ok(qr) = qrcode::QrCode::new(data.as_bytes()) {
        println!(
            "{}",
            qr.render::<unicode::Dense1x2>().quiet_zone(true).build()
        );
    }
}
