use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// Validate that `path` is safe to read or write:
/// - Resolves to an absolute path (relative paths are resolved against $HOME).
/// - Must not escape `$HOME` (mirrors AUDIT-SEC-01 logic).
///
/// Walks up to the nearest existing ancestor and canonicalizes that instead of
/// `abs` directly, so `..` segments and symlinks are resolved even when `abs`
/// itself doesn't exist yet (e.g. writing a brand-new file).
pub fn validate_path(path: &Path) -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine $HOME"))?;
    let home = home.canonicalize().unwrap_or(home);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        home.join(path)
    };

    let mut probe = abs.clone();
    let is_safe = loop {
        match probe.canonicalize() {
            Ok(canon) => break canon.starts_with(&home),
            Err(_) => {
                if !probe.pop() {
                    break false;
                }
            }
        }
    };

    if !is_safe {
        bail!(
            "path {} is outside $HOME — refusing to access",
            abs.display()
        );
    }
    Ok(abs)
}
