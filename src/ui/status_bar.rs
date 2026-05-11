use crate::config::schema::{StatusBarColors, StatusBarStyle};
use rust_i18n::t;
use std::time::SystemTime;

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

    /// Width of left separators in character columns for this bar's style.
    pub fn left_sep_width(&self) -> usize {
        match self.style {
            StatusBarStyle::Plain => " › ".chars().count(), // 3
            StatusBarStyle::Powerline => 1,                 // ""
        }
    }

    /// Width of right separators in character columns for this bar's style.
    pub fn right_sep_width(&self) -> usize {
        match self.style {
            StatusBarStyle::Plain => " │ ".chars().count(), // 3
            StatusBarStyle::Powerline => 1,                 // ""
        }
    }

    /// Given a column click position and the total bar width, return which segment kind
    /// was clicked (if any). Mirrors the layout produced by `build_status_bar_instances`.
    pub fn click_kind(&self, col: usize, total_cols: usize) -> Option<SegmentKind> {
        let sep_w = self.left_sep_width();
        // Walk left segments.
        let mut x = 0usize;
        for (i, seg) in self.left.iter().enumerate() {
            if i > 0 {
                x += sep_w;
            }
            let w = seg.text.chars().count();
            if col >= x && col < x + w {
                return Some(seg.kind.clone());
            }
            x += w;
        }
        // Walk right segments (right-aligned).
        let rsep_w = self.right_sep_width();
        // In Powerline mode a leading arrow precedes the first right segment.
        let leading = if self.style == StatusBarStyle::Powerline && !self.right.is_empty() {
            1
        } else {
            0
        };
        let right_total: usize = leading
            + self
                .right
                .iter()
                .map(|s| s.text.chars().count())
                .sum::<usize>()
            + self.right.len().saturating_sub(1) * rsep_w;
        let mut rx = total_cols.saturating_sub(right_total);
        rx += leading; // skip the leading arrow
        for (i, seg) in self.right.iter().enumerate() {
            if i > 0 {
                rx += rsep_w;
            }
            let w = seg.text.chars().count();
            if col >= rx && col < rx + w {
                return Some(seg.kind.clone());
            }
            rx += w;
        }
        None
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

/// Format current local time as "YYYY-MM-DD HH:MM".
fn format_time() -> String {
    use std::time::UNIX_EPOCH;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Simple UTC calculation (no chrono dependency).
    let secs_per_day = 86_400u64;
    let days = secs / secs_per_day;
    let day_secs = secs % secs_per_day;
    let hh = day_secs / 3600;
    let mm = (day_secs % 3600) / 60;

    // Gregorian calendar from day count (days since 1970-01-01).
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02} {hh:02}:{mm:02}")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm: Euclidean, handles leap years.
    let mut year = 1970u64;
    loop {
        let leap = is_leap(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days = [
        31u64,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}
