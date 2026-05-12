use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub version: u32,
    pub name: String,
    pub saved_at: u64,
    pub tabs: Vec<TabSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub title: String,
    pub pane_tree: PaneNodeSnapshot,
    pub accent_color: Option<[f32; 4]>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PaneNodeSnapshot {
    Leaf {
        cwd: String,
    },
    Split {
        dir: SplitDirSnapshot,
        ratio: f32,
        left: Box<PaneNodeSnapshot>,
        right: Box<PaneNodeSnapshot>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirSnapshot {
    Horizontal,
    Vertical,
}

pub struct SavedWorkspaceInfo {
    pub path: PathBuf,
    pub name: String,
    pub tab_count: usize,
    pub saved_at: u64,
}

pub fn workspaces_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("petruterm").join("workspaces"))
}

pub fn list_saved_workspaces() -> Vec<SavedWorkspaceInfo> {
    let Some(dir) = workspaces_dir() else {
        return vec![];
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut results = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(snap) = serde_json::from_str::<WorkspaceSnapshot>(&data) {
                results.push(SavedWorkspaceInfo {
                    name: snap.name.clone(),
                    tab_count: snap.tabs.len(),
                    saved_at: snap.saved_at,
                    path,
                });
            }
        }
    }
    results.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    results
}

pub fn load_workspace(path: &PathBuf) -> Result<WorkspaceSnapshot> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn save_snapshot(snap: &WorkspaceSnapshot) -> Result<()> {
    let dir = workspaces_dir().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(&dir)?;
    let safe_name = snap
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let path = dir.join(format!("{safe_name}.json"));
    let data = serde_json::to_string_pretty(snap)?;
    std::fs::write(&path, data)?;
    Ok(())
}
