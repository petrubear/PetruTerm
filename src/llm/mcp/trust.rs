use std::path::{Path, PathBuf};

fn trust_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("petruterm/mcp_trust.json"))
}

/// Returns true if `cwd` has been explicitly trusted to load project-local MCP servers.
pub fn is_trusted(cwd: &Path) -> bool {
    let canon = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let Some(path) = trust_file() else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(list) = serde_json::from_str::<Vec<String>>(&text) else {
        return false;
    };
    list.iter().any(|s| Path::new(s) == canon)
}

/// Mark `cwd` as trusted and persist to `~/.config/petruterm/mcp_trust.json`.
pub fn trust(cwd: &Path) -> anyhow::Result<()> {
    let canon = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let path = trust_file().ok_or_else(|| anyhow::anyhow!("no config dir available"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut list: Vec<String> = if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        vec![]
    };
    let s = canon.to_string_lossy().into_owned();
    if !list.contains(&s) {
        list.push(s);
        std::fs::write(&path, serde_json::to_string_pretty(&list)?)?;
    }
    Ok(())
}
