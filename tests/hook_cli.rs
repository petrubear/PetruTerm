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

fn run_cli_output(
    args: &[&str],
    stdin: &str,
    sock: &std::path::Path,
) -> (std::process::ExitStatus, Vec<u8>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_petruterm"))
        .args(args)
        .env("PETRUTERM_PANE_ID", "11")
        .env("PETRUTERM_SOCKET", sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status, out.stdout)
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
fn pre_tool_use_hook_sends_the_activity() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let status = run_cli(
        &["hook", "claude", "PreToolUse"],
        r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo test"},"cwd":"/p"}"#,
        &sock,
    );
    assert!(status.success());
    let msg = first_line(&listener);
    assert_eq!(msg["state"], "working");
    assert_eq!(msg["activity"], "Bash: cargo test");
}

#[test]
fn prompt_hook_sends_the_prompt_start_and_stop_sends_no_activity() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let status = run_cli(
        &["hook", "claude", "UserPromptSubmit"],
        r#"{"hook_event_name":"UserPromptSubmit","prompt":"fix the bug\nin detail"}"#,
        &sock,
    );
    assert!(status.success());
    assert_eq!(first_line(&listener)["activity"], "Prompt: fix the bug");
    let status = run_cli(&["hook", "claude", "Stop"], "{}", &sock);
    assert!(status.success());
    let msg = first_line(&listener);
    assert_eq!(msg["state"], "done");
    assert!(msg.get("activity").is_none());
}

#[test]
fn notify_sends_activity_and_truncation_is_left_to_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let status = run_cli(
        &[
            "notify",
            "--state",
            "working",
            "--activity",
            "Indexing repo",
        ],
        "",
        &sock,
    );
    assert!(status.success());
    let msg = first_line(&listener);
    assert_eq!(msg["state"], "working");
    assert_eq!(msg["activity"], "Indexing repo");
    let long = "x".repeat(5000);
    let status = run_cli(
        &["notify", "--state", "working", "--activity", &long],
        "",
        &sock,
    );
    assert!(status.success());
    let msg = first_line(&listener);
    assert_eq!(msg["state"], "working");
    assert_eq!(msg["activity"].as_str().unwrap().len(), 5000);
    let status = run_cli(&["notify", "--state", "done"], "", &sock);
    assert!(status.success());
    assert!(first_line(&listener).get("activity").is_none());
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

#[test]
fn unknown_agents_and_claude_outside_a_pane_are_silent_and_succeed() {
    for args in [["hook", "nosuchagent", "Stop"], ["hook", "claude", "Stop"]] {
        let out = Command::new(env!("CARGO_BIN_EXE_petruterm"))
            .args(args)
            .env_remove("PETRUTERM_PANE_ID")
            .env_remove("PETRUTERM_SOCKET")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?} printed {:?}", out.stdout);
    }
}

#[test]
fn kiro_hooks_report_state_and_activity_and_print_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let cases = [
        (
            "UserPromptSubmit",
            r#"{"hook_event_name":"UserPromptSubmit","cwd":"/p","session_id":"k1","prompt":"list files"}"#,
            "working",
            Some("Prompt: list files"),
        ),
        (
            "PreToolUse",
            r#"{"hook_event_name":"PreToolUse","cwd":"/p","session_id":"k1","tool_name":"execute_bash","tool_input":{"command":"ls"}}"#,
            "working",
            Some("Shell: ls"),
        ),
        (
            "Stop",
            r#"{"hook_event_name":"Stop","session_id":"k1"}"#,
            "done",
            None,
        ),
        (
            "SessionEnd",
            r#"{"hook_event_name":"SessionEnd"}"#,
            "idle",
            None,
        ),
    ];
    for (event, payload, state, activity) in cases {
        let (status, stdout) = run_cli_output(&["hook", "kiro", event], payload, &sock);
        assert!(status.success(), "{event}");
        assert!(stdout.is_empty(), "{event} printed {stdout:?}");
        let msg = first_line(&listener);
        assert_eq!(msg["pane"], 11);
        assert_eq!(msg["state"], state, "{event}");
        match activity {
            Some(a) => assert_eq!(msg["activity"], a, "{event}"),
            None => assert!(msg.get("activity").is_none(), "{event}"),
        }
    }
}

#[test]
fn codex_hooks_report_permission_requests_and_interrupts_silently() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let cases = [
        (
            "PreToolUse",
            r#"{"session_id":"c1","cwd":"/p","tool_name":"Bash","tool_input":{"command":"cargo test"}}"#,
            "working",
        ),
        (
            "PermissionRequest",
            r#"{"session_id":"c1","tool_name":"Bash","tool_input":{"command":"rm x","description":"Remove x"}}"#,
            "needs-input",
        ),
        ("Interrupt", r#"{"session_id":"c1"}"#, "idle"),
    ];
    for (event, payload, state) in cases {
        let (status, stdout) = run_cli_output(&["hook", "codex", event], payload, &sock);
        assert!(status.success(), "{event}");
        assert!(stdout.is_empty(), "{event} printed {stdout:?}");
        let msg = first_line(&listener);
        assert_eq!(msg["state"], state, "{event}");
        if event == "PreToolUse" {
            assert_eq!(msg["activity"], "Bash: cargo test");
        }
        if event == "PermissionRequest" {
            assert_eq!(msg["body"], "Remove x");
        }
    }
}

#[test]
fn agy_hooks_always_print_the_required_json_and_still_report() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let payload = r#"{"conversationId":"a1","workspacePaths":["/p"],"toolCall":{"name":"run_command","args":{"CommandLine":"ls"}}}"#;
    let (status, stdout) = run_cli_output(&["hook", "agy", "PreToolUse"], payload, &sock);
    assert!(status.success());
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        r#"{"decision":"allow"}"#
    );
    let msg = first_line(&listener);
    assert_eq!(msg["state"], "working");
    assert_eq!(msg["activity"], "Shell: ls");
    let (status, stdout) = run_cli_output(&["hook", "agy", "Stop"], r#"{"fullyIdle":true}"#, &sock);
    assert!(status.success());
    assert_eq!(String::from_utf8(stdout).unwrap(), "{}");
    assert_eq!(first_line(&listener)["state"], "done");

    // Outside a pane and with a dead socket the reply is the same and nothing is sent.
    for envs in [false, true] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_petruterm"));
        cmd.args(["hook", "agy", "PreToolUse"]).stdin(Stdio::null());
        if envs {
            cmd.env("PETRUTERM_PANE_ID", "1")
                .env("PETRUTERM_SOCKET", dir.path().join("dead.sock"));
        } else {
            cmd.env_remove("PETRUTERM_PANE_ID")
                .env_remove("PETRUTERM_SOCKET");
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success());
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            r#"{"decision":"allow"}"#
        );
    }
}
