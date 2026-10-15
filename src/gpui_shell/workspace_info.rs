// Per-workspace sidebar enrichment (agent attention design, slice 4): git branch, GitHub PR
// status (via the optional `gh` CLI) and listening TCP ports of the processes running in a
// workspace's panes. Gathered on a background thread every `REFRESH` while the sidebar is
// open and the window is focused, and handed back through a static slot that `poll.rs`
// drains -- same bridge shape as `status_bar::git`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::term::attention::AgentState;

const REFRESH: Duration = Duration::from_secs(15);
/// `gh` talks to the network, so PR status refreshes less often than the local probes.
const PR_REFRESH: Duration = Duration::from_secs(60);
/// A probe thread stuck this long (hung `gh`/`lsof`) no longer blocks new refreshes.
const STUCK_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrInfo {
    pub number: u64,
    pub state: PrState,
}

impl PrInfo {
    pub fn label(&self) -> String {
        let state = match self.state {
            PrState::Open => "open",
            PrState::Draft => "draft",
            PrState::Merged => "merged",
            PrState::Closed => "closed",
        };
        format!("PR #{} {state}", self.number)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub branch: Option<String>,
    pub pr: Option<PrInfo>,
    pub ports: Vec<u16>,
}

/// What the sidebar shows under a workspace's name: probed info plus attention state.
#[derive(Debug, Clone)]
pub struct RowDetail {
    pub info: WorkspaceInfo,
    pub state: AgentState,
    pub unread: bool,
    pub message: Option<String>,
}

impl RowDetail {
    /// Short state label for the attention line; `None` when idle.
    pub fn state_label(&self) -> Option<&'static str> {
        match self.state {
            AgentState::Idle => None,
            AgentState::Working => Some("working"),
            AgentState::NeedsInput => Some("needs input"),
            AgentState::Done => Some("done"),
        }
    }
}

/// Input for one workspace's probe, captured on the main thread.
pub struct WorkspaceProbe {
    pub id: usize,
    /// Working directory of the workspace's focused pane.
    pub cwd: Option<PathBuf>,
    /// Shell pids of every pane in the workspace.
    pub pids: Vec<u32>,
    pub want_pr: bool,
}

type ProbeResult = Vec<(usize, WorkspaceInfo, bool)>;
static PENDING: Mutex<Option<ProbeResult>> = Mutex::new(None);

#[derive(Default)]
pub struct WorkspaceInfoState {
    infos: HashMap<usize, WorkspaceInfo>,
    last_spawn: Option<Instant>,
    last_pr: Option<Instant>,
    in_flight: bool,
}

impl WorkspaceInfoState {
    pub fn get(&self, workspace_id: usize) -> Option<&WorkspaceInfo> {
        self.infos.get(&workspace_id)
    }

    /// Take a finished probe's results. Returns true when new data arrived (caller re-renders).
    pub fn drain(&mut self) -> bool {
        let Some(results) = PENDING.lock().unwrap().take() else {
            return false;
        };
        let live: BTreeSet<usize> = results.iter().map(|(id, _, _)| *id).collect();
        self.infos.retain(|id, _| live.contains(id));
        for (id, mut info, pr_fetched) in results {
            if !pr_fetched {
                info.pr = self.infos.get(&id).and_then(|old| old.pr.clone());
            }
            self.infos.insert(id, info);
        }
        self.in_flight = false;
        true
    }

    /// Whether a new probe should start now (sidebar open and window focused, interval
    /// elapsed, none running).
    pub fn refresh_due(&self, active: bool, now: Instant) -> bool {
        should_refresh(self, active, now)
    }

    /// Start a background probe for `probes`.
    pub fn start(&mut self, now: Instant, mut probes: Vec<WorkspaceProbe>) {
        let want_pr = self
            .last_pr
            .is_none_or(|t| now.duration_since(t) >= PR_REFRESH);
        for p in &mut probes {
            p.want_pr = want_pr;
        }
        self.in_flight = true;
        self.last_spawn = Some(now);
        if want_pr {
            self.last_pr = Some(now);
        }
        std::thread::Builder::new()
            .name("workspace-info".into())
            .spawn(move || {
                let results = run_probes(probes);
                *PENDING.lock().unwrap() = Some(results);
            })
            .ok();
    }
}

fn should_refresh(state: &WorkspaceInfoState, active: bool, now: Instant) -> bool {
    if !active {
        return false;
    }
    let due = state
        .last_spawn
        .is_none_or(|t| now.duration_since(t) >= REFRESH);
    let stuck = state
        .last_spawn
        .is_some_and(|t| now.duration_since(t) >= STUCK_AFTER);
    due && (!state.in_flight || stuck)
}

fn run_probes(probes: Vec<WorkspaceProbe>) -> ProbeResult {
    let process_table = command_output("ps", &["-axo", "pid=,ppid="], None);
    let children = process_table.as_deref().map(parse_ps).unwrap_or_default();
    let listeners = command_output("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pn"], None)
        .as_deref()
        .map(parse_lsof_listen)
        .unwrap_or_default();
    probes
        .into_iter()
        .map(|p| {
            let tree: BTreeSet<u32> = descendants(&p.pids, &children).into_iter().collect();
            let ports: BTreeSet<u16> = listeners
                .iter()
                .filter(|(pid, _)| tree.contains(pid))
                .map(|(_, port)| *port)
                .collect();
            let branch = p.cwd.as_deref().and_then(git_branch);
            let pr = if p.want_pr && branch.is_some() {
                p.cwd.as_deref().and_then(fetch_pr)
            } else {
                None
            };
            let info = WorkspaceInfo {
                branch,
                pr,
                ports: ports.into_iter().collect(),
            };
            (p.id, info, p.want_pr)
        })
        .collect()
}

