//! Detection of CLI coding agents running in panes, for the sidebar "Agents" section.
//!
//! Pure logic, no UI dependencies: parse one `ps` snapshot, decide which panes host a
//! recognized agent (foreground process group of the pane's tty plus the shell's direct
//! children), and order the resulting rows. Identity comes only from the process; hook
//! reports (see `attention`) only supply state. The `args` column of `ps` is used for
//! matching only: it is never stored in a detection or row, logged, or displayed.
//! See `specs/001-agents-sidebar-tab/`.

// Used by the gpui binary only; the wgpu binary compiles this module but never calls it.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::attention::AgentState;

mod activity;
mod names;
mod session;
#[allow(unused_imports)] // the wgpu binary compiles this module but never calls it
pub use activity::*;
#[allow(unused_imports)] // the wgpu binary compiles this module but never calls it
pub use names::*;
pub use session::session_from_args;

/// Arguments for the single `ps` run per scan.
pub const PS_ARGS: [&str; 2] = ["-axo", "pid=,ppid=,pgid=,tpgid=,args="];

/// Interpreters whose first script argument names the real program (`node /x/claude.js`).
const INTERPRETERS: [&str; 3] = ["node", "bun", "deno"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognizedAgent {
    /// Display name (also the override key when merging user entries).
    pub name: String,
    /// Process basenames (or interpreter script names) that identify the agent.
    pub process: Vec<String>,
    /// Color of the agent's name in the sidebar list (RGB); `None` = default text color.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

impl RecognizedAgent {
    fn new(name: &str, process: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            process: process.iter().map(|p| p.to_string()).collect(),
            color: None,
        }
    }

    fn colored(mut self, rgb: [u8; 3]) -> Self {
        self.color = Some(rgb);
        self
    }
}

/// `#rrggbb` (or `rrggbb`) to RGB bytes.
pub fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Built-in list. Names verified 2026-11-06 against the installed commands (all native
/// binaries: claude, kiro-cli, codex, copilot, agy); users can extend it via `agents.recognized`.
pub fn default_agents() -> Vec<RecognizedAgent> {
    vec![
        RecognizedAgent::new("Claude Code", &["claude"]).colored([0xf2, 0x8c, 0x28]), // orange
        RecognizedAgent::new("Kiro", &["kiro-cli", "kiro-cli-chat", "kiro"])
            .colored([0xa8, 0x7b, 0xff]), // purple
        RecognizedAgent::new("Codex", &["codex"]).colored([0x4a, 0xde, 0x80]),        // green
        RecognizedAgent::new("Copilot", &["copilot"]).colored([0xf7, 0x78, 0xba]),    // pink
        RecognizedAgent::new("Antigravity", &["agy", "antigravity"]).colored([0x4f, 0x8c, 0xff]), // blue
    ]
}

/// Defaults plus user entries; a user entry with the same name replaces the default.
pub fn merged_agents(
    defaults: Vec<RecognizedAgent>,
    user: &[RecognizedAgent],
) -> Vec<RecognizedAgent> {
    let mut out = defaults;
    for u in user {
        match out.iter_mut().find(|a| a.name == u.name) {
            Some(slot) => {
                let color = u.color.or(slot.color);
                *slot = u.clone();
                slot.color = color;
            }
            None => out.push(u.clone()),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRow {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    /// Foreground process group of the controlling tty; <= 0 without a tty.
    pub tpgid: i64,
    pub args: String,
}

/// One pane to examine: its terminal id and the pid of its shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneProbe {
    pub terminal_id: usize,
    pub shell_pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDetection {
    pub terminal_id: usize,
    /// `RecognizedAgent::name`.
    pub agent: String,
    pub color: Option<[u8; 3]>,
    /// Pid of the matched agent process; ties names and session ids to one running agent.
    pub pid: u32,
    /// Session id found on the process command line, if any.
    pub session: Option<String>,
}

/// Parse `ps -axo pid=,ppid=,pgid=,tpgid=,args=` output, skipping malformed lines.
pub fn parse_ps_rows(text: &str) -> Vec<ProcessRow> {
    text.lines().filter_map(parse_row).collect()
}

fn parse_row(line: &str) -> Option<ProcessRow> {
    let mut rest = line.trim_start();
    let mut nums = [0i64; 4];
    for slot in &mut nums {
        let end = rest.find(char::is_whitespace)?;
        *slot = rest[..end].parse().ok()?;
        rest = rest[end..].trim_start();
    }
    Some(ProcessRow {
        pid: u32::try_from(nums[0]).ok()?,
        ppid: u32::try_from(nums[1]).ok()?,
        pgid: u32::try_from(nums[2]).ok()?,
        tpgid: nums[3],
        args: rest.to_string(),
    })
}

fn basename(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

fn is_interpreter(name: &str) -> bool {
    INTERPRETERS.contains(&name) || name.starts_with("python")
}

/// Program name to compare against the recognized list: argv[0]'s basename, or for an
/// interpreter the basename (extension stripped) of its first non-flag argument.
fn program_name(args: &str) -> Option<String> {
    let mut tokens = args.split_whitespace();
    let first = basename(tokens.next()?).to_ascii_lowercase();
    if !is_interpreter(&first) {
        return Some(first);
    }
    let script = tokens.find(|t| !t.starts_with('-'))?;
    let script = basename(script);
    let stem = script.rsplit_once('.').map_or(script, |(s, _)| s);
    Some(stem.to_ascii_lowercase())
}

/// The recognized agent a process row belongs to, if any.
pub fn match_agent<'a>(
    row: &ProcessRow,
    agents: &'a [RecognizedAgent],
) -> Option<&'a RecognizedAgent> {
    let name = program_name(&row.args)?;
    agents
        .iter()
        .find(|a| a.process.iter().any(|p| p.eq_ignore_ascii_case(&name)))
}

/// Which panes run a recognized agent. Per pane the candidates are, in order: members of the
/// tty's foreground process group (leader first), then direct children of the shell. At most
/// one detection per pane.
pub fn detect(
    rows: &[ProcessRow],
    probes: &[PaneProbe],
    agents: &[RecognizedAgent],
) -> Vec<AgentDetection> {
    let by_pid: HashMap<u32, &ProcessRow> = rows.iter().map(|r| (r.pid, r)).collect();
    let mut by_pgid: HashMap<u32, Vec<&ProcessRow>> = HashMap::new();
    let mut by_ppid: HashMap<u32, Vec<&ProcessRow>> = HashMap::new();
    for r in rows {
        by_pgid.entry(r.pgid).or_default().push(r);
        by_ppid.entry(r.ppid).or_default().push(r);
    }
    let mut out = Vec::new();
    for probe in probes {
        let Some(shell) = by_pid.get(&probe.shell_pid) else {
            continue;
        };
        let mut candidates: Vec<&ProcessRow> = Vec::new();
        if let Ok(fg) = u32::try_from(shell.tpgid) {
            if fg > 0 {
                let mut group = by_pgid.get(&fg).cloned().unwrap_or_default();
                group.sort_by_key(|r| (r.pid != fg, r.pid));
                candidates.extend(group);
            }
        }
        let mut children = by_ppid.get(&probe.shell_pid).cloned().unwrap_or_default();
        children.sort_by_key(|r| r.pid);
        candidates.extend(children);
        let mut seen = HashSet::new();
        let found = candidates
            .into_iter()
            .filter(|r| seen.insert(r.pid))
            .find_map(|r| match_agent(r, agents).map(|a| (r, a)));
        if let Some((row, agent)) = found {
            out.push(AgentDetection {
                terminal_id: probe.terminal_id,
                agent: agent.name.clone(),
                color: agent.color,
                pid: row.pid,
                session: session_from_args(&row.args),
            });
        }
    }
    out
}

/// One line of the sidebar list. `state` is `None` for an agent that never reported
/// (displayed as "running").
#[derive(Debug, Clone)]
pub struct AgentRow {
    pub terminal_id: usize,
    pub agent: String,
    pub color: Option<[u8; 3]>,
    /// Pid of the agent process (identity for names and session ids).
    pub agent_pid: u32,
    /// The agent accepts messages from other agents.
    pub accepting: bool,
    /// A message was delivered to it in the last few seconds (badge accent).
    pub msg_recent: bool,
    /// User-assigned name, if any.
    pub custom_name: Option<String>,
    /// Full session id when known (shown shortened).
    pub session: Option<String>,
    pub workspace_name: String,
    pub tab_title: String,
    /// 1-based pane position in its tab; `Some` only when the tab has 2+ panes.
    pub pane_number: Option<usize>,
    pub state: Option<AgentState>,
    pub unread: bool,
    pub message: Option<String>,
    /// Current action of a working agent, already filtered by `agents.activity`; `None` when
    /// off, not working, or never reported.
    pub activity: Option<String>,
    pub updated_at: Option<Instant>,
}

fn urgency(state: Option<AgentState>) -> u8 {
    match state {
        Some(AgentState::NeedsInput) => 0,
        Some(AgentState::Working) => 1,
        Some(AgentState::Done) => 2,
        Some(AgentState::Idle) => 3,
        None => 4,
    }
}

/// Order by urgency, then most recent update; otherwise keep the incoming order.
pub fn sort_rows(rows: &mut [AgentRow]) {
    rows.sort_by(|a, b| {
        urgency(a.state)
            .cmp(&urgency(b.state))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

/// Compact age for a row: "12s", "3m", "1h", "2d".
pub fn format_age(age: Duration) -> String {
    let s = age.as_secs();
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

#[cfg(test)]
mod tests;
