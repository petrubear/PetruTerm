use std::time::SystemTime;

/// A single colored segment in the status bar.
#[derive(Debug, Clone)]
pub struct StatusBarSegment {
    pub text: String,
    pub fg: [f32; 4],
    pub bg: [f32; 4],
}

/// Assembled status bar with left and right segment groups.
#[derive(Debug, Clone, Default)]
pub struct StatusBar {
    /// Segments shown on the left, separated by › arrows.
    pub left: Vec<StatusBarSegment>,
    /// Segments shown on the right, separated by │.
    pub right: Vec<StatusBarSegment>,
}

// ── Dracula Pro palette constants ────────────────────────────────────────────
const BG_BAR:      [f32; 4] = [0.16, 0.16, 0.22, 1.0]; // current-line #282a36
const FG_DEFAULT:  [f32; 4] = [0.97, 0.97, 0.95, 1.0]; // foreground   #f8f8f2
const FG_DIM:      [f32; 4] = [0.55, 0.56, 0.67, 1.0]; // comment      #6272a4

const BG_LEADER_ACTIVE:   [f32; 4] = [0.58, 0.50, 1.00, 1.0]; // purple  #9580ff
const BG_LEADER_INACTIVE: [f32; 4] = [0.22, 0.22, 0.30, 1.0]; // subdued
const BG_CWD:      [f32; 4] = [0.20, 0.20, 0.27, 1.0]; // slightly lighter
const BG_GIT:      [f32; 4] = [0.16, 0.28, 0.22, 1.0]; // green-tinted
const BG_ERROR:    [f32; 4] = [0.60, 0.12, 0.12, 1.0]; // red
const BG_TIME:     [f32; 4] = [0.13, 0.20, 0.30, 1.0]; // blue-tinted

impl StatusBar {
    /// Build the status bar from current application state.
    ///
    /// - `leader_active`: true when the leader key has been pressed and the
    ///   timeout is still running (shows the LEADER segment in purple).
    /// - `cwd`: current working directory (None if unavailable).
    /// - `git_branch`: cached git branch string (None if not a git repo or not yet fetched).
    /// - `last_exit_code`: last exit code from shell context (None if unavailable).
    pub fn build(
        leader_active: bool,
        leader_key: &str,
        cwd: Option<&std::path::Path>,
        git_branch: Option<&str>,
        last_exit_code: Option<i32>,
    ) -> Self {
        let mut bar = StatusBar::default();

        // ── Left segments ────────────────────────────────────────────────────

        // Leader-mode indicator.
        let leader_bg = if leader_active { BG_LEADER_ACTIVE } else { BG_LEADER_INACTIVE };
        let leader_label = format!(" ^{} ", leader_key.to_uppercase());
        let leader_text = if leader_active { " LEADER " } else { leader_label.as_str() };
        bar.left.push(StatusBarSegment { text: leader_text.into(), fg: FG_DEFAULT, bg: leader_bg });

        // Current working directory (truncated).
        if let Some(path) = cwd {
            let display = truncate_path(path, 25);
            bar.left.push(StatusBarSegment { text: format!("  {display} "), fg: FG_DEFAULT, bg: BG_CWD });
        }

        // Git branch.
        if let Some(branch) = git_branch {
            if !branch.is_empty() {
                bar.left.push(StatusBarSegment {
                    text: format!("  {branch} "),
                    fg: FG_DEFAULT,
                    bg: BG_GIT,
                });
            }
        }

        // ── Right segments ───────────────────────────────────────────────────

        // Exit code (only shown when non-zero).
        if let Some(code) = last_exit_code {
            if code != 0 {
                bar.right.push(StatusBarSegment {
                    text: format!(" ✘ {code} "),
                    fg: FG_DEFAULT,
                    bg: BG_ERROR,
                });
            }
        }

        // Date + time.
        let time_str = format_time();
        bar.right.push(StatusBarSegment { text: format!(" {time_str} "), fg: FG_DIM, bg: BG_TIME });

        bar
    }

    /// Background color for empty space between left and right groups.
    pub fn bar_bg() -> [f32; 4] { BG_BAR }

    /// Separator used between left segments.
    pub fn left_sep() -> &'static str { " › " }

    /// Separator used between right segments.
    pub fn right_sep() -> &'static str { " │ " }
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
    let start = chars[start..].iter().position(|&c| c == '/').map(|i| start + i).unwrap_or(start);
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
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md { break; }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