fn command_output(program: &str, args: &[&str], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .env("GH_PROMPT_DISABLED", "1");
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    String::from_utf8(out.stdout).ok()
}

fn git_branch(cwd: &Path) -> Option<String> {
    let out = command_output("git", &["branch", "--show-current"], Some(cwd))?;
    let branch = out.trim();
    (!branch.is_empty()).then(|| branch.to_string())
}

fn fetch_pr(cwd: &Path) -> Option<PrInfo> {
    let out = command_output(
        "gh",
        &["pr", "view", "--json", "number,state,isDraft"],
        Some(cwd),
    )?;
    parse_pr(&out)
}

/// `ps -axo pid=,ppid=` -> parent pid -> child pids.
pub fn parse_ps(text: &str) -> HashMap<u32, Vec<u32>> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for line in text.lines() {
        let mut cols = line.split_whitespace();
        if let (Some(Ok(pid)), Some(Ok(ppid))) = (
            cols.next().map(str::parse::<u32>),
            cols.next().map(str::parse::<u32>),
        ) {
            children.entry(ppid).or_default().push(pid);
        }
    }
    children
}

/// `roots` and every process below them.
pub fn descendants(roots: &[u32], children: &HashMap<u32, Vec<u32>>) -> Vec<u32> {
    let mut seen: BTreeSet<u32> = roots.iter().copied().collect();
    let mut stack: Vec<u32> = roots.to_vec();
    while let Some(pid) = stack.pop() {
        for &child in children.get(&pid).into_iter().flatten() {
            if seen.insert(child) {
                stack.push(child);
            }
        }
    }
    seen.into_iter().collect()
}

/// `lsof -F pn` output (`p<pid>` then `n<addr>:<port>` lines) -> (pid, port) pairs.
pub fn parse_lsof_listen(text: &str) -> Vec<(u32, u16)> {
    let mut pid = None;
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(addr), Some(pid)) = (line.strip_prefix('n'), pid) {
            if let Some(port) = addr.rsplit(':').next().and_then(|p| p.parse().ok()) {
                out.push((pid, port));
            }
        }
    }
    out
}

/// `gh pr view --json number,state,isDraft` output.
pub fn parse_pr(json: &str) -> Option<PrInfo> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let number = v.get("number")?.as_u64()?;
    let state = match v.get("state")?.as_str()? {
        "OPEN" if v.get("isDraft").and_then(|d| d.as_bool()) == Some(true) => PrState::Draft,
        "OPEN" => PrState::Open,
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => return None,
    };
    Some(PrInfo { number, state })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_tree_and_descendants() {
        let children = parse_ps("  1     0\n 10     1\n 11    10\n 12    11\n 20     1\nbogus\n");
        assert_eq!(descendants(&[10], &children), vec![10, 11, 12]);
        assert_eq!(descendants(&[10, 20], &children), vec![10, 11, 12, 20]);
        assert_eq!(descendants(&[99], &children), vec![99]);
    }

    #[test]
    fn lsof_listen_output_is_parsed() {
        let text = "p100\nn*:3000\nn127.0.0.1:8080\np200\nn[::1]:5173\nnnot-a-port\n";
        assert_eq!(
            parse_lsof_listen(text),
            vec![(100, 3000), (100, 8080), (200, 5173)]
        );
    }

    #[test]
    fn pr_json_is_parsed() {
        let pr = |j: &str| parse_pr(j).map(|p| (p.number, p.state));
        assert_eq!(
            pr(r#"{"number":7,"state":"OPEN","isDraft":false}"#),
            Some((7, PrState::Open))
        );
        assert_eq!(
            pr(r#"{"number":7,"state":"OPEN","isDraft":true}"#),
            Some((7, PrState::Draft))
        );
        assert_eq!(
            pr(r#"{"number":7,"state":"MERGED","isDraft":false}"#),
            Some((7, PrState::Merged))
        );
        assert_eq!(pr("no pull requests found"), None);
        assert_eq!(pr(r#"{"number":7,"state":"WEIRD"}"#), None);
    }

    #[test]
    fn probes_find_a_listening_port_of_the_process() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let results = run_probes(vec![WorkspaceProbe {
            id: 5,
            cwd: None,
            pids: vec![std::process::id()],
            want_pr: false,
        }]);
        let (id, info, pr_fetched) = &results[0];
        assert_eq!(*id, 5);
        assert!(!pr_fetched);
        assert!(info.ports.contains(&port), "{:?} lacks {port}", info.ports);
    }

    #[test]
    fn pr_label() {
        let pr = PrInfo {
            number: 12,
            state: PrState::Draft,
        };
        assert_eq!(pr.label(), "PR #12 draft");
    }

    #[test]
    fn refresh_policy() {
        let now = Instant::now();
        let mut s = WorkspaceInfoState::default();
        assert!(should_refresh(&s, true, now));
        assert!(!should_refresh(&s, false, now));
        s.last_spawn = Some(now);
        s.in_flight = true;
        assert!(!should_refresh(&s, true, now + REFRESH));
        assert!(should_refresh(&s, true, now + STUCK_AFTER));
        s.in_flight = false;
        assert!(!should_refresh(&s, true, now + Duration::from_secs(5)));
        assert!(should_refresh(&s, true, now + REFRESH));
    }

    #[test]
    fn state_labels() {
        let detail = |state| RowDetail {
            info: WorkspaceInfo::default(),
            state,
            unread: false,
            message: None,
        };
        assert_eq!(detail(AgentState::Idle).state_label(), None);
        assert_eq!(
            detail(AgentState::NeedsInput).state_label(),
            Some("needs input")
        );
    }
}
