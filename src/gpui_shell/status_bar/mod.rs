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
//
// Split (Task 6a) under the 400-line convention: this file keeps the
// segment/`StatusBar` types + `truncate_path`/`format_time`; `git` holds the
// git-branch async bridge, `exit_code` holds `ExitCodeState`, `battery`
// holds `BatteryState` (added later, once the widget itself was ported --
// see that file's own doc comment), and `render` holds `render_status_bar`.
// Re-exported below so every caller keeps using `status_bar::{...}` paths
// unchanged.

mod battery;
mod exit_code;
mod git;
mod render;

pub use battery::{poll_battery, resolve_battery_saver_active, BatteryState};
pub use exit_code::ExitCodeState;
pub use git::{poll_git_branch, GitBranchState};
pub use render::render_status_bar;

use rust_i18n::t;

use crate::config::schema::{StatusBarColors, StatusBarStyle};

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
            fg: colors.git_fg,
            bg: colors.git_bg,
            kind: SegmentKind::Time,
        });

        bar
    }

    /// Background color for empty space between left and right groups, derived from theme.
    pub fn bar_bg(colors: &StatusBarColors) -> [f32; 4] {
        colors.bar_bg
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
