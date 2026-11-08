//! `petruterm hooks setup|remove|status` against a temporary HOME with stub agent binaries.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{json, Value};

struct Sandbox {
    dir: tempfile::TempDir,
}

impl Sandbox {
    fn new(binaries: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("home")).unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        for b in binaries {
            std::fs::write(dir.path().join("bin").join(b), "").unwrap();
        }
        Self { dir }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn file(&self, rel: &str) -> PathBuf {
        self.home().join(rel)
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_petruterm"))
            .arg("hooks")
            .args(args)
            .env("HOME", self.home())
            .env("PATH", self.dir.path().join("bin"))
            .env_remove("KIRO_HOME")
            .env_remove("CODEX_HOME")
            .output()
            .unwrap();
        (
            out.status.code().unwrap(),
            String::from_utf8_lossy(&out.stdout).into_owned()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.file(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.file(rel)).unwrap()
    }

    fn json(&self, rel: &str) -> Value {
        serde_json::from_str(&self.read(rel)).unwrap()
    }
}

const KIRO: &str = ".kiro/hooks/petruterm.json";
const CODEX: &str = ".codex/hooks.json";
const AGY: &str = ".gemini/config/hooks.json";

fn user_codex() -> Value {
    json!({
        "model": "x",
        "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}
    })
}

fn user_agy() -> Value {
    json!({"lint": {"PostToolUse": [{"matcher": "run_command", "hooks": [{"type": "command", "command": "./lint.sh"}]}]}})
}

#[test]
fn setup_installs_for_every_agent_found_and_keeps_user_entries() {
    let sb = Sandbox::new(&["kiro-cli", "codex", "agy"]);
    sb.write(CODEX, &user_codex().to_string());
    sb.write(AGY, &user_agy().to_string());
    let (code, out) = sb.run(&["setup"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("trust required: run /hooks"), "{out}");
    assert!(out.contains("Kiro CLI V3"), "{out}");

    assert_eq!(sb.json(KIRO)["hooks"].as_array().unwrap().len(), 5);
    let codex = sb.json(CODEX);
    assert_eq!(codex["model"], "x");
    assert_eq!(codex["hooks"]["Stop"][0], user_codex()["hooks"]["Stop"][0]);
    assert!(codex.to_string().contains("PETRUTERM_BIN"));
    let agy = sb.json(AGY);
    assert_eq!(agy["lint"], user_agy()["lint"]);
    assert!(agy["petruterm"]["PreToolUse"].is_array());

    // backups of the files that existed before
    assert_eq!(
        sb.read(&format!("{CODEX}.petruterm-bak")),
        user_codex().to_string()
    );
    assert_eq!(
        sb.read(&format!("{AGY}.petruterm-bak")),
        user_agy().to_string()
    );
    assert!(!sb.file(&format!("{KIRO}.petruterm-bak")).exists());
}

#[test]
fn setup_twice_changes_nothing_and_remove_restores_the_user_files() {
    let sb = Sandbox::new(&["kiro-cli", "codex", "agy"]);
    sb.write(CODEX, &user_codex().to_string());
    sb.write(AGY, &user_agy().to_string());
    sb.run(&["setup"]);
    let before = (sb.read(KIRO), sb.read(CODEX), sb.read(AGY));
    let (code, out) = sb.run(&["setup"]);
    assert_eq!(code, 0);
    assert_eq!(out.matches("nothing to do").count(), 3, "{out}");
    assert_eq!((sb.read(KIRO), sb.read(CODEX), sb.read(AGY)), before);

    let (code, out) = sb.run(&["remove"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(sb.json(CODEX), user_codex());
    assert_eq!(sb.json(AGY), user_agy());
    assert!(!sb.file(KIRO).exists(), "our own Kiro file is deleted");
    // nothing left to remove
    assert!(sb.run(&["remove"]).1.contains("nothing to remove"));
}

#[test]
fn files_we_created_are_deleted_on_remove() {
    let sb = Sandbox::new(&["codex"]);
    assert_eq!(sb.run(&["setup", "codex"]).0, 0);
    assert!(sb.file(CODEX).exists());
    assert_eq!(sb.run(&["remove", "codex"]).0, 0);
    assert!(!sb.file(CODEX).exists());
}

#[test]
fn an_invalid_file_is_refused_unchanged_while_the_others_still_install() {
    let sb = Sandbox::new(&["kiro-cli", "codex", "agy"]);
    sb.write(CODEX, "{ broken");
    let (code, out) = sb.run(&["setup"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("not touching it"), "{out}");
    assert_eq!(sb.read(CODEX), "{ broken");
    assert!(sb.file(KIRO).exists());
    assert!(sb.file(AGY).exists());
}

#[test]
fn dry_run_writes_nothing_and_missing_binaries_are_skipped() {
    let sb = Sandbox::new(&["codex"]);
    let (code, out) = sb.run(&["setup", "--dry-run"]);
    assert_eq!(code, 0, "{out}");
    assert!(!sb.file(CODEX).exists());
    assert!(out.contains("kiro: skipped"), "{out}");
    assert!(out.contains("agy: skipped"), "{out}");
}

#[test]
fn a_named_agent_touches_only_its_own_file() {
    let sb = Sandbox::new(&["kiro-cli", "codex", "agy"]);
    assert_eq!(sb.run(&["setup", "kiro"]).0, 0);
    assert!(sb.file(KIRO).exists());
    assert!(!sb.file(CODEX).exists());
    assert!(!sb.file(AGY).exists());
}

#[test]
fn status_reports_not_installed_installed_and_the_codex_trust_note() {
    let sb = Sandbox::new(&["kiro-cli", "codex"]);
    let (code, out) = sb.run(&["status"]);
    assert_eq!(code, 0);
    assert!(out.contains("kiro: not installed (binary found)"), "{out}");
    assert!(
        out.contains("agy: not installed (binary not found)"),
        "{out}"
    );
    sb.run(&["setup"]);
    let (_, out) = sb.run(&["status"]);
    assert!(
        out.contains("kiro: installed (5 events, binary found)"),
        "{out}"
    );
    assert!(
        out.contains("codex: installed (7 events, binary found), note: trust required"),
        "{out}"
    );
    // a hand-trimmed file counts what is left
    let mut codex = sb.json(CODEX);
    codex["hooks"].as_object_mut().unwrap().remove("Interrupt");
    sb.write(CODEX, &codex.to_string());
    assert!(sb.run(&["status"]).1.contains("codex: installed (6 events"));
}

#[test]
fn unknown_agents_and_flags_are_usage_errors() {
    let sb = Sandbox::new(&[]);
    assert_eq!(sb.run(&["setup", "nope"]).0, 2);
    assert_eq!(sb.run(&["setup", "--bogus"]).0, 2);
    assert_eq!(sb.run(&["frobnicate"]).0, 2);
}
