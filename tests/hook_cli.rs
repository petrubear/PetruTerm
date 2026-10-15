//! Runs the real `petruterm` binary as a hook would, against a fake control socket.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

fn run_cli(args: &[&str], stdin: &str, sock: &std::path::Path) -> std::process::ExitStatus {
    let mut child = Command::new(env!("CARGO_BIN_EXE_petruterm"))
        .args(args)
        .env("PETRUTERM_PANE_ID", "11")
        .env("PETRUTERM_SOCKET", sock)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait().unwrap()
}

fn first_line(listener: &UnixListener) -> serde_json::Value {
    let (stream, _) = listener.accept().unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn hook_sends_mapped_state_to_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let status = run_cli(
        &["hook", "claude", "Notification"],
        r#"{"hook_event_name":"Notification","message":"Allow Bash?"}"#,
        &sock,
    );
    assert!(status.success());
    let msg = first_line(&listener);
    assert_eq!(msg["pane"], 11);
    assert_eq!(msg["state"], "needs-input");
    assert_eq!(msg["body"], "Allow Bash?");
}

#[test]
fn notify_sends_state_title_and_body() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let status = run_cli(
        &[
            "notify", "--state", "done", "--title", "Build", "--body", "ok",
        ],
        "",
        &sock,
    );
    assert!(status.success());
    let msg = first_line(&listener);
    assert_eq!(msg["state"], "done");
    assert_eq!(msg["title"], "Build");
    assert_eq!(msg["body"], "ok");
}

#[test]
fn outside_a_pane_or_with_a_dead_socket_hooks_exit_zero() {
    let status = Command::new(env!("CARGO_BIN_EXE_petruterm"))
        .args(["hook", "claude", "Stop"])
        .env_remove("PETRUTERM_PANE_ID")
        .env_remove("PETRUTERM_SOCKET")
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let dir = tempfile::tempdir().unwrap();
    let status = run_cli(
        &["hook", "claude", "Stop"],
        "{}",
        &dir.path().join("gone.sock"),
    );
    assert!(status.success());
}

#[test]
fn hooks_setup_dry_run_does_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let out = Command::new(env!("CARGO_BIN_EXE_petruterm"))
        .args(["hooks", "setup", "claude", "--dry-run", "--settings"])
        .arg(&settings)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!settings.exists());
}
