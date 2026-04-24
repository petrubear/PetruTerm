use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Configuration for a single MCP server process.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpServerConfig {
    /// Executable to spawn (e.g. "npx", "node", "python").
    pub command: String,
    /// Arguments passed to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables injected into the server process.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Map of server name → server config.
/// This is the public surface consumed by D-2 (McpClient).
pub type McpConfig = HashMap<String, McpServerConfig>;

/// Internal: mirrors the top-level JSON structure `{ "mcpServers": { ... } }`.
#[derive(Debug, Deserialize, Default)]
struct McpFile {
    #[serde(rename = "mcpServers", default)]
    servers: McpConfig,
}

/// Load and merge MCP server configs from global and project-local sources.
///
/// Resolution order (last wins on name conflict):
/// 1. `{config_dir}/petruterm/mcp/mcp.json`  — platform config dir
///    - macOS: `~/Library/Application Support/petruterm/mcp/mcp.json`
///    - Linux: `~/.config/petruterm/mcp/mcp.json`
/// 2. `~/.config/petruterm/mcp/mcp.json`     — XDG fallback (macOS only, if different from above)
/// 3. `<cwd>/.petruterm/mcp.json`            — project-local (highest priority)
///
/// Missing files are silently skipped. Malformed JSON returns `Err`.
pub fn load(cwd: &Path) -> Result<McpConfig> {
    let mut config = McpConfig::new();

    // 1. Platform config dir (~/Library/Application Support on macOS, ~/.config on Linux)
    let platform_path = dirs::config_dir().map(|d| d.join("petruterm/mcp/mcp.json"));
    if let Some(ref p) = platform_path {
        if p.exists() {
            let servers =
                parse_file(p).with_context(|| format!("Failed to parse {}", p.display()))?;
            config.extend(servers);
        }
    }

    // 2. XDG fallback: ~/.config/petruterm/mcp/mcp.json
    //    On macOS, dirs::config_dir() returns ~/Library/Application Support, so
    //    ~/.config is a separate location that many users expect to work.
    if let Some(home) = dirs::home_dir() {
        let xdg_path = home.join(".config/petruterm/mcp/mcp.json");
        let already_loaded = platform_path.as_ref().map_or(false, |p| p == &xdg_path);
        if !already_loaded && xdg_path.exists() {
            let servers = parse_file(&xdg_path)
                .with_context(|| format!("Failed to parse {}", xdg_path.display()))?;
            config.extend(servers);
        }
    }

    // 3. Project-local config (overrides global on name conflict)
    let local_path = cwd.join(".petruterm/mcp.json");
    if local_path.exists() {
        let servers = parse_file(&local_path)
            .with_context(|| format!("Failed to parse {}", local_path.display()))?;
        config.extend(servers);
    }

    Ok(config)
}

fn parse_file(path: &Path) -> Result<McpConfig> {
    let text = std::fs::read_to_string(path)?;
    let file: McpFile = serde_json::from_str(&text)?;
    Ok(file.servers)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn parse_valid_json() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "mcp.json",
            r#"{ "mcpServers": { "fs": { "command": "npx", "args": ["--yes", "server-fs"] } } }"#,
        );
        let servers = parse_file(&dir.path().join("mcp.json")).unwrap();
        assert_eq!(servers["fs"].command, "npx");
        assert_eq!(servers["fs"].args, vec!["--yes", "server-fs"]);
        assert!(servers["fs"].env.is_empty());
    }

    #[test]
    fn missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        // load() with no files on disk → empty config, no error
        let config = load(dir.path()).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn local_overrides_global() {
        // We can't easily override dirs::config_dir(), so we test the merge
        // logic directly by calling parse_file + extend.
        let dir = TempDir::new().unwrap();

        write(
            dir.path(),
            "global.json",
            r#"{ "mcpServers": { "shared": { "command": "global-cmd" }, "only-global": { "command": "og" } } }"#,
        );
        write(
            dir.path(),
            "local.json",
            r#"{ "mcpServers": { "shared": { "command": "local-cmd" } } }"#,
        );

        let mut config = parse_file(&dir.path().join("global.json")).unwrap();
        config.extend(parse_file(&dir.path().join("local.json")).unwrap());

        assert_eq!(config["shared"].command, "local-cmd");
        assert_eq!(config["only-global"].command, "og");
    }

    #[test]
    fn malformed_json_returns_err() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "bad.json", "{ not valid json }");
        assert!(parse_file(&dir.path().join("bad.json")).is_err());
    }

    #[test]
    fn env_vars_parsed() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "mcp.json",
            r#"{ "mcpServers": { "srv": { "command": "cmd", "env": { "FOO": "bar" } } } }"#,
        );
        let servers = parse_file(&dir.path().join("mcp.json")).unwrap();
        assert_eq!(servers["srv"].env["FOO"], "bar");
    }
}
