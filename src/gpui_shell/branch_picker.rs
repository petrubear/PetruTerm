// gpui chrome migration (M5c Task 4): the git-branch picker. Mirrors
// `src/app/ui/git.rs`'s own `open_branch_picker`/`poll_branch_scan`/
// `git_checkout` (std::thread::spawn + crossbeam_channel, zero winit
// coupling) -- the same shape `status_bar/git.rs`'s own `poll_git_
// branch` already proved out in `gpui_shell` for the status bar's
// branch display. `list_git_branches_sync` is ported verbatim (it was
// a private free function in `git.rs`, not reusable directly).

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Open the palette in branch-picker mode: a loading placeholder
    /// immediately, real branch names populate async via `poll_branch_
    /// scan` (`poll.rs`'s own 33ms tick).
    pub(super) fn open_branch_picker(&mut self, cwd: &std::path::Path) {
        use crate::ui::palette::{Action, PaletteAction};
        let placeholder = vec![PaletteAction {
            name: "Loading branches…".to_string(),
            action: Action::Noop,
            keybind: None,
        }];
        self.palette.open_with_items(placeholder);
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.branch_scan_rx = Some(rx);
        let cwd_owned = cwd.to_path_buf();
        std::thread::spawn(move || {
            let branches = list_git_branches_sync(&cwd_owned);
            let _ = tx.send(branches);
        });
    }

    /// Drain a completed branch scan and repopulate the palette. Returns
    /// `true` if it updated anything (caller should `cx.notify()`).
    /// Called from `poll.rs`'s existing 33ms tick.
    pub(super) fn poll_branch_scan(&mut self) -> bool {
        let Some(rx) = &self.branch_scan_rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(branches) => {
                self.branch_scan_rx = None;
                if branches.is_empty() {
                    self.palette.close();
                    return true;
                }
                use crate::ui::palette::{Action, PaletteAction};
                let current = self
                    .git_branch
                    .cache
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
                true
            }
            Err(_) => false,
        }
    }
}

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
