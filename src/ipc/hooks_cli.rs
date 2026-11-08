//! `petruterm hooks setup|remove|status` for the agents that keep hooks in a file (Kiro,
//! Codex, Antigravity). Claude Code needs no install (its shim injects hooks at launch); its
//! `hooks setup claude` stays in `cli.rs`. See
//! `specs/004-agent-integrations/contracts/hooks-setup-cli.md`.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::hook_files::{backup_path, load_json, write_json};
use super::integrations::{self, AgentIntegration};

const USAGE: &str =
    "usage: petruterm hooks setup|remove [kiro|codex|agy|all] [--dry-run] | hooks status";

/// Where the command looks for hook files and agent binaries.
pub struct Env {
    pub home: PathBuf,
    pub path_dirs: Vec<PathBuf>,
}

impl Env {
    fn from_process() -> Option<Self> {
        Some(Self {
            home: dirs::home_dir()?,
            path_dirs: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
        })
    }
}

/// Entry point for `petruterm hooks setup|remove|status ...` (args start with the subcommand).
pub fn run(args: &[String]) -> i32 {
    let Some(env) = Env::from_process() else {
        eprintln!("petruterm hooks: cannot locate the home directory");
        return 1;
    };
    let mut out = String::new();
    let code = execute(args, &env, integrations::all(), &mut out);
    if code == 2 {
        eprint!("{out}");
    } else {
        print!("{out}");
    }
    code
}

fn binary_found(names: &[&str], dirs: &[PathBuf]) -> bool {
    names
        .iter()
        .any(|n| dirs.iter().any(|d| d.join(n).is_file()))
}

/// The testable core: parse, act, append report lines to `out`, return the exit code.
pub fn execute(
    args: &[String],
    env: &Env,
    registry: &[&dyn AgentIntegration],
    out: &mut String,
) -> i32 {
    let Some(sub) = args.first().map(String::as_str) else {
        out.push_str(USAGE);
        out.push('\n');
        return 2;
    };
    let mut dry_run = false;
    let mut selector = "all";
    for a in &args[1..] {
        match a.as_str() {
            "--dry-run" => dry_run = true,
            id if !id.starts_with('-') => selector = id,
            other => {
                out.push_str(&format!(
                    "petruterm hooks: unknown argument `{other}`\n{USAGE}\n"
                ));
                return 2;
            }
        }
    }
    let managed = |i: &dyn AgentIntegration| i.hook_file(&env.home).is_some();
    let selected: Vec<&dyn AgentIntegration> = if selector == "all" {
        registry.iter().copied().filter(|i| managed(*i)).collect()
    } else {
        match registry.iter().copied().find(|i| i.id() == selector) {
            Some(i) if managed(i) => vec![i],
            Some(_) => {
                out.push_str(&format!(
                    "{selector}: nothing to install here (Claude Code is set up by its shim)\n"
                ));
                return 0;
            }
            None => {
                out.push_str(&format!(
                    "petruterm hooks: unknown agent `{selector}`\n{USAGE}\n"
                ));
                return 2;
            }
        }
    };
    let mut code = 0;
    for i in selected {
        let Some(path) = i.hook_file(&env.home) else {
            continue;
        };
        let line = match sub {
            "setup" => setup_one(i, &path, env, dry_run),
            "remove" => remove_one(i, &path, dry_run),
            "status" => Ok(status_one(i, &path, env)),
            _ => {
                out.push_str(&format!(
                    "petruterm hooks: unknown subcommand `{sub}`\n{USAGE}\n"
                ));
                return 2;
            }
        };
        match line {
            Ok(l) => out.push_str(&format!("{}: {l}\n", i.id())),
            Err(e) => {
                out.push_str(&format!("{}: {e}\n", i.id()));
                code = 1;
            }
        }
    }
    code
}

fn setup_one(
    i: &dyn AgentIntegration,
    path: &Path,
    env: &Env,
    dry_run: bool,
) -> Result<String, String> {
    if !binary_found(i.binaries(), &env.path_dirs) {
        return Ok(format!(
            "skipped, {} not found on PATH",
            i.binaries().join(" or ")
        ));
    }
    let (existing, mut doc) = match load_json(path)? {
        Some((text, doc)) => (Some(text), doc),
        None => (None, json!({})),
    };
    if i.install(&mut doc)? == 0 {
        return Ok("hooks already installed, nothing to do".into());
    }
    let mut line = write_json(path, &doc, existing.as_deref(), dry_run)?;
    if let Some(note) = i.setup_note() {
        line.push_str(&format!("\n  note: {note}"));
    }
    Ok(line)
}

fn remove_one(i: &dyn AgentIntegration, path: &Path, dry_run: bool) -> Result<String, String> {
    let Some((text, mut doc)) = load_json(path)? else {
        return Ok("not installed".into());
    };
    if i.remove(&mut doc) == 0 {
        return Ok("nothing to remove".into());
    }
    // A file we created and nothing else uses goes away; otherwise it keeps the user's content.
    if i.is_empty(&doc) && !backup_path(path).exists() {
        if !dry_run {
            std::fs::remove_file(path)
                .map_err(|e| format!("cannot delete {}: {e}", path.display()))?;
        }
        return Ok(format!(
            "{}: deleted{}",
            path.display(),
            if dry_run {
                " (--dry-run: not done)"
            } else {
                ""
            }
        ));
    }
    write_json(path, &doc, Some(&text), dry_run)
}

