//! Behavioral tests for `scripts/shims/claude` using a fake `claude` that echoes its argv.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn shim_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/shims/claude")
}

struct Env {
    _dir: tempfile::TempDir,
    shim_dir: PathBuf,
    real_dir: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

fn setup() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let shim_dir = dir.path().join("shim");
    let real_dir = dir.path().join("real");
    let home = dir.path().join("home");
    for d in [&shim_dir, &real_dir, &home] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(shim_path(), shim_dir.join("claude")).unwrap();
    let fake = real_dir.join("claude");
    // One argument per line, so JSON with spaces survives.
    std::fs::write(
        &fake,
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done\n",
    )
    .unwrap();
    let bin = dir.path().join("petruterm");
    std::fs::write(&bin, "#!/bin/sh\n").unwrap();
    for p in [&shim_dir.join("claude"), &fake, &bin] {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    Env {
        _dir: dir,
        shim_dir,
        real_dir,
        home,
        bin,
    }
}

fn run(env: &Env, in_pane: bool, args: &[&str]) -> Vec<String> {
    let mut cmd = Command::new(env.shim_dir.join("claude"));
    cmd.args(args).env_clear().env("HOME", &env.home).env(
        "PATH",
        format!(
            "{}:{}:/usr/bin:/bin",
            env.shim_dir.display(),
            env.real_dir.display()
        ),
    );
    if in_pane {
        cmd.env("PETRUTERM_PANE_ID", "3")
            .env("PETRUTERM_BIN", &env.bin);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

#[test]
fn injects_hooks_matching_the_rust_definition() {
    let env = setup();
    let argv = run(&env, true, &["fix the bug"]);
    assert_eq!(argv[0], "--settings");
    assert_eq!(argv[2], "fix the bug");
    let injected: serde_json::Value = serde_json::from_str(&argv[1]).expect("valid JSON");
    assert_eq!(injected, petruterm::ipc::claude_hooks::settings_fragment());
}

#[test]
fn passes_through_outside_a_pane() {
    let env = setup();
    assert_eq!(run(&env, false, &["--version"]), ["--version"]);
}

#[test]
fn passes_through_with_user_settings_and_subcommands() {
    let env = setup();
    assert_eq!(
        run(&env, true, &["--settings", "x.json", "hi"]),
        ["--settings", "x.json", "hi"]
    );
    assert_eq!(
        run(&env, true, &["--settings=x.json"]),
        ["--settings=x.json"]
    );
    assert_eq!(run(&env, true, &["mcp", "list"]), ["mcp", "list"]);
}

#[test]
fn skips_injection_when_installer_hooks_exist() {
    let env = setup();
    std::fs::create_dir_all(env.home.join(".claude")).unwrap();
    std::fs::write(
        env.home.join(".claude/settings.json"),
        r#"{"hooks":{"Stop":[{"hooks":[{"command":"\"$PETRUTERM_BIN\" hook claude Stop"}]}]}}"#,
    )
    .unwrap();
    assert_eq!(run(&env, true, &["hi"]), ["hi"]);
}

#[test]
fn reports_missing_real_claude() {
    let env = setup();
    std::fs::remove_file(env.real_dir.join("claude")).unwrap();
    let out = Command::new(env.shim_dir.join("claude"))
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", env.shim_dir.display()))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(127));
}
