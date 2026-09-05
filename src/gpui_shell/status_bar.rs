// gpui chrome migration (M2 Task 5): status bar segments + rendering, plus
// the two small pieces of async/mtime-gated state (git branch, exit code)
// the wgpu app keeps on its own `App`/`UiManager` structs but that live
// directly on `GpuiShellRoot` here (see this task's own design ledger in
// `task-5-report.md` for why).
//
// `StatusBar`/`StatusBarSegment`/`SegmentKind`/`StatusBar::build`/
// `truncate_path`/`format_time` below are a verbatim port of
// `src/ui/status_bar.rs`, minus the pixel-column math
// (`click_kind`/`left_sep_width`/`right_sep_width`) `render_status_bar`
// below replaces with real gpui `div()`s -- the same simplification
// `tabs::render_tab_bar` already got over the wgpu tab bar's own pixel math.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use gpui::{div, prelude::*, Div, MouseButton, MouseDownEvent};
use rust_i18n::t;

use crate::config::schema::{StatusBarColors, StatusBarStyle};

use super::pane_view::to_rgba;

/// Which logical widget a status bar segment represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentKind {
    Leader,
    Cwd,
    GitBranch,
    ExitCode,
    Battery,
    Time,
}

/// A single colored segment in the status bar.
#[derive(Debug, Clone)]
pub struct StatusBarSegment {
    pub text: String,
    pub fg: [f32; 4],
    pub bg: [f32; 4],
    pub kind: SegmentKind,
}

/// Assembled status bar with left and right segment groups.
#[derive(Debug, Clone)]
pub struct StatusBar {
    /// Segments shown on the left.
    pub left: Vec<StatusBarSegment>,
    /// Segments shown on the right.
    pub right: Vec<StatusBarSegment>,
    /// Visual style: plain text separators or Nerd Font powerline arrows.
    pub style: StatusBarStyle,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            left: vec![],
            right: vec![],
            style: StatusBarStyle::Plain,
        }
    }
}

