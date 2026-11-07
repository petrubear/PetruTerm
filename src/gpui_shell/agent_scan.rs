// Background scan behind the sidebar's "Agents" section: one `ps` run on a worker thread,
// results handed back through a static slot that `poll.rs` drains (same bridge shape as
// `workspace_info`). Scans only run while the Agents section is on screen and the window is
// focused, so a closed sidebar costs nothing. See `specs/001-agents-sidebar-tab/`.

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::term::agents::{self, AgentDetection, PaneProbe, RecognizedAgent};

use super::sidebar::SidebarSection;
use super::GpuiShellRoot;

/// Gap between scans while the section is visible (rows appear/disappear within 5 s).
const INTERVAL: Duration = Duration::from_secs(2);
/// A scan stuck this long (hung `ps`) no longer blocks new scans.
const STUCK_AFTER: Duration = Duration::from_secs(10);

static PENDING: Mutex<Option<Vec<AgentDetection>>> = Mutex::new(None);

#[derive(Default)]
pub struct AgentScanState {
    detections: Vec<AgentDetection>,
    last_spawn: Option<Instant>,
    in_flight: bool,
    force: bool,
}

impl AgentScanState {
    pub fn detections(&self) -> &[AgentDetection] {
        &self.detections
    }

    /// Ask for a scan on the next tick (section selected, sidebar opened, stale row hit).
    pub fn mark_due(&mut self) {
        self.force = true;
    }

    /// Take a finished scan's results. Returns `(drained, changed)`: whether a scan result
    /// arrived, and whether it differs from the previous one (caller re-renders).
    fn drain(&mut self) -> (bool, bool) {
        let Some(found) = PENDING.lock().unwrap().take() else {
            return (false, false);
        };
        self.in_flight = false;
        let changed = found != self.detections;
        self.detections = found;
        (true, changed)
    }

    fn due(&self, active: bool, now: Instant) -> bool {
        if !active {
            return false;
        }
        let elapsed = self.last_spawn.map(|t| now.duration_since(t));
        let wanted = self.force || elapsed.is_none_or(|e| e >= INTERVAL);
        let stuck = elapsed.is_some_and(|e| e >= STUCK_AFTER);
        wanted && (!self.in_flight || stuck)
    }

    fn start(&mut self, now: Instant, probes: Vec<PaneProbe>, known: Vec<RecognizedAgent>) {
        self.in_flight = true;
        self.force = false;
        self.last_spawn = Some(now);
        std::thread::Builder::new()
            .name("agent-scan".into())
            .spawn(move || {
                let found = run_ps()
                    .map(|text| agents::detect(&agents::parse_ps_rows(&text), &probes, &known))
                    .unwrap_or_default();
                *PENDING.lock().unwrap() = Some(found);
            })
            .ok();
    }
}

fn run_ps() -> Option<String> {
    let out = Command::new("ps")
        .args(agents::PS_ARGS)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    String::from_utf8(out.stdout).ok()
}

/// Called from the poll loop each tick: drain finished scans and start one when due.
/// Returns true when the visible list changed.
pub(super) fn tick(root: &mut GpuiShellRoot, window_focused: bool) -> bool {
    let (drained, changed) = root.agent_scan.drain();
    let now = Instant::now();
    if drained {
        root.agent_sessions
            .retain_live(root.agent_scan.detections(), now);
        root.agent_names.retain_live(root.agent_scan.detections());
        // The editor closes without saving when its agent is gone or was replaced.
        let gone = root.agent_rename.as_ref().is_some_and(|r| {
            !root
                .agent_scan
                .detections()
                .iter()
                .any(|d| d.terminal_id == r.terminal_id && d.pid == r.pid)
        });
        if gone {
            root.agent_rename = None;
        }
    }
    let active = window_focused
        && root.sidebar.is_visible()
        && root.sidebar.active_section() == SidebarSection::Agents;
    if root.agent_scan.due(active, now) {
        let probes = root.agent_probes();
        let known = agents::merged_agents(agents::default_agents(), &root.config.agents.recognized);
        root.agent_scan.start(now, probes, known);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_only_when_active_and_interval_elapsed() {
        let mut s = AgentScanState::default();
        let t0 = Instant::now();
        assert!(!s.due(false, t0));
        assert!(s.due(true, t0));
        s.in_flight = true;
        s.last_spawn = Some(t0);
        assert!(!s.due(true, t0 + Duration::from_secs(3)));
        s.in_flight = false;
        assert!(!s.due(true, t0 + Duration::from_millis(500)));
        assert!(s.due(true, t0 + Duration::from_secs(2)));
    }

    #[test]
    fn force_bypasses_interval_and_stuck_scan_is_replaced() {
        let mut s = AgentScanState::default();
        let t0 = Instant::now();
        s.last_spawn = Some(t0);
        s.mark_due();
        assert!(s.due(true, t0 + Duration::from_millis(10)));
        s.in_flight = true;
        assert!(!s.due(true, t0 + Duration::from_secs(5)));
        assert!(s.due(true, t0 + Duration::from_secs(11)));
    }
}
