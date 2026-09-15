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

/// Load MCP server configs from global sources only (platform config dir + XDG fallback).
/// Project-local `.petruterm/mcp.json` is intentionally excluded — use `load_local` for
/// that, guarded by a trust check.
///
/// Resolution order (last wins on name conflict):
/// 1. `{config_dir}/petruterm/mcp/mcp.json`  — platform config dir
///    - macOS: `~/Library/Application Support/petruterm/mcp/mcp.json`
///    - Linux: `~/.config/petruterm/mcp/mcp.json`
/// 2. `~/.config/petruterm/mcp/mcp.json`     — XDG fallback (macOS only, if different from above)
///
/// Missing files are silently skipped. Malformed JSON returns `Err`.
pub fn load_global() -> Result<McpConfig> {
    let platform_path = dirs::config_dir().map(|d| d.join("petruterm/mcp/mcp.json"));
    let xdg_path = dirs::home_dir().map(|home| home.join(".config/petruterm/mcp/mcp.json"));
    let already_loaded =
        matches!((platform_path.as_deref(), xdg_path.as_deref()), (Some(p), Some(x)) if p == x);
    let mut config = McpConfig::new();
    if let Some(p) = platform_path.as_deref().filter(|p| p.exists()) {
        let servers = parse_file(p).with_context(|| format!("Failed to parse {}", p.display()))?;
        config.extend(servers);
    }
    if let Some(p) = xdg_path
        .as_deref()
        .filter(|p| !already_loaded && p.exists())
    {
        let servers = parse_file(p).with_context(|| format!("Failed to parse {}", p.display()))?;
        config.extend(servers);
    }
    Ok(config)
}

/// Load MCP server config from a project-local `.petruterm/mcp.json`.
/// Returns an empty config if the file does not exist.
/// Callers MUST verify trust via `mcp::trust::is_trusted(cwd)` before calling this.
pub fn load_local(cwd: &Path) -> Result<McpConfig> {
    let local_path = cwd.join(".petruterm/mcp.json");
    if !local_path.exists() {
        return Ok(McpConfig::new());
    }
    parse_file(&local_path).with_context(|| format!("Failed to parse {}", local_path.display()))
}

/// Load MCP config merged from global + project-local sources, matching the
/// merge policy three call sites (two in the wgpu binary, one in gpui_shell)
/// previously each duplicated inline. Global config always loads; local
/// config is included only when `trusted` is true. On a `load_global`
/// failure, returns `Err` (callers decide how to degrade — some treat this
/// as "keep whatever was already running," others as "connect with zero
/// MCP servers," which is why this doesn't collapse the error internally).
/// A `load_local` failure is logged and does not fail the whole call — the
/// global-only config is still returned, matching the more lenient of the
/// three call sites this consolidates.
#[allow(dead_code)]
pub fn load_merged(cwd: &Path, trusted: bool) -> Result<McpConfig> {
    let mut cfg = load_global()?;
    let local_path = cwd.join(".petruterm/mcp.json");
    if local_path.exists() {
        if trusted {
            match load_local(cwd) {
                Ok(local) => cfg.extend(local),
                Err(e) => log::warn!("MCP: failed to load local config: {e:#}"),
            }
        } else {
            log::info!(
                "Local MCP config found at {} but this directory is not trusted -- skipping. \
                 Use 'Trust local MCP' in the command palette to enable.",
                local_path.display()
            );
        }
    }
    Ok(cfg)
}

/// Map this project's own MCP config shape to the ACP protocol's server
/// list, for `NewSessionRequest::mcp_servers` -- the ACP agent connects to
/// and calls these servers' tools itself. This project's own `McpManager`
/// (used only for the direct-provider tool-calling path) is entirely
/// separate and untouched by this mapping.
#[allow(dead_code)]
pub fn to_acp_servers(config: &McpConfig) -> Vec<agent_client_protocol::schema::McpServer> {
    config
        .iter()
        .map(|(name, cfg)| {
            let mut server = agent_client_protocol::schema::McpServerStdio::new(
                name.clone(),
                cfg.command.clone(),
            );
            server.args = cfg.args.clone();
            server.env = cfg
                .env
                .iter()
                .map(|(key, value)| {
                    agent_client_protocol::schema::EnvVariable::new(key.clone(), value.clone())
                })
                .collect();
            agent_client_protocol::schema::McpServer::Stdio(server)
        })
        .collect()
}

#[cfg(test)]
fn load_from_paths(
    platform_path: Option<&Path>,
    xdg_path: Option<&Path>,
    local_path: &Path,
) -> Result<McpConfig> {
    let mut config = McpConfig::new();

    if let Some(p) = platform_path.filter(|p| p.exists()) {
        let servers = parse_file(p).with_context(|| format!("Failed to parse {}", p.display()))?;
        config.extend(servers);
    }

    let already_loaded_xdg = matches!((platform_path, xdg_path), (Some(p), Some(x)) if p == x);
    if let Some(p) = xdg_path.filter(|p| !already_loaded_xdg && p.exists()) {
        let servers = parse_file(p).with_context(|| format!("Failed to parse {}", p.display()))?;
        config.extend(servers);
    }

    if local_path.exists() {
        let servers = parse_file(local_path)
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
        let local = dir.path().join(".petruterm/mcp.json");
        let platform = dir.path().join("platform/petruterm/mcp/mcp.json");
        let xdg = dir.path().join(".config/petruterm/mcp/mcp.json");
        let config = load_from_paths(Some(&platform), Some(&xdg), &local).unwrap();
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

    #[test]
    fn load_merged_includes_local_when_trusted() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            ".petruterm/mcp.json",
            r#"{ "mcpServers": { "m5d-test-local-trusted": { "command": "local-cmd" } } }"#,
        );
        let cfg = load_merged(dir.path(), true).unwrap();
        assert_eq!(
            cfg.get("m5d-test-local-trusted")
                .map(|c| c.command.as_str()),
            Some("local-cmd")
        );
    }

    #[test]
    fn load_merged_excludes_local_when_untrusted() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            ".petruterm/mcp.json",
            r#"{ "mcpServers": { "m5d-test-local-untrusted": { "command": "local-cmd" } } }"#,
        );
        let cfg = load_merged(dir.path(), false).unwrap();
        assert!(!cfg.contains_key("m5d-test-local-untrusted"));
    }

    #[test]
    fn load_merged_with_no_local_file_returns_global_only() {
        let dir = TempDir::new().unwrap();
        let cfg = load_merged(dir.path(), true).unwrap();
        assert!(!cfg.contains_key("m5d-test-should-never-exist"));
    }

    #[test]
    fn to_acp_servers_maps_stdio_shape() {
        let mut cfg = McpConfig::new();
        cfg.insert(
            "test-srv".to_string(),
            McpServerConfig {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "pkg".to_string()],
                env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            },
        );
        let servers = to_acp_servers(&cfg);
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            agent_client_protocol::schema::McpServer::Stdio(s) => {
                assert_eq!(s.name, "test-srv");
                assert_eq!(s.command, std::path::PathBuf::from("npx"));
                assert_eq!(s.args, vec!["-y", "pkg"]);
                assert_eq!(s.env.len(), 1);
                assert_eq!(s.env[0].name, "FOO");
                assert_eq!(s.env[0].value, "bar");
            }
            other => panic!("expected Stdio variant, got {other:?}"),
        }
    }

    #[test]
    fn to_acp_servers_on_empty_config_is_empty() {
        let cfg = McpConfig::new();
        assert!(to_acp_servers(&cfg).is_empty());
    }
}