impl StatusBar {
    /// Build the status bar from current application state.
    ///
    /// - `leader_active`: true when the leader key has been pressed and the
    ///   timeout is still running (shows the LEADER segment in theme accent color).
    /// - `leader_resize_mode`: true when leader is active AND the Alt/Option modifier
    ///   is held, indicating the user is about to resize a pane (shows RESIZE in yellow).
    /// - `cwd`: current working directory (None if unavailable).
    /// - `git_branch`: cached git branch string (None if not a git repo or not yet fetched).
    /// - `last_exit_code`: last exit code from shell context (None if unavailable).
    /// - `colors`: theme-derived status bar colors (AUDIT-THEME-01).
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        leader_active: bool,
        leader_resize_mode: bool,
        leader_key: &str,
        cwd: Option<&std::path::Path>,
        git_branch: Option<&str>,
        last_exit_code: Option<i32>,
        pane_zoomed: bool,
        style: StatusBarStyle,
        battery: Option<(u8, bool)>,
        colors: &StatusBarColors,
    ) -> Self {
        let mut bar = StatusBar {
            style,
            ..StatusBar::default()
        };

        // ── Left segments ────────────────────────────────────────────────────

        // Leader-mode indicator.
        let (leader_text, leader_bg) = if leader_resize_mode {
            (t!("status.resize").to_string(), colors.leader_resize)
        } else if leader_active {
            (t!("status.leader").to_string(), colors.leader_active)
        } else {
            (
                format!(" ^{} ", leader_key.to_uppercase()),
                colors.leader_inactive,
            )
        };
        bar.left.push(StatusBarSegment {
            text: leader_text,
            fg: colors.fg_default,
            bg: leader_bg,
            kind: SegmentKind::Leader,
        });

        if pane_zoomed {
            bar.left.push(StatusBarSegment {
                text: " ZOOM ".to_string(),
                fg: colors.fg_default,
                bg: colors.zoom_bg,
                kind: SegmentKind::Leader,
            });
        }

        // Current working directory (truncated).
        if let Some(path) = cwd {
            let display = truncate_path(path, 25);
            bar.left.push(StatusBarSegment {
                text: format!("  {display} "),
                fg: colors.cwd_fg,
                bg: colors.cwd_bg,
                kind: SegmentKind::Cwd,
            });
        }

        // Git branch.
        if let Some(branch) = git_branch {
            if !branch.is_empty() {
                bar.left.push(StatusBarSegment {
                    text: format!("  {branch} "),
                    fg: colors.git_fg,
                    bg: colors.git_bg,
                    kind: SegmentKind::GitBranch,
                });
            }
        }

        // ── Right segments ───────────────────────────────────────────────────

        // Exit code (only shown when non-zero).
        if let Some(code) = last_exit_code {
            if code != 0 {
                bar.right.push(StatusBarSegment {
                    text: t!("status.exit_code", code = code).to_string(),
                    fg: colors.fg_default,
                    bg: colors.error_bg,
                    kind: SegmentKind::ExitCode,
                });
            }
        }

        // Battery — shown only when running on battery power.
        if let Some((percent, true)) = battery {
            let (fg, bg) = if percent < 20 {
                (colors.bat_low_fg, colors.bat_low_bg)
            } else {
                (colors.bat_ok_fg, colors.bat_ok_bg)
            };
            bar.right.push(StatusBarSegment {
                text: format!(" BAT {percent}% "),
                fg,
                bg,
                kind: SegmentKind::Battery,
            });
        }

        // Date + time.
        let time_str = format_time();
        bar.right.push(StatusBarSegment {
            text: format!(" {time_str} "),
            fg: colors.fg_dim,
            bg: colors.bar_bg,
            kind: SegmentKind::Time,
        });

        bar
    }

    /// Background color for empty space between left and right groups, derived from theme.
    pub fn bar_bg(colors: &StatusBarColors) -> [f32; 4] {
        colors.bar_bg
    }

    /// Powerline left arrow glyph (U+E0B0 — solid right-pointing triangle).
    pub fn pl_left_arrow() -> &'static str {
        "\u{E0B0}"
    }

    /// Powerline right arrow glyph (U+E0B2 — solid left-pointing triangle).
    pub fn pl_right_arrow() -> &'static str {
        "\u{E0B2}"
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Shorten a path to at most `max_chars` characters, using `…/` prefix when truncated.
fn truncate_path(path: &std::path::Path, max_chars: usize) -> String {
    let s = path.to_string_lossy();
    // Replace $HOME with ~
    let home = std::env::var("HOME").unwrap_or_default();
    let s = if !home.is_empty() && s.starts_with(&home) {
        format!("~{}", &s[home.len()..])
    } else {
        s.to_string()
    };

    if s.chars().count() <= max_chars {
        return s;
    }

    // Take the last `max_chars - 2` chars with ellipsis prefix.
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(max_chars.saturating_sub(2));
    // Find the next `/` boundary to avoid splitting mid-component.
    let start = chars[start..]
        .iter()
        .position(|&c| c == '/')
        .map(|i| start + i)
        .unwrap_or(start);
    format!("…{}", chars[start..].iter().collect::<String>())
}

/// Format the current time in the system's local timezone as "YYYY-MM-DD HH:MM".
fn format_time() -> String {
    // SAFETY: `t` is a valid non-null `time_t` output pointer target and `tm` is
    // fully written by `localtime_r` before it's read below.
    let tm = unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
    )
}

// ── Git-branch async bridge ──────────────────────────────────────────────────
//
// Mirrors `gpui_shell/mod.rs`'s `PENDING_CONFIG_RELOAD`/`CONFIG_CHANGED`
// bridge (see that file's own doc comment for *why*: gpui 0.2.2 has no
// `spawn_blocking`-style bridge from `BackgroundExecutor` to drive a true
// cross-thread wake, so a background tokio task writes into a static slot
// and the existing ~33ms poll loop in `GpuiShellRoot::new`'s `cx.spawn`
// block reads it). One window, one `GpuiShellRoot`, so a single static slot
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

// ── Rendering ────────────────────────────────────────────────────────────────

