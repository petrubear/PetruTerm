//! Shell integration: make sure the agent shim directory is first on PATH in the pane's
//! interactive shell, even though login-shell startup files (macOS `path_helper`, the user's
//! `export PATH=...:$PATH`) reorder it after the app set it.
//!
//! - zsh: `ZDOTDIR` points at a generated directory whose `.zshenv` restores the user's
//!   `ZDOTDIR`, sources their `.zshenv` and registers a one-shot `precmd` hook.
//! - bash: a `PROMPT_COMMAND` prefix does the same before the first prompt.
//!
//! Other shells (fish, ...) keep the plain PATH prepend and can use `hooks setup claude`.

use std::path::{Path, PathBuf};

const ZSHENV: &str = include_str!("../../scripts/shell-integration/zsh/.zshenv");

/// Runs before each bash prompt; only reorders when the shim is not already first.
const BASH_PROMPT_COMMAND: &str =
    r#"case ":$PATH:" in ":$PETRUTERM_SHIM_DIR:"*) ;; *) PATH="$PETRUTERM_SHIM_DIR:$PATH";; esac"#;

fn shell_name(shell: &str) -> &str {
    Path::new(shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
}

/// Write the zsh integration under `base` and return the `ZDOTDIR` to use.
fn ensure_zdotdir(base: &Path) -> std::io::Result<PathBuf> {
    let dir = base.join("zdotdir");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(".zshenv");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(ZSHENV) {
        std::fs::write(&file, ZSHENV)?;
    }
    Ok(dir)
}

/// Extra environment for `shell` so `shim_dir` wins PATH at the first prompt. `base` is where
/// generated integration files live. Empty for shells without integration.
pub fn integration_env(shell: &str, shim_dir: &Path, base: &Path) -> Vec<(String, String)> {
    let shim = shim_dir.to_string_lossy().into_owned();
    match shell_name(shell) {
        "zsh" => {
            let Ok(zdotdir) = ensure_zdotdir(base) else {
                return Vec::new();
            };
            vec![
                ("PETRUTERM_SHIM_DIR".into(), shim),
                (
                    "PETRUTERM_ORIG_ZDOTDIR".into(),
                    std::env::var("ZDOTDIR").unwrap_or_default(),
                ),
                ("ZDOTDIR".into(), zdotdir.to_string_lossy().into_owned()),
            ]
        }
        "bash" => {
            let mut prompt = BASH_PROMPT_COMMAND.to_string();
            if let Ok(existing) = std::env::var("PROMPT_COMMAND") {
                if !existing.is_empty() {
                    prompt = format!("{prompt}; {existing}");
                }
            }
            vec![
                ("PETRUTERM_SHIM_DIR".into(), shim),
                ("PROMPT_COMMAND".into(), prompt),
            ]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn first_path_entry(shell: &str, home: &Path, env: &[(String, String)]) -> Option<String> {
        if !Path::new(shell).exists() {
            return None; // shell not installed on this machine
        }
        let mut child = Command::new(shell)
            .args(["-l", "-i"])
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        child
            .stdin
            .take()?
            .write_all(b"echo PATHIS:$PATH\nexit\n")
            .ok()?;
        let out = child.wait_with_output().ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| l.split_once("PATHIS:").map(|(_, p)| p.to_string()))
            .and_then(|p| p.split(':').next().map(String::from))
    }

    fn user_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        // The common offender: a startup file that prepends the user's own bin directory.
        for f in [".zshrc", ".bash_profile"] {
            std::fs::write(
                home.path().join(f),
                "export PATH=\"$HOME/.local/bin:$PATH\"\n",
            )
            .unwrap();
        }
        home
    }

    #[test]
    fn zsh_gets_shim_first_despite_user_rc() {
        let home = user_home();
        let base = tempfile::tempdir().unwrap();
        let env = integration_env("/bin/zsh", Path::new("/shim/dir"), base.path());
        if let Some(first) = first_path_entry("/bin/zsh", home.path(), &env) {
            assert_eq!(first, "/shim/dir");
        }
        // Control: without integration the user's directory wins.
        if let Some(first) = first_path_entry("/bin/zsh", home.path(), &[]) {
            assert!(first.ends_with(".local/bin"), "{first}");
        }
    }

    #[test]
    fn zsh_user_zshenv_still_runs_and_zdotdir_is_restored() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".zshenv"), "export FROM_USER_ZSHENV=1\n").unwrap();
        let base = tempfile::tempdir().unwrap();
        let env = integration_env("/bin/zsh", Path::new("/shim/dir"), base.path());
        if !Path::new("/bin/zsh").exists() {
            return;
        }
        let mut child = Command::new("/bin/zsh")
            .args(["-i"])
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", "/usr/bin:/bin")
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"echo U=$FROM_USER_ZSHENV Z=${ZDOTDIR:-unset}\nexit\n")
            .unwrap();
        let out = String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).to_string();
        assert!(out.contains("U=1 Z=unset"), "{out}");
    }

    #[test]
    fn bash_gets_shim_first_despite_user_rc() {
        let home = user_home();
        let base = tempfile::tempdir().unwrap();
        let env = integration_env("/bin/bash", Path::new("/shim/dir"), base.path());
        if let Some(first) = first_path_entry("/bin/bash", home.path(), &env) {
            assert_eq!(first, "/shim/dir");
        }
    }

    #[test]
    fn unknown_shells_get_nothing() {
        let base = tempfile::tempdir().unwrap();
        assert!(integration_env("/usr/bin/fish", Path::new("/s"), base.path()).is_empty());
    }

    #[test]
    fn zdotdir_is_rewritten_when_stale() {
        let base = tempfile::tempdir().unwrap();
        let dir = ensure_zdotdir(base.path()).unwrap();
        std::fs::write(dir.join(".zshenv"), "old").unwrap();
        ensure_zdotdir(base.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(".zshenv")).unwrap(),
            ZSHENV
        );
    }
}