fn status_one(i: &dyn AgentIntegration, path: &Path, env: &Env) -> String {
    let found = binary_found(i.binaries(), &env.path_dirs);
    let installed = match load_json(path) {
        Ok(Some((_, doc))) => i.installed(&doc),
        _ => 0,
    };
    let binary = if found {
        "binary found"
    } else {
        "binary not found"
    };
    match (installed, i.setup_note()) {
        (0, _) => format!("not installed ({binary})"),
        (n, Some(note)) => format!("installed ({n} events, {binary}), note: {note}"),
        (n, None) => format!("installed ({n} events, {binary})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::hook_files::{contains_marker, strip_marked};
    use serde_json::Value;

    /// A fake agent keeping Claude-style hooks under `hooks` in `<home>/.fake/hooks.json`.
    struct Fake;

    impl AgentIntegration for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn events(&self) -> &'static [&'static str] {
            &["Stop"]
        }
        fn on_hook(&self, _: &str, _: &Value, _: u64) -> Option<crate::ipc::Message> {
            None
        }
        fn binaries(&self) -> &'static [&'static str] {
            &["fakeagent"]
        }
        fn hook_file(&self, home: &Path) -> Option<PathBuf> {
            Some(home.join(".fake/hooks.json"))
        }
        fn install(&self, doc: &mut Value) -> Result<usize, String> {
            let events = doc
                .as_object_mut()
                .ok_or("not an object")?
                .entry("hooks")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or("hooks is not an object")?;
            if crate::ipc::hook_files::count_marked(events) > 0 {
                return Ok(0);
            }
            events
                .entry("Stop")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("Stop is not an array")?
                .push(json!({"hooks": [{"command": "[ -z \"$PETRUTERM_BIN\" ] || x"}]}));
            Ok(1)
        }
        fn remove(&self, doc: &mut Value) -> usize {
            doc.get_mut("hooks")
                .and_then(Value::as_object_mut)
                .map_or(0, strip_marked)
        }
        fn installed(&self, doc: &Value) -> usize {
            doc.get("hooks")
                .and_then(Value::as_object)
                .map_or(0, crate::ipc::hook_files::count_marked)
        }
        fn is_empty(&self, doc: &Value) -> bool {
            doc.get("hooks")
                .and_then(Value::as_object)
                .is_none_or(|o| o.is_empty())
        }
        fn setup_note(&self) -> Option<&'static str> {
            Some("trust it")
        }
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn env_with_binary(dir: &Path, with_binary: bool) -> Env {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        if with_binary {
            std::fs::write(bin.join("fakeagent"), "").unwrap();
        }
        Env {
            home: dir.join("home"),
            path_dirs: vec![bin],
        }
    }

    fn run_cmd(a: &[&str], env: &Env) -> (i32, String) {
        let mut out = String::new();
        let code = execute(&args(a), env, &[&Fake], &mut out);
        (code, out)
    }

    #[test]
    fn setup_installs_is_idempotent_and_remove_deletes_our_file() {
        let d = tempfile::tempdir().unwrap();
        let env = env_with_binary(d.path(), true);
        let file = env.home.join(".fake/hooks.json");
        let (code, out) = run_cmd(&["setup"], &env);
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("note: trust it"), "{out}");
        assert!(contains_marker(&load_json(&file).unwrap().unwrap().1));
        let (_, out) = run_cmd(&["setup"], &env);
        assert!(out.contains("nothing to do"), "{out}");
        let (code, out) = run_cmd(&["remove"], &env);
        assert_eq!(code, 0, "{out}");
        assert!(!file.exists(), "a file we created is deleted: {out}");
    }

    #[test]
    fn user_content_survives_setup_and_remove_with_a_backup() {
        let d = tempfile::tempdir().unwrap();
        let env = env_with_binary(d.path(), true);
        let file = env.home.join(".fake/hooks.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let user = r#"{"hooks":{"Stop":[{"hooks":[{"command":"say done"}]}]},"other":1}"#;
        std::fs::write(&file, user).unwrap();
        run_cmd(&["setup", "fake"], &env);
        assert_eq!(std::fs::read_to_string(backup_path(&file)).unwrap(), user);
        run_cmd(&["remove", "fake"], &env);
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(after, serde_json::from_str::<Value>(user).unwrap());
    }

    #[test]
    fn missing_binary_dry_run_and_invalid_files() {
        let d = tempfile::tempdir().unwrap();
        let env = env_with_binary(d.path(), false);
        let (code, out) = run_cmd(&["setup"], &env);
        assert_eq!(code, 0);
        assert!(out.contains("skipped"), "{out}");
        assert!(!env.home.join(".fake/hooks.json").exists());

        let env = env_with_binary(d.path(), true);
        let (_, out) = run_cmd(&["setup", "--dry-run"], &env);
        assert!(out.contains("dry-run"), "{out}");
        assert!(!env.home.join(".fake/hooks.json").exists());

        let file = env.home.join(".fake/hooks.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{ broken").unwrap();
        let (code, out) = run_cmd(&["setup"], &env);
        assert_eq!(code, 1, "{out}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ broken");
    }

    #[test]
    fn status_reports_each_state() {
        let d = tempfile::tempdir().unwrap();
        let env = env_with_binary(d.path(), true);
        assert!(run_cmd(&["status"], &env)
            .1
            .contains("not installed (binary found)"));
        run_cmd(&["setup"], &env);
        let (code, out) = run_cmd(&["status"], &env);
        assert_eq!(code, 0);
        assert!(
            out.contains("installed (1 events, binary found), note: trust it"),
            "{out}"
        );
    }

    #[test]
    fn usage_errors_exit_2() {
        let d = tempfile::tempdir().unwrap();
        let env = env_with_binary(d.path(), true);
        assert_eq!(run_cmd(&[], &env).0, 2);
        assert_eq!(run_cmd(&["setup", "--bogus"], &env).0, 2);
        assert_eq!(run_cmd(&["setup", "nope"], &env).0, 2);
        assert_eq!(run_cmd(&["bogus"], &env).0, 2);
    }
}