/// Render one status-bar row: one `div()` per segment, left-aligned segments
/// then a flexible spacer then right-aligned segments. Replaces the wgpu
/// renderer's pixel-column math (`click_kind`/`left_sep_width`/
/// `right_sep_width`, dropped from this port) with gpui's own flex layout --
/// the same simplification the tab bar already got (`tabs::render_tab_bar`).
///
/// Git-branch and exit-code segments get a click target (`cursor_pointer` +
/// `on_mouse_down`) reserved for the branch picker / exit-info context menu
/// (both M4, command-palette era) -- deliberately a no-op for now, per this
/// task's brief.
pub fn render_status_bar(bar: &StatusBar, colors: &StatusBarColors) -> Div {
    let powerline = bar.style == StatusBarStyle::Powerline;
    let bar_bg_color = to_rgba(StatusBar::bar_bg(colors));
    let sep_fg = to_rgba(colors.fg_dim);

    let segment_div = |seg: &StatusBarSegment| -> Div {
        let clickable = matches!(seg.kind, SegmentKind::GitBranch | SegmentKind::ExitCode);
        let cell = div()
            .bg(to_rgba(seg.bg))
            .text_color(to_rgba(seg.fg))
            .child(seg.text.clone());
        if clickable {
            cell.cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _window, _cx| {})
        } else {
            cell
        }
    };

    let mut left_row = div().flex().flex_row().items_center();
    for (i, seg) in bar.left.iter().enumerate() {
        if i > 0 {
            let prev_bg = bar.left[i - 1].bg;
            left_row = left_row.child(if powerline {
                div()
                    .text_color(to_rgba(prev_bg))
                    .bg(to_rgba(seg.bg))
                    .child(StatusBar::pl_left_arrow())
            } else {
                div().text_color(sep_fg).bg(to_rgba(seg.bg)).child(" › ")
            });
        }
        left_row = left_row.child(segment_div(seg));
    }

    let mut right_row = div().flex().flex_row().items_center();
    if powerline && !bar.right.is_empty() {
        right_row = right_row.child(
            div()
                .text_color(to_rgba(bar.right[0].bg))
                .bg(bar_bg_color)
                .child(StatusBar::pl_right_arrow()),
        );
    }
    for (i, seg) in bar.right.iter().enumerate() {
        right_row = right_row.child(segment_div(seg));
        if i + 1 < bar.right.len() {
            let next_bg = bar.right[i + 1].bg;
            right_row = right_row.child(if powerline {
                div()
                    .text_color(to_rgba(next_bg))
                    .bg(to_rgba(seg.bg))
                    .child(StatusBar::pl_right_arrow())
            } else {
                div().text_color(sep_fg).bg(bar_bg_color).child(" │ ")
            });
        }
    }

    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .flex_shrink_0()
        .bg(bar_bg_color)
        // Without this, this row's text falls back to gpui's own default UI
        // font -- a different (and differently metriced) typeface from the
        // terminal grid's own cosmic-text-rasterized glyphs sitting right
        // above it, which is exactly what a dogfood report flagged ("the
        // statusbar seems to be a completely different font"). `.font_family`
        // on a div cascades to its text children via gpui's TextStyle stack,
        // the same mechanism `.text_color` above already relies on.
        .font_family(super::font_state::font_family())
        .text_size(gpui::px(super::font_state::font_size()))
        .child(left_row)
        .child(div().flex_1())
        .child(right_row)
}

#[cfg(test)]
mod truncate_path_tests {
    use super::truncate_path;

    #[test]
    fn short_path_is_unchanged() {
        assert_eq!(truncate_path(std::path::Path::new("/tmp/x"), 25), "/tmp/x");
    }

    #[test]
    fn replaces_home_with_tilde() {
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                let p = std::path::PathBuf::from(&home).join("projects/petruterm");
                assert_eq!(truncate_path(&p, 100), "~/projects/petruterm");
            }
        }
    }

    #[test]
    fn truncates_long_paths_at_a_slash_boundary() {
        // len=21, max_chars=10 -> keep last 8 chars, then walk forward to the
        // next '/' (index 13 lands mid-"ccccc"; the next '/' is at index 15).
        let p = std::path::Path::new("/aaa/bbbb/ccccc/ddddd");
        assert_eq!(truncate_path(p, 10), "…/ddddd");
    }
}

#[cfg(test)]
mod format_time_tests {
    use super::format_time;

    #[test]
    fn matches_yyyy_mm_dd_hh_mm_shape() {
        let s = format_time();
        assert_eq!(s.len(), 16, "unexpected length for {s:?}");
        let bytes = s.as_bytes();
        assert_eq!(bytes[4], b'-');
        assert_eq!(bytes[7], b'-');
        assert_eq!(bytes[10], b' ');
        assert_eq!(bytes[13], b':');
        for (i, c) in s.chars().enumerate() {
            if ![4, 7, 10, 13].contains(&i) {
                assert!(c.is_ascii_digit(), "expected digit at {i} in {s:?}");
            }
        }
    }
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
