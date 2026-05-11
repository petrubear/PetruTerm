use super::UiManager;

impl UiManager {
    /// Poll for async git branch results and refresh the cache if due.
    /// Returns true if the cache was updated (caller should redraw).
    pub fn poll_git_branch(
        &mut self,
        cwd: Option<&std::path::Path>,
        dirty_check: bool,
        ttl: std::time::Duration,
    ) -> bool {
        // Drain any result that arrived from a previous spawn.
        let mut updated = false;
        while let Ok(branch) = self.git_rx.try_recv() {
            log::debug!("Git branch fetch completed: '{}'", branch);
            self.git_branch_cache = Some(branch);
            self.git_branch_fetched_at = Some(std::time::Instant::now());
            self.git_branch_in_flight = false;
            self.git_branch_spawn_time = None;
            updated = true;
        }

        // TD-PERF-19: Timeout recovery — if fetch is stuck in-flight for >30s, reset flag
        // without clearing the cache (keep stale result visible).
        if self.git_branch_in_flight {
            if let Some(spawn_time) = self.git_branch_spawn_time {
                if spawn_time.elapsed() > std::time::Duration::from_secs(30) {
                    log::warn!(
                        "Git branch fetch stuck for >30s, resetting in-flight flag (cache remains stale)"
                    );
                    self.git_branch_in_flight = false;
                    self.git_branch_spawn_time = None;
                }
            }
        }

        // Decide whether to spawn a fresh fetch.
        let cwd_changed = cwd.map(|p| p.to_path_buf()) != self.git_branch_cwd;
        let ttl_expired = self
            .git_branch_fetched_at
            .map(|t| t.elapsed() > ttl)
            .unwrap_or(true);

        if (cwd_changed || ttl_expired) && !self.git_branch_in_flight {
            if let Some(cwd_path) = cwd {
                self.git_branch_cwd = Some(cwd_path.to_path_buf());
                self.git_branch_in_flight = true;
                self.git_branch_spawn_time = Some(std::time::Instant::now());
                let tx = self.git_tx.clone();
                let cwd_owned = cwd_path.to_path_buf();
                log::debug!("Spawning git branch fetch for CWD: {:?}", cwd_owned);
                self.tokio_rt.spawn(async move {
                    let branch = fetch_git_branch(&cwd_owned, dirty_check).await;
                    let _ = tx.send(branch);
                });
            }
        }

        updated
    }

    /// Open the command palette in branch-picker mode.
    /// The palette opens immediately with a placeholder; branches populate async (TD-PERF-25).
    pub fn open_branch_picker(&mut self, cwd: &std::path::Path) {
        use crate::ui::palette::{Action, PaletteAction};
        use rust_i18n::t;
        let placeholder = vec![PaletteAction {
            name: t!("ai.loading_branches").to_string(),
            action: Action::Noop,
            keybind: None,
        }];
        self.palette.open_with_items(placeholder);
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.branch_scan_rx = Some(rx);
        self.branch_scan_cwd = Some(cwd.to_path_buf());
        let cwd_owned = cwd.to_path_buf();
        std::thread::spawn(move || {
            let branches = list_git_branches_sync(&cwd_owned);
            let _ = tx.send(branches);
        });
    }

    /// Drain branch scan results and repopulate the palette. Returns true if updated.
    pub fn poll_branch_scan(&mut self) -> bool {
        let Some(rx) = &self.branch_scan_rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(branches) => {
                self.branch_scan_rx = None;
                if branches.is_empty() {
                    self.palette.close();
                    self.branch_scan_cwd = None;
                    return true;
                }
                use crate::ui::palette::{Action, PaletteAction};
                let current = self
                    .git_branch_cache
                    .as_deref()
                    .unwrap_or("")
                    .trim_end_matches('*');
                let items: Vec<PaletteAction> = branches
                    .into_iter()
                    .map(|b| {
                        let label = if b == current {
                            format!("  {b}  ✓")
                        } else {
                            format!("  {b}")
                        };
                        PaletteAction {
                            name: label,
                            action: Action::GitCheckout(b),
                            keybind: None,
                        }
                    })
                    .collect();
                self.palette.open_with_items(items);
                self.branch_scan_cwd = None;
                true
            }
            Err(_) => false,
        }
    }

    /// Run `git checkout <branch>` in `cwd` and invalidate the branch cache.
    pub fn git_checkout(&mut self, branch: &str, cwd: &std::path::Path) {
        let status = std::process::Command::new("git")
            .args(["-C", &cwd.to_string_lossy(), "checkout", branch])
            .status();
        match status {
            Ok(s) if s.success() => {
                // Invalidate cache so the status bar refreshes immediately.
                self.git_branch_cache = None;
                self.git_branch_fetched_at = None;
            }
            Ok(s) => log::warn!("git checkout {branch} exited with {s}"),
            Err(e) => log::error!("git checkout {branch} failed: {e}"),
        }
    }

    /// Open the command palette in theme-picker mode.
    /// Lists .lua files in ~/.config/petruterm/themes/ and pre-populates the palette.
    pub fn open_theme_picker(&mut self) {
        use crate::ui::palette::{Action, PaletteAction};
        let themes = crate::config::list_themes();
        if themes.is_empty() {
            log::warn!(
                "No themes found in {}",
                crate::config::themes_dir().display()
            );
            return;
        }
        let items: Vec<PaletteAction> = themes
            .into_iter()
            .map(|name| PaletteAction {
                name: format!("  {name}"),
                action: Action::SwitchTheme(name),
                keybind: None,
            })
            .collect();
        self.palette.open_with_items(items);
    }
}

/// Async helper: fetch the current git branch for `cwd`.
/// Returns the branch name (with dirty `*` suffix if uncommitted changes),
/// or an empty string if `cwd` is not a git repo.
async fn fetch_git_branch(cwd: &std::path::Path, dirty_check: bool) -> String {
    use tokio::process::Command;

    let branch = Command::new("git")
        .args(["-C", &cwd.to_string_lossy(), "branch", "--show-current"])
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if branch.is_empty() {
        return String::new();
    }

    if !dirty_check {
        return branch;
    }

    let dirty = Command::new("git")
        .args(["-C", &cwd.to_string_lossy(), "status", "--porcelain"])
        .output()
        .await
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    if dirty {
        format!("{branch}*")
    } else {
        branch
    }
}

/// Sync helper: list local git branches for `cwd` (runs in a background thread).
/// Returns branch names sorted alphabetically, empty vec if not a git repo.
fn list_git_branches_sync(cwd: &std::path::Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &cwd.to_string_lossy(),
            "branch",
            "--format=%(refname:short)",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let mut branches: Vec<String> = out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    branches.sort();
    branches
}
