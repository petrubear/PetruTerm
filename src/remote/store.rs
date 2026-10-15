//! On-disk state for remote access, in `~/.local/state/petruterm/remote/` (dir 0700, files
//! 0600): the host identity key, the paired-device list and the live status file.
//!
//! The host private key is stored in a 0600 file. Keychain storage would be stronger and is
//! a possible follow-up; an attacker who can read this file already runs as the user.

use remote_proto::noise::{generate_keypair, Keypair};
use serde::{Deserialize, Serialize};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub fn dir() -> Option<PathBuf> {
    Some(crate::ipc::server::state_dir()?.join("remote"))
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn ensure_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Write `data` to `path` atomically with mode 0600.
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(tmp, path)
}

/// Load the host identity, creating it on first use.
pub fn load_or_create_identity(dir: &Path) -> io::Result<Keypair> {
    ensure_dir(dir)?;
    let path = dir.join("host.key");
    if let Ok(raw) = std::fs::read(&path) {
        if raw.len() == 64 {
            return Ok(Keypair {
                private: raw[..32].try_into().unwrap(),
                public: raw[32..].try_into().unwrap(),
            });
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "host.key is corrupt; delete it to create a new identity (devices must re-pair)",
        ));
    }
    let kp = generate_keypair().map_err(|e| io::Error::other(e.to_string()))?;
    let mut raw = kp.private.to_vec();
    raw.extend_from_slice(&kp.public);
    write_private(&path, &raw)?;
    Ok(kp)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    /// Hex of the device's static public key.
    pub public: String,
    pub read_only: bool,
    pub added: u64,
    pub last_seen: u64,
}

pub fn load_devices(dir: &Path) -> Vec<Device> {
    std::fs::read(dir.join("devices.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

pub fn save_devices(dir: &Path, devices: &[Device]) -> io::Result<()> {
    ensure_dir(dir)?;
    let raw = serde_json::to_vec_pretty(devices)?;
    write_private(&dir.join("devices.json"), &raw)
}

pub fn find_device(dir: &Path, public: &[u8; 32]) -> Option<Device> {
    let hex = remote_proto::hex(public);
    load_devices(dir).into_iter().find(|d| d.public == hex)
}

/// Register a device (or refresh the name of an already known key).
pub fn add_device(dir: &Path, name: &str, public: &[u8; 32]) -> io::Result<Device> {
    let hex = remote_proto::hex(public);
    let mut devices = load_devices(dir);
    let now = now_unix();
    if let Some(existing) = devices.iter_mut().find(|d| d.public == hex) {
        existing.name = name.to_string();
        existing.last_seen = now;
        let device = existing.clone();
        save_devices(dir, &devices)?;
        return Ok(device);
    }
    let device = Device {
        name: name.to_string(),
        public: hex,
        read_only: false,
        added: now,
        last_seen: now,
    };
    devices.push(device.clone());
    save_devices(dir, &devices)?;
    Ok(device)
}

pub fn touch_device(dir: &Path, public: &[u8; 32]) {
    let hex = remote_proto::hex(public);
    let mut devices = load_devices(dir);
    if let Some(d) = devices.iter_mut().find(|d| d.public == hex) {
        d.last_seen = now_unix();
        let _ = save_devices(dir, &devices);
    }
}

/// Remove the device whose key starts with `prefix` (at least 4 hex digits, unambiguous).
pub fn revoke_device(dir: &Path, prefix: &str) -> Result<Device, String> {
    if prefix.len() < 4 {
        return Err("give at least 4 characters of the device id".into());
    }
    let mut devices = load_devices(dir);
    let matches: Vec<usize> = devices
        .iter()
        .enumerate()
        .filter(|(_, d)| d.public.starts_with(&prefix.to_ascii_lowercase()))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [] => Err("no such device".into()),
        [i] => {
            let device = devices.remove(*i);
            save_devices(dir, &devices).map_err(|e| e.to_string())?;
            Ok(device)
        }
        _ => Err("ambiguous device id".into()),
    }
}

pub fn set_read_only(dir: &Path, prefix: &str, read_only: bool) -> Result<Device, String> {
    let mut devices = load_devices(dir);
    let matches: Vec<usize> = devices
        .iter()
        .enumerate()
        .filter(|(_, d)| prefix.len() >= 4 && d.public.starts_with(&prefix.to_ascii_lowercase()))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [] => Err("no such device".into()),
        [i] => {
            devices[*i].read_only = read_only;
            let device = devices[*i].clone();
            save_devices(dir, &devices).map_err(|e| e.to_string())?;
            Ok(device)
        }
        _ => Err("ambiguous device id".into()),
    }
}

/// What `petruterm remote status|pair` need to know about the running instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub pid: u32,
    pub host_public: String,
    /// Relay URL phones should use (may differ from the one the desktop dials).
    pub relay_public: String,
    pub socket: Option<String>,
}

pub fn write_status(dir: &Path, status: &Status) -> io::Result<()> {
    ensure_dir(dir)?;
    write_private(
        &dir.join("status.json"),
        &serde_json::to_vec_pretty(status)?,
    )
}

pub fn clear_status(dir: &Path) {
    let _ = std::fs::remove_file(dir.join("status.json"));
}

/// The status of a live instance; a stale file of a dead pid is ignored.
pub fn read_status(dir: &Path) -> Option<Status> {
    let status: Status =
        serde_json::from_slice(&std::fs::read(dir.join("status.json")).ok()?).ok()?;
    let alive = unsafe { libc::kill(status.pid as libc::pid_t, 0) } == 0
        || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    alive.then_some(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_created_once_and_private() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("remote");
        let a = load_or_create_identity(&dir).unwrap();
        let b = load_or_create_identity(&dir).unwrap();
        assert_eq!(a.public, b.public);
        let mode = std::fs::metadata(dir.join("host.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let dmode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(dmode & 0o777, 0o700);
    }

    #[test]
    fn corrupt_identity_is_not_silently_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("host.key"), b"short").unwrap();
        assert!(load_or_create_identity(tmp.path()).is_err());
    }

    #[test]
    fn devices_add_find_revoke() {
        let tmp = tempfile::tempdir().unwrap();
        let key = [0xab; 32];
        assert!(find_device(tmp.path(), &key).is_none());
        add_device(tmp.path(), "phone", &key).unwrap();
        let found = find_device(tmp.path(), &key).unwrap();
        assert_eq!(found.name, "phone");
        assert!(!found.read_only);
        assert!(revoke_device(tmp.path(), "ab").is_err(), "too short");
        assert!(revoke_device(tmp.path(), "ffff").is_err());
        revoke_device(tmp.path(), "abab").unwrap();
        assert!(find_device(tmp.path(), &key).is_none());
    }

    #[test]
    fn read_only_flag_persists() {
        let tmp = tempfile::tempdir().unwrap();
        add_device(tmp.path(), "p", &[1; 32]).unwrap();
        set_read_only(tmp.path(), "0101", true).unwrap();
        assert!(find_device(tmp.path(), &[1; 32]).unwrap().read_only);
    }

    #[test]
    fn stale_status_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let mut status = Status {
            pid: std::process::id(),
            host_public: "aa".into(),
            relay_public: "ws://x".into(),
            socket: None,
        };
        write_status(tmp.path(), &status).unwrap();
        assert!(read_status(tmp.path()).is_some());
        status.pid = 999_999;
        write_status(tmp.path(), &status).unwrap();
        assert!(read_status(tmp.path()).is_none());
    }
}
