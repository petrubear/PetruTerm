use std::path::PathBuf;
use std::sync::LazyLock;
use regex::Regex;
use serde::Deserialize;

static EXPORT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"(?i)(password|token|key|secret|auth|pass|pwd)=[^ \t\n\r|;&<>]+"##).unwrap()
});
static AUTH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"(?i)(-H|--header)\s+['"]?(authorization|x-api-key):\s*[^ \t\n\r'"|;&<>]+['"]?"##).unwrap()
});

/// Shell state written by `scripts/shell-integration.zsh` after each command.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShellContext {
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub last_command: String,
    #[serde(default)]
    pub last_exit_code: i32,
}

impl ShellContext {
    pub fn context_file_path() -> PathBuf {
        let cache_base = std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir().unwrap_or_default().join(".cache")
            });
        cache_base.join("petruterm").join("shell-context.json")
    }

    /// Load from the JSON file. Returns `None` if missing or unparseable.
    pub fn load() -> Option<Self> {
        let data = std::fs::read_to_string(Self::context_file_path()).ok()?;
        serde_json::from_str(&data).ok()
    }

    #[allow(dead_code)]
    pub fn has_failed_exit(&self) -> bool {
        self.last_exit_code != 0
    }

    /// Redact sensitive information like API keys, tokens, and passwords from commands.
    pub fn sanitize_command(cmd: &str) -> String {
        let cmd = EXPORT_REGEX.replace_all(cmd, "$1=[REDACTED]");
        let cmd = AUTH_REGEX.replace_all(&cmd, "$1 $2: [REDACTED]");
        cmd.to_string()
    }

    /// One-paragraph summary suitable for appending to a system message.
    pub fn format_for_system_message(&self) -> String {
        let mut parts = Vec::new();
        if !self.cwd.is_empty() {
            parts.push(format!("Current directory: {}", self.cwd));
        }
        if !self.last_command.is_empty() {
            let sanitized = Self::sanitize_command(&self.last_command);
            parts.push(format!("Last command: {}", sanitized));
        }
        if self.last_exit_code != 0 {
            parts.push(format!(
                "Last exit code: {} (non-zero — the command failed)",
                self.last_exit_code
            ));
        }
        parts.join("\n")
    }
}
