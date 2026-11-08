//! Helpers shared by the per-agent hook-file installers: marker detection, load/write with a
//! backup, and the strip/count logic for the `event -> [group or handler]` layout that Codex
//! and Antigravity use. See `specs/004-agent-integrations/contracts/agent-hook-files.md`.

use std::path::Path;

use serde_json::{Map, Value};

/// Every hook command we install contains this; it is how an entry is recognized as ours.
pub const MARKER: &str = "PETRUTERM_BIN";

/// True when any string in the subtree contains the marker.
pub fn contains_marker(v: &Value) -> bool {
    match v {
        Value::String(s) => s.contains(MARKER),
        Value::Array(a) => a.iter().any(contains_marker),
        Value::Object(o) => o.values().any(contains_marker),
        _ => false,
    }
}

/// Read a JSON file. `Ok(None)` when it does not exist; an empty file is `{}`; invalid JSON is
/// an error that says the file is left alone. Returns the original text and the parsed value.
pub fn load_json(path: &Path) -> Result<Option<(String, Value)>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let value = if text.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&text).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); not touching it",
                path.display()
            )
        })?
    };
    Ok(Some((text, value)))
}

/// Path of the backup written next to `path`.
pub fn backup_path(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".petruterm-bak");
    s.into()
}

/// Write `doc` to `path`, creating parent directories. The first time an existing file is
/// changed it is copied to `<path>.petruterm-bak` (an earlier backup is never overwritten).
/// Nothing is written on a dry run. Returns a one-line report.
pub fn write_json(
    path: &Path,
    doc: &Value,
    existing: Option<&str>,
    dry_run: bool,
) -> Result<String, String> {
    if dry_run {
        return Ok(format!("{}: would be written (--dry-run)", path.display()));
    }
    let bak = backup_path(path);
    let mut note = String::new();
    match existing {
        Some(text) if !bak.exists() => {
            std::fs::write(&bak, text).map_err(|e| format!("cannot write backup: {e}"))?;
            note = format!(" (backup: {})", bak.display());
        }
        Some(_) => {}
        None => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
            }
        }
    }
    let out = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
    std::fs::write(path, out + "\n")
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(format!("{}: written{note}", path.display()))
}

/// A matcher group (`{"matcher"?, "hooks": []}`), an empty array or an empty object is dead.
fn is_dead(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => {
            o.is_empty()
                || (o.keys().all(|k| k == "matcher" || k == "hooks")
                    && o.get("hooks")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty))
        }
        _ => false,
    }
}

/// Remove every handler of ours from an `event -> [group | handler]` map, then drop the groups
/// and events that became empty. Returns how many handlers were removed. Entries without the
/// marker are never touched.
pub fn strip_marked(events: &mut Map<String, Value>) -> usize {
    let mut removed = 0;
    for list in events.values_mut() {
        let Some(items) = list.as_array_mut() else {
            continue;
        };
        for item in items.iter_mut() {
            if let Some(handlers) = item.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = handlers.len();
                handlers.retain(|h| !contains_marker(h));
                removed += before - handlers.len();
            }
        }
        let before = items.len();
        // Flat handlers (Antigravity's non-tool events) that are ours.
        items.retain(|i| i.get("hooks").is_some() || !contains_marker(i));
        removed += before - items.len();
        items.retain(|i| !is_dead(i));
    }
    events.retain(|_, v| !is_dead(v));
    removed
}

/// How many handlers of ours an `event -> [group | handler]` map holds.
pub fn count_marked(events: &Map<String, Value>) -> usize {
    events
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .map(|item| match item.get("hooks").and_then(Value::as_array) {
            Some(handlers) => handlers.iter().filter(|h| contains_marker(h)).count(),
            None => usize::from(contains_marker(item)),
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn marker_is_found_anywhere_in_a_subtree() {
        assert!(contains_marker(&json!({"a": [{"b": "x PETRUTERM_BIN y"}]})));
        assert!(!contains_marker(&json!({"a": ["say done", 5, null]})));
    }

    #[test]
    fn load_handles_missing_empty_and_invalid_files() {
        let d = tmp();
        let p = d.path().join("f.json");
        assert!(load_json(&p).unwrap().is_none());
        std::fs::write(&p, "  \n").unwrap();
        assert_eq!(load_json(&p).unwrap().unwrap().1, json!({}));
        std::fs::write(&p, "{ broken").unwrap();
        let e = load_json(&p).unwrap_err();
        assert!(e.contains("not touching it"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ broken");
    }

    #[test]
    fn write_backs_up_once_creates_dirs_and_honors_dry_run() {
        let d = tmp();
        let p = d.path().join("a/b/f.json");
        let r = write_json(&p, &json!({"x": 1}), None, false).unwrap();
        assert!(r.contains("written") && p.exists());
        assert!(!backup_path(&p).exists());
        write_json(&p, &json!({"x": 2}), Some("orig"), false).unwrap();
        assert_eq!(std::fs::read_to_string(backup_path(&p)).unwrap(), "orig");
        write_json(&p, &json!({"x": 3}), Some("second"), false).unwrap();
        assert_eq!(
            std::fs::read_to_string(backup_path(&p)).unwrap(),
            "orig",
            "the first backup is kept"
        );
        let q = d.path().join("never.json");
        let r = write_json(&q, &json!({}), None, true).unwrap();
        assert!(r.contains("dry-run") && !q.exists());
    }

    #[test]
    fn key_order_is_preserved() {
        let v: Value = serde_json::from_str(r#"{"zeta":1,"alpha":2}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"zeta":1,"alpha":2}"#
        );
    }

    fn sample() -> Value {
        json!({
            "PreToolUse": [
                {"matcher": "Bash", "hooks": [{"type": "command", "command": "user-lint"}]},
                {"matcher": "*", "hooks": [{"type": "command", "command": "[ -z \"$PETRUTERM_BIN\" ] || x"}]}
            ],
            "Stop": [{"type": "command", "command": "\"$PETRUTERM_BIN\" hook agy Stop"}],
            "SessionStart": [{"hooks": [{"command": "[ -z \"$PETRUTERM_BIN\" ] || y"}]}]
        })
    }

    #[test]
    fn strip_removes_only_our_handlers_and_prunes() {
        let mut v = sample();
        let events = v.as_object_mut().unwrap();
        assert_eq!(count_marked(events), 3);
        assert_eq!(strip_marked(events), 3);
        assert_eq!(count_marked(events), 0);
        assert_eq!(
            Value::Object(events.clone()),
            json!({"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "user-lint"}]}]})
        );
        // nothing left to remove
        assert_eq!(strip_marked(events), 0);
    }
}
