// The git-branch async bridge.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── Git-branch async bridge ──────────────────────────────────────────────────
//
// Mirrors `config_watch.rs`'s `PENDING_CONFIG_RELOAD`/`CONFIG_CHANGED`
// bridge: gpui 0.2.2 has no `spawn_blocking`-style bridge from
// `BackgroundExecutor` to drive a true cross-thread wake, so a background
// tokio task writes into a static slot and the poll loop (`poll.rs`) reads
// it. One window, one `GpuiShellRoot`, so a single static slot
// is safe the same way `PENDING_CONFIG_RELOAD` already is -- a second
// concurrent consumer would race over it, but nothing here creates one.
static PENDING_GIT_BRANCH: Mutex<Option<String>> = Mutex::new(None);
static GIT_BRANCH_READY: AtomicBool = AtomicBool::new(false);

/// Cached git-branch poll state, living as a plain field on `GpuiShellRoot`
/// (matching how `cursor_blink_on`/`cursor_last_blink` already do) rather
/// than a separate manager type or a thread_local.
#[derive(Default)]
pub struct GitBranchState {
    pub cache: Option<String>,
    cwd: Option<PathBuf>,
    fetched_at: Option<Instant>,
    in_flight: bool,
    spawn_time: Option<Instant>,
}

/// Ported verbatim from `src/app/ui/git.rs:168-202`: runs `git branch
/// --show-current`, then (if `dirty_check`) `git status --porcelain` and
/// appends a `*` suffix if dirty. Empty string if `cwd` isn't a git repo.
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

/// Whether `poll_git_branch` should spawn a fresh fetch this tick. Extracted
/// as a pure function (ported policy from `src/app/ui/git.rs`'s
/// `poll_git_branch`) so the TTL/cwd-changed decision is unit-testable
/// without spawning a real `git` subprocess.
fn should_spawn_fetch(
    state: &GitBranchState,
    cwd: Option<&std::path::Path>,
    ttl: Duration,
) -> bool {
    let cwd_changed = cwd.map(|p| p.to_path_buf()) != state.cwd;
    let ttl_expired = state.fetched_at.map(|t| t.elapsed() > ttl).unwrap_or(true);
    (cwd_changed || ttl_expired) && !state.in_flight
}

/// TD-PERF-19 port: if a fetch has been in flight for more than 30s, reset
/// the flag without clearing the cache (keep the stale branch name visible)
/// rather than let a hung `git` process wedge the indicator forever.
/// Returns true if a stuck fetch was recovered.
fn recover_stuck_in_flight(state: &mut GitBranchState) -> bool {
    if state.in_flight {
        if let Some(spawn_time) = state.spawn_time {
            if spawn_time.elapsed() > Duration::from_secs(30) {
                log::warn!(
                    "gpui-shell: git branch fetch stuck for >30s, resetting in-flight flag (cache remains stale)"
                );
                state.in_flight = false;
                state.spawn_time = None;
                return true;
            }
        }
    }
    false
}

/// Call once per poll-loop tick with the currently-focused terminal's cwd.
/// Drains any completed fetch from the static bridge above, recovers a
/// fetch stuck in flight for more than 30s, then spawns a fresh one onto
/// `tokio_rt` when the cwd changed or `ttl` has expired and none is already
/// running. Returns true when `state.cache` changed (caller should redraw).
pub fn poll_git_branch(
    state: &mut GitBranchState,
    cwd: Option<&std::path::Path>,
    dirty_check: bool,
    ttl: Duration,
    tokio_rt: &tokio::runtime::Runtime,
) -> bool {
    let mut updated = false;

    if GIT_BRANCH_READY.swap(false, Ordering::AcqRel) {
        if let Some(branch) = PENDING_GIT_BRANCH.lock().unwrap().take() {
            log::debug!("gpui-shell: git branch fetch completed: '{branch}'");
            state.cache = Some(branch);
            state.fetched_at = Some(Instant::now());
            state.in_flight = false;
            state.spawn_time = None;
            updated = true;
        }
    }

    recover_stuck_in_flight(state);

    if should_spawn_fetch(state, cwd, ttl) {
        if let Some(cwd_path) = cwd {
            state.cwd = Some(cwd_path.to_path_buf());
            state.in_flight = true;
            state.spawn_time = Some(Instant::now());
            let cwd_owned = cwd_path.to_path_buf();
            log::debug!("gpui-shell: spawning git branch fetch for CWD: {cwd_owned:?}");
            tokio_rt.spawn(async move {
                let branch = fetch_git_branch(&cwd_owned, dirty_check).await;
                *PENDING_GIT_BRANCH.lock().unwrap() = Some(branch);
                GIT_BRANCH_READY.store(true, Ordering::Release);
            });
        }
    }

    updated
}

#[cfg(test)]
mod git_branch_state_tests {
    use super::{recover_stuck_in_flight, should_spawn_fetch, GitBranchState};
    use std::time::{Duration, Instant};

    #[test]
    fn spawns_when_cwd_changed() {
        let state = GitBranchState {
            cwd: Some(std::path::PathBuf::from("/a")),
            fetched_at: Some(Instant::now()),
            ..Default::default()
        };
        let new_cwd = std::path::PathBuf::from("/b");
        assert!(should_spawn_fetch(
            &state,
            Some(&new_cwd),
            Duration::from_secs(15)
        ));
    }

    #[test]
    fn spawns_when_ttl_expired() {
        let cwd = std::path::PathBuf::from("/a");
        let state = GitBranchState {
            cwd: Some(cwd.clone()),
            fetched_at: Some(Instant::now() - Duration::from_secs(20)),
            ..Default::default()
        };
        assert!(should_spawn_fetch(
            &state,
            Some(&cwd),
            Duration::from_secs(15)
        ));
    }

    #[test]
    fn does_not_spawn_while_in_flight_even_if_ttl_expired() {
        let state = GitBranchState {
            in_flight: true,
            fetched_at: None,
            ..Default::default()
        };
        let cwd = std::path::PathBuf::from("/a");
        assert!(!should_spawn_fetch(
            &state,
            Some(&cwd),
            Duration::from_secs(15)
        ));
    }

    #[test]
    fn does_not_spawn_when_fresh_and_cwd_unchanged() {
        let cwd = std::path::PathBuf::from("/a");
        let state = GitBranchState {
            cwd: Some(cwd.clone()),
            fetched_at: Some(Instant::now()),
            ..Default::default()
        };
        assert!(!should_spawn_fetch(
            &state,
            Some(&cwd),
            Duration::from_secs(15)
        ));
    }

    #[test]
    fn recovers_a_fetch_stuck_for_more_than_30s_without_clearing_cache() {
        let mut state = GitBranchState {
            cache: Some("main".to_string()),
            in_flight: true,
            spawn_time: Some(Instant::now() - Duration::from_secs(35)),
            ..Default::default()
        };
        assert!(recover_stuck_in_flight(&mut state));
        assert!(!state.in_flight);
        assert_eq!(state.cache.as_deref(), Some("main"));
    }

    #[test]
    fn leaves_a_recent_in_flight_fetch_alone() {
        let mut state = GitBranchState {
            in_flight: true,
            spawn_time: Some(Instant::now()),
            ..Default::default()
        };
        assert!(!recover_stuck_in_flight(&mut state));
        assert!(state.in_flight);
    }
}
