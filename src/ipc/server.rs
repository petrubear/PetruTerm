//! Unix-socket listener for [`super::Message`]s. One socket per process, in
//! `~/.local/state/petruterm/` (dir 0700, socket 0600), named by pid. A plain thread per
//! connection is enough: connections are short-lived and rare (one per hook invocation).

use super::{parse_line, Message, MAX_LINE_BYTES};
use crossbeam_channel::{Receiver, Sender};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// `sun_path` is 104 bytes on macOS (including the NUL).
const MAX_SOCKET_PATH: usize = 100;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const QUEUE_CAP: usize = 256;

static SOCKET: OnceLock<PathBuf> = OnceLock::new();
static RX: OnceLock<Receiver<Message>> = OnceLock::new();

pub fn state_dir() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".local/state/petruterm"))
}

pub fn socket_name(pid: u32) -> String {
    format!("petruterm-{pid}.sock")
}

/// Path of this process's socket once [`start`] succeeded.
pub fn socket_path() -> Option<&'static Path> {
    SOCKET.get().map(PathBuf::as_path)
}

/// Messages received since the last call. Empty when the server is not running.
pub fn drain() -> Vec<Message> {
    RX.get()
        .map(|rx| rx.try_iter().collect())
        .unwrap_or_default()
}

/// Start the listener (idempotent). `wakeup` is called after each queued message so an idle
/// event loop picks it up. Failures are logged and leave the app without a control channel.
pub fn start(wakeup: Arc<dyn Fn() + Send + Sync>) {
    if SOCKET.get().is_some() {
        return;
    }
    let Some(dir) = state_dir() else { return };
    if let Err(e) = start_in(&dir, wakeup) {
        log::warn!("control socket disabled: {e}");
    }
}

fn start_in(dir: &Path, wakeup: Arc<dyn Fn() + Send + Sync>) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    remove_stale_sockets(dir);
    let path = dir.join(socket_name(std::process::id()));
    if path.as_os_str().len() > MAX_SOCKET_PATH {
        return Err(std::io::Error::other("socket path too long"));
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let (tx, rx) = crossbeam_channel::bounded(QUEUE_CAP);
    let _ = RX.set(rx);
    let _ = SOCKET.set(path);
    std::thread::Builder::new()
        .name("ipc-accept".into())
        .spawn(move || accept_loop(listener, tx, wakeup))?;
    Ok(())
}

fn accept_loop(listener: UnixListener, tx: Sender<Message>, wakeup: Arc<dyn Fn() + Send + Sync>) {
    for stream in listener.incoming().flatten() {
        let tx = tx.clone();
        let wakeup = Arc::clone(&wakeup);
        let _ = std::thread::Builder::new()
            .name("ipc-conn".into())
            .spawn(move || handle_connection(stream, &tx, wakeup.as_ref()));
    }
}

fn handle_connection(stream: UnixStream, tx: &Sender<Message>, wakeup: &(dyn Fn() + Send + Sync)) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = Vec::new();
        // Cap each line; a line that hits the cap without a newline is hostile or broken.
        let n = reader
            .by_ref()
            .take(MAX_LINE_BYTES as u64 + 1)
            .read_until(b'\n', &mut line);
        match n {
            Ok(0) | Err(_) => return,
            Ok(_) if line.len() > MAX_LINE_BYTES => return,
            Ok(_) => {}
        }
        if let Some(msg) = std::str::from_utf8(&line).ok().and_then(parse_line) {
            if tx.try_send(msg).is_ok() {
                wakeup();
            }
        }
    }
}

/// Remove `petruterm-<pid>.sock` files whose process no longer exists.
fn remove_stale_sockets(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.strip_prefix("petruterm-"))
            .and_then(|n| n.strip_suffix(".sock"))
            .and_then(|n| n.parse::<i32>().ok())
        else {
            continue;
        };
        // SAFETY: signal 0 only checks existence/permission.
        let alive = unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !alive {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Send one message to the socket at `path`.
pub fn send_to(path: &Path, msg: &Message) -> std::io::Result<()> {
    use std::io::Write;
    let mut stream = UnixStream::connect(path)?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut line = serde_json::to_vec(msg).map_err(std::io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn serve(dir: &Path) -> (PathBuf, Receiver<Message>) {
        let path = dir.join("t.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (tx, rx) = crossbeam_channel::bounded(QUEUE_CAP);
        std::thread::spawn(move || accept_loop(listener, tx, Arc::new(|| {})));
        (path, rx)
    }

    #[test]
    fn delivers_valid_lines_and_skips_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let (path, rx) = serve(dir.path());
        let mut s = UnixStream::connect(&path).unwrap();
        s.write_all(b"garbage\n{\"pane\":1,\"state\":\"done\"}\n\n{\"pane\":2}\n")
            .unwrap();
        drop(s);
        let a = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let b = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!((a.pane, b.pane), (1, 2));
    }

    #[test]
    fn send_to_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let (path, rx) = serve(dir.path());
        let msg = Message {
            pane: 9,
            state: Some("working".into()),
            ..Default::default()
        };
        send_to(&path, &msg).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), msg);
    }

    #[test]
    fn oversized_line_drops_the_connection() {
        let dir = tempfile::tempdir().unwrap();
        let (path, rx) = serve(dir.path());
        let mut s = UnixStream::connect(&path).unwrap();
        let big = vec![b'a'; MAX_LINE_BYTES * 2];
        let _ = s.write_all(&big);
        let _ = s.write_all(b"\n{\"pane\":1}\n");
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    }

    #[test]
    fn stale_sockets_of_dead_pids_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let dead = dir.path().join(socket_name(i32::MAX as u32));
        let mine = dir.path().join(socket_name(std::process::id()));
        let other = dir.path().join("notes.txt");
        for p in [&dead, &mine, &other] {
            std::fs::write(p, b"").unwrap();
        }
        remove_stale_sockets(dir.path());
        assert!(!dead.exists());
        assert!(mine.exists());
        assert!(other.exists());
    }
}
