// gpui chrome migration (M2 Task 6a): `ExitCodeState`, split out of
// `status_bar.rs` for the 400-line convention.

use std::time::SystemTime;

// ── Exit-code polling ────────────────────────────────────────────────────────

/// Mtime-gated exit-code cache for the focused pane's shell (TD-PERF-09 port
/// of `src/app/app_state.rs`'s `update_terminal_shell_ctx`'s mtime check,
/// plus `frame.rs`'s "only cache when non-zero" rule) -- refreshed once per
/// poll tick, same cadence as the git-branch poll.
#[derive(Default)]
pub struct ExitCodeState {
    /// `Some(code)` only when the last command's exit code was non-zero,
    /// matching `StatusBar::build`'s own "only show on failure" contract.
    pub cache: Option<i32>,
    pid: Option<u32>,
    mtime: Option<SystemTime>,
}

impl ExitCodeState {
    /// Refresh the cache for `pid`'s shell-context file. Returns true if
    /// `cache` changed (caller should redraw). Skips the disk read entirely
    /// when the file's mtime hasn't moved since the last call for this pid.
    pub fn poll(&mut self, pid: u32) -> bool {
        let path = crate::llm::shell_context::ShellContext::context_file_path_for_pid(pid);
        self.poll_impl(pid, &path)
    }

    /// Pid-aware core: resets bookkeeping (and, critically, the cached
    /// value itself) whenever `pid` differs from the last call, then defers
    /// to `poll_path` for the mtime-gated read. Split out from `poll` so a
    /// test can drive it against tempfiles for two different pids without
    /// touching the real per-pid cache path.
    ///
    /// The `self.cache = None` reset (not just `self.mtime = None`) matters:
    /// without it, a pid switch whose new shell-context file hasn't been
    /// written yet (`std::fs::metadata` fails, `poll_path` returns early
    /// without touching `cache`) would keep showing the *previous* pane's
    /// exit code, since nothing else distinguishes "same pid, file
    /// unchanged" from "different pid, file not there yet". This mirrors
    /// `src/app/app_state.rs`'s `terminal_shell_ctxs: HashMap<terminal_id,
    /// (ShellContext, mtime)>`, which keys the cache per terminal so
    /// switching panes always reads that pane's own (possibly-empty) entry
    /// and never carries over another pane's value.
    fn poll_impl(&mut self, pid: u32, path: &std::path::Path) -> bool {
        let old_cache = self.cache;
        if self.pid != Some(pid) {
            self.pid = Some(pid);
            self.mtime = None; // force a reread for the newly-focused pane
            self.cache = None; // never leak the previous pane's exit code
        }
        Self::poll_path(path, &mut self.mtime, &mut self.cache);
        self.cache != old_cache
    }

    /// Path-parametrized core so the mtime-gating + "only cache non-zero"
    /// logic is unit-testable against a tempfile, rather than the real
    /// per-pid path under `$XDG_CACHE_HOME`/`~/.cache` a test shouldn't touch.
    fn poll_path(
        path: &std::path::Path,
        mtime: &mut Option<SystemTime>,
        cache: &mut Option<i32>,
    ) -> bool {
        let Ok(new_mtime) = std::fs::metadata(path).and_then(|m| m.modified()) else {
            return false;
        };
        if *mtime == Some(new_mtime) {
            return false;
        }
        *mtime = Some(new_mtime);
        let Ok(data) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(ctx) = serde_json::from_str::<crate::llm::shell_context::ShellContext>(&data) else {
            return false;
        };
        let new_cache = (ctx.last_exit_code != 0).then_some(ctx.last_exit_code);
        let changed = *cache != new_cache;
        *cache = new_cache;
        changed
    }
}

#[cfg(test)]
mod exit_code_state_tests {
    use super::ExitCodeState;

    fn tempdir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "petruterm-status-bar-test-{tag}-{}-{}",
            std::process::id(),
            tag
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_nonzero_exit_code_and_skips_reload_when_mtime_unchanged() {
        let dir = tempdir("nonzero");
        let path = dir.join("ctx.json");
        std::fs::write(
            &path,
            r#"{"cwd":"/x","last_command":"false","last_exit_code":1}"#,
        )
        .unwrap();

        let mut mtime = None;
        let mut cache = None;
        assert!(ExitCodeState::poll_path(&path, &mut mtime, &mut cache));
        assert_eq!(cache, Some(1));

        // Second call, file untouched: mtime-gated, no reload, no change.
        assert!(!ExitCodeState::poll_path(&path, &mut mtime, &mut cache));
        assert_eq!(cache, Some(1));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clears_cache_when_exit_code_returns_to_zero() {
        let dir = tempdir("zero");
        let path = dir.join("ctx.json");
        std::fs::write(
            &path,
            r#"{"cwd":"/x","last_command":"true","last_exit_code":0}"#,
        )
        .unwrap();

        let mut mtime = None;
        let mut cache = Some(1); // previously nonzero
        assert!(ExitCodeState::poll_path(&path, &mut mtime, &mut cache));
        assert_eq!(cache, None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_a_noop() {
        let mut mtime = None;
        let mut cache = None;
        let missing = std::path::Path::new("/nonexistent/petruterm-status-bar-test.json");
        assert!(!ExitCodeState::poll_path(missing, &mut mtime, &mut cache));
        assert_eq!(cache, None);
    }

    // ── Regression: switching pid must never leak the previous pane's cache ──

    #[test]
    fn switching_pid_clears_stale_cache_before_the_new_pane_has_its_own_file() {
        let dir_a = tempdir("pidswitch-a");
        let path_a = dir_a.join("ctx.json");
        std::fs::write(
            &path_a,
            r#"{"cwd":"/x","last_command":"false","last_exit_code":1}"#,
        )
        .unwrap();
        // Pane B's shell-context file doesn't exist yet -- e.g. a freshly
        // split/switched-to pane whose shell integration (.zshrc/nvm/p10k)
        // hasn't written it out yet.
        let missing_path_b =
            std::path::Path::new("/nonexistent/petruterm-status-bar-test-pid-b.json");

        let mut state = ExitCodeState::default();

        assert!(state.poll_impl(100, &path_a));
        assert_eq!(state.cache, Some(1));

        // Switching focus to a different pid whose file isn't there yet must
        // clear the stale value immediately, not keep showing pid 100's
        // exit code until pid 200 eventually writes its own file.
        assert!(state.poll_impl(200, missing_path_b));
        assert_eq!(state.cache, None);

        let _ = std::fs::remove_dir_all(&dir_a);
    }

    #[test]
    fn switching_pid_immediately_reflects_the_new_pids_own_value() {
        let dir_a = tempdir("pidswitch-c-a");
        let path_a = dir_a.join("ctx.json");
        std::fs::write(
            &path_a,
            r#"{"cwd":"/x","last_command":"false","last_exit_code":1}"#,
        )
        .unwrap();
        let dir_b = tempdir("pidswitch-c-b");
        let path_b = dir_b.join("ctx.json");
        std::fs::write(
            &path_b,
            r#"{"cwd":"/y","last_command":"false","last_exit_code":42}"#,
        )
        .unwrap();

        let mut state = ExitCodeState::default();
        assert!(state.poll_impl(100, &path_a));
        assert_eq!(state.cache, Some(1));

        assert!(state.poll_impl(200, &path_b));
        assert_eq!(state.cache, Some(42));

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }
}
