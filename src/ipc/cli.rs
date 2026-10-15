//! Subcommands handled before any GUI starts, shared by both binaries:
//!
//! - `notify [--state S] [--title T] [--body B]`: report state / send a notification
//! - `hook <agent> <event>`: entry point for agent hooks (payload JSON on stdin)
//! - `hooks setup claude [--dry-run]`: install the hooks in `~/.claude/settings.json`
//! - `remote ...`: phone access (see `remote::control`)
//!
//! `notify` and `hook` exit 0 silently when not running inside a PetruTerm pane, so hooks
//! are safe to leave configured everywhere.

use super::{claude_hooks, parse_state, server, Message};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Run a subcommand if `args` (without argv[0]) starts with one. `Some(exit_code)` means the
/// process must exit with that code instead of starting the GUI.
pub fn run(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str)? {
        "notify" => Some(notify(&args[1..])),
        "hook" => Some(hook(&args[1..])),
        "hooks" => Some(hooks(&args[1..])),
        "remote" => Some(crate::remote::control::cli(&args[1..])),
        _ => None,
    }
}

/// Pane id and socket from the environment, or `None` outside a PetruTerm pane.
fn pane_target() -> Option<(u64, PathBuf)> {
    let pane = std::env::var("PETRUTERM_PANE_ID").ok()?.parse().ok()?;
    let sock = std::env::var_os("PETRUTERM_SOCKET")?;
    Some((pane, PathBuf::from(sock)))
}

fn deliver(sock: &Path, msg: &Message) {
    // A dead or missing socket must never fail a hook.
    let _ = server::send_to(sock, msg);
}

fn notify(args: &[String]) -> i32 {
    let mut msg = Message::default();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let slot = match flag.as_str() {
            "--state" => &mut msg.state,
            "--title" => &mut msg.title,
            "--body" => &mut msg.body,
            other => {
                eprintln!("petruterm notify: unknown argument `{other}`");
                return 2;
            }
        };
        let Some(value) = it.next() else {
            eprintln!("petruterm notify: `{flag}` needs a value");
            return 2;
        };
        *slot = Some(value.clone());
    }
    if let Some(s) = &msg.state {
        if parse_state(s).is_none() {
            eprintln!("petruterm notify: state must be working, needs-input, done or idle");
            return 2;
        }
    } else if msg.body.is_none() {
        eprintln!("usage: petruterm notify [--state S] [--title T] [--body B]");
        return 2;
    }
    if let Some((pane, sock)) = pane_target() {
        msg.pane = pane;
        deliver(&sock, &msg);
    }
    0
}

fn hook(args: &[String]) -> i32 {
    let (Some(agent), Some(event)) = (args.first(), args.get(1)) else {
        return 0;
    };
    let Some((pane, sock)) = pane_target() else {
        return 0;
    };
    if agent != "claude" {
        return 0;
    }
    let payload = read_stdin_json(Duration::from_millis(500));
    if let Some(msg) = claude_hooks::message_for(event, &payload, pane) {
        deliver(&sock, &msg);
    }
    0
}

/// Read a JSON document from stdin without ever blocking the hook for long.
fn read_stdin_json(timeout: Duration) -> Value {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::stdin().take(64 * 1024).read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    rx.recv_timeout(timeout)
        .ok()
        .and_then(|buf| serde_json::from_slice(&buf).ok())
        .unwrap_or(Value::Null)
}

fn hooks(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("setup") => setup(&args[1..]),
        _ => {
            eprintln!("usage: petruterm hooks setup claude [--dry-run] [--settings PATH]");
            2
        }
    }
}

fn setup(args: &[String]) -> i32 {
    if args.first().map(String::as_str) != Some("claude") {
        eprintln!("petruterm hooks setup: only `claude` is supported");
        return 2;
    }
    let mut dry_run = false;
    let mut path = dirs::home_dir().map(|h| h.join(".claude/settings.json"));
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dry-run" => dry_run = true,
            "--settings" => path = it.next().map(PathBuf::from),
            other => {
                eprintln!("petruterm hooks setup: unknown argument `{other}`");
                return 2;
            }
        }
    }
    let Some(path) = path else {
        eprintln!("petruterm hooks setup: cannot locate the home directory");
        return 1;
    };
    match install_claude_hooks(&path, dry_run) {
        Ok(report) => {
            println!("{report}");
            0
        }
        Err(e) => {
            eprintln!("petruterm hooks setup: {e}");
            1
        }
    }
}

/// Merge our hooks into the settings file at `path`. Writes `<path>.petruterm-bak` first when
/// the file exists. Returns a human-readable report.
pub fn install_claude_hooks(path: &Path, dry_run: bool) -> Result<String, String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let mut settings: Value = match &existing {
        Some(text) if !text.trim().is_empty() => serde_json::from_str(text).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); not touching it",
                path.display()
            )
        })?,
        _ => serde_json::json!({}),
    };
    let added = claude_hooks::merge_into(&mut settings)?;
    if added.is_empty() {
        return Ok(format!(
            "{}: PetruTerm hooks already installed, nothing to do.",
            path.display()
        ));
    }
    let mut report = format!(
        "{}: adding hooks for {} (command: `{}`)",
        path.display(),
        added.join(", "),
        claude_hooks::hook_command("<Event>"),
    );
    if dry_run {
        report.push_str("\n--dry-run: no changes written.");
        return Ok(report);
    }
    if let Some(text) = &existing {
        let mut bak = path.as_os_str().to_owned();
        bak.push(".petruterm-bak");
        std::fs::write(&bak, text).map_err(|e| format!("cannot write backup: {e}"))?;
        report.push_str(&format!("\nbackup: {}", PathBuf::from(bak).display()));
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let out = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(path, out + "\n").map_err(|e| format!("cannot write settings: {e}"))?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn unknown_commands_fall_through_to_the_gui() {
        assert_eq!(run(&args(&[])), None);
        assert_eq!(run(&args(&["--some-flag"])), None);
    }

    #[test]
    fn notify_validates_arguments() {
        assert_eq!(notify(&args(&["--state", "bogus"])), 2);
        assert_eq!(notify(&args(&["--bogus", "x"])), 2);
        assert_eq!(notify(&args(&["--body"])), 2);
        assert_eq!(notify(&args(&[])), 2);
    }

    #[test]
    fn install_creates_backs_up_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/settings.json");
        let r = install_claude_hooks(&path, false).unwrap();
        assert!(r.contains("adding hooks"));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v["hooks"]["Stop"].is_array());

        // Existing user settings are kept and backed up.
        std::fs::write(&path, r#"{"model":"opus"}"#).unwrap();
        install_claude_hooks(&path, false).unwrap();
        let bak = dir.path().join("sub/settings.json.petruterm-bak");
        assert_eq!(std::fs::read_to_string(bak).unwrap(), r#"{"model":"opus"}"#);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["model"], "opus");

        let r = install_claude_hooks(&path, false).unwrap();
        assert!(r.contains("nothing to do"));
    }

    #[test]
    fn dry_run_and_invalid_json_do_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let r = install_claude_hooks(&path, true).unwrap();
        assert!(r.contains("--dry-run"));
        assert!(!path.exists());
        std::fs::write(&path, "{ broken").unwrap();
        assert!(install_claude_hooks(&path, false).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
    }
}
