//! Runs the real `petruterm msg` CLI against a fake control socket that answers like the app.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

/// Answer one request with `reply` and return the request line the CLI sent.
fn serve_once(listener: UnixListener, reply: &'static str) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        writeln!(stream, "{reply}").unwrap();
        line
    })
}

fn run(args: &[&str], stdin: &str, env: bool, sock: &std::path::Path) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_petruterm"));
    cmd.arg("msg")
        .args(args)
        .env_remove("PETRUTERM_PANE_ID")
        .env_remove("PETRUTERM_SOCKET")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if env {
        cmd.env("PETRUTERM_PANE_ID", "11")
            .env("PETRUTERM_SOCKET", sock);
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    )
}

fn outcome(reply: &'static str, args: &[&str]) -> (i32, String, serde_json::Value) {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let h = serve_once(UnixListener::bind(&sock).unwrap(), reply);
    let (code, out) = run(args, "", true, &sock);
    let line = h.join().unwrap();
    (code, out, serde_json::from_str(&line).unwrap())
}

#[test]
fn delivered_prints_and_exits_zero_and_sends_the_request() {
    let (code, out, req) = outcome(
        r#"{"result":"delivered","detail":"kterm"}"#,
        &["kterm", "What", "is", "7*6?"],
    );
    assert_eq!((code, out.as_str()), (0, "delivered to kterm"));
    assert_eq!(req["pane"], 11);
    assert_eq!(req["msg"]["to"], "kterm");
    assert_eq!(req["msg"]["text"], "What is 7*6?");
    assert_eq!(req["msg"]["no_reply"], false);
}

#[test]
fn no_reply_flag_is_sent() {
    let (code, _, req) = outcome(
        r#"{"result":"delivered","detail":"kterm"}"#,
        &["--no-reply", "kterm", "done"],
    );
    assert_eq!(code, 0);
    assert_eq!(req["msg"]["no_reply"], true);
}

#[test]
fn every_outcome_maps_to_its_exit_code_and_line() {
    let cases: [(&'static str, i32, &str); 11] = [
        (
            r#"{"result":"unknown_name","detail":"kterm"}"#,
            3,
            "error: unknown agent kterm",
        ),
        (
            r#"{"result":"not_accepting","detail":"kterm"}"#,
            4,
            "not accepting messages",
        ),
        (
            r#"{"result":"rate_limited","retry_after_s":12}"#,
            5,
            "try again in 12s",
        ),
        (
            r#"{"result":"conversation_limit"}"#,
            6,
            "conversation limit reached",
        ),
        (r#"{"result":"too_long"}"#, 7, "too long"),
        (
            r#"{"result":"agent_gone","detail":"kterm"}"#,
            8,
            "kterm is no longer running",
        ),
        (r#"{"result":"sender_unknown"}"#, 9, "cannot send from here"),
        (
            r#"{"result":"self_message"}"#,
            10,
            "cannot message yourself",
        ),
        (r#"{"result":"empty"}"#, 11, "empty message"),
        (
            r#"{"result":"unsupported"}"#,
            12,
            "not available in this build",
        ),
        (r#"{"result":"timeout"}"#, 13, "did not answer"),
    ];
    for (reply, want_code, want_text) in cases {
        let (code, out, _) = outcome(reply, &["kterm", "x"]);
        assert_eq!(code, want_code, "{reply}");
        assert!(out.contains(want_text), "{reply}: {out}");
    }
}

#[test]
fn text_can_come_from_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let h = serve_once(
        UnixListener::bind(&sock).unwrap(),
        r#"{"result":"delivered","detail":"kterm"}"#,
    );
    let (code, _) = run(&["kterm", "-"], "line one\nsay \"hi\"\n", true, &sock);
    let req: serde_json::Value = serde_json::from_str(&h.join().unwrap()).unwrap();
    assert_eq!(code, 0);
    assert_eq!(req["msg"]["text"], "line one\nsay \"hi\"\n");
}

#[test]
fn outside_petruterm_exits_nine_without_connecting() {
    let (code, out) = run(
        &["kterm", "hi"],
        "",
        false,
        std::path::Path::new("/nonexistent"),
    );
    assert_eq!(code, 9);
    assert!(out.contains("cannot send from here"));
}

#[test]
fn usage_errors_and_unreachable_app() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("gone.sock");
    assert_eq!(run(&[], "", true, &sock).0, 2);
    assert_eq!(run(&["kterm"], "", true, &sock).0, 2);
    // socket path exists in the env but nobody listens
    let (code, out) = run(&["kterm", "hi"], "", true, &sock);
    assert_eq!(code, 13);
    assert!(out.contains("did not answer"));
}

#[test]
fn oversized_text_is_refused_before_sending() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("none.sock");
    let big = "a".repeat(13_000);
    let (code, out) = run(&["kterm", &big], "", true, &sock);
    assert_eq!(code, 7);
    assert!(out.contains("too long"));
}

fn agents(args: &[String]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_petruterm"))
        .arg("agents")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

#[test]
fn agents_setup_installs_is_idempotent_and_remove_cleans_up() {
    let d = tempfile::tempdir().unwrap();
    let claude = d.path().join("c/skills");
    let kiro = d.path().join("k/steering");
    let args = |first: &str| -> Vec<String> {
        vec![
            first.to_string(),
            "--claude-skills-dir".to_string(),
            claude.to_str().unwrap().to_string(),
            "--kiro-steering-dir".to_string(),
            kiro.to_str().unwrap().to_string(),
        ]
    };
    let (code, out) = agents(&args("setup"));
    assert_eq!(code, 0, "{out}");
    assert!(claude.join("petruterm-messaging/SKILL.md").is_file());
    assert!(kiro.join("petruterm-messaging.md").is_file());
    let (code, out) = agents(&args("setup"));
    assert_eq!(code, 0);
    assert!(out.contains("already installed"), "{out}");
    let (code, _) = agents(&args("remove"));
    assert_eq!(code, 0);
    assert!(!kiro.join("petruterm-messaging.md").exists());
    assert_eq!(agents(&["bogus".to_string()]).0, 2);
}
