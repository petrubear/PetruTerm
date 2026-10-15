//! Per-pane "does this pane need the user" tracking, shared by both binaries (wgpu `Mux` and
//! gpui `GpuiShellRoot`). Pure state, no UI and no window dependencies.
//!
//! State is keyed by terminal id, never by tab index or position: tabs can be reordered
//! (gpui drag-and-drop) and wgpu archives inactive workspaces, so positions are not stable.
//! Tab / workspace indicators are derived by aggregating terminal ids at render time.
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Color of the unread tab dot and the pane ring (warm orange, distinct from accent colors).
pub const ATTENTION_COLOR: [f32; 4] = [1.0, 0.65, 0.2, 1.0];

/// Minimum gap between two accepted attention events from the same pane. Agents and scripts
/// can print notification sequences in a loop, and the text is untrusted.
const MIN_EVENT_GAP: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    /// Reserved for hook-driven state (design slice 2); not constructed yet.
    #[allow(dead_code)]
    Working,
    NeedsInput,
    /// Reserved for hook-driven state (design slice 2); not constructed yet.
    #[allow(dead_code)]
    Done,
}

impl AgentState {
    fn rank(self) -> u8 {
        match self {
            AgentState::Idle => 0,
            AgentState::Working => 1,
            AgentState::Done => 2,
            AgentState::NeedsInput => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttentionEvent {
    /// OSC 9 / 99 / 777 notification from the program running in the pane.
    Notify {
        title: Option<String>,
        body: String,
    },
    Bell,
}

/// A native notification the caller should deliver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub title: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct PaneAttention {
    pub state: AgentState,
    /// Set when an event arrived while the user was not looking at the pane.
    pub unread: bool,
    pub last_message: Option<String>,
    last_event_at: Option<Instant>,
}

impl PaneAttention {
    fn new() -> Self {
        Self {
            state: AgentState::Idle,
            unread: false,
            last_message: None,
            last_event_at: None,
        }
    }
}

/// Highest-priority state and unread flag across a set of panes (a tab or a workspace).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aggregate {
    pub state: AgentState,
    pub unread: bool,
}

#[derive(Default)]
pub struct AttentionTracker {
    panes: HashMap<usize, PaneAttention>,
}

impl AttentionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an event for `terminal_id`.
    ///
    /// `observed` is true when the user is looking at the pane right now (it is the focused
    /// pane of the active tab and the window is active). `bell_attention` is the
    /// `agents.bell_attention` setting. Returns an [`Alert`] when a native notification
    /// should be shown.
    pub fn apply(
        &mut self,
        terminal_id: usize,
        event: AttentionEvent,
        observed: bool,
        bell_attention: bool,
        now: Instant,
    ) -> Option<Alert> {
        let (title, body) = match event {
            AttentionEvent::Notify { title, body } => (title, body),
            AttentionEvent::Bell if bell_attention => (None, "Bell".to_string()),
            AttentionEvent::Bell => return None,
        };
        let pane = self
            .panes
            .entry(terminal_id)
            .or_insert_with(PaneAttention::new);
        if pane
            .last_event_at
            .is_some_and(|t| now.saturating_duration_since(t) < MIN_EVENT_GAP)
        {
            return None;
        }
        pane.last_event_at = Some(now);
        pane.state = AgentState::NeedsInput;
        pane.last_message = Some(body.clone());
        if observed {
            return None;
        }
        pane.unread = true;
        Some(Alert { title, body })
    }

    /// The user is now looking at this pane: clear its indicator.
    pub fn clear(&mut self, terminal_id: usize) {
        if let Some(pane) = self.panes.get_mut(&terminal_id) {
            pane.unread = false;
            if matches!(pane.state, AgentState::NeedsInput | AgentState::Done) {
                pane.state = AgentState::Idle;
            }
        }
    }

    /// Forget a closed pane.
    pub fn remove(&mut self, terminal_id: usize) {
        self.panes.remove(&terminal_id);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn get(&self, terminal_id: usize) -> Option<&PaneAttention> {
        self.panes.get(&terminal_id)
    }

    pub fn is_unread(&self, terminal_id: usize) -> bool {
        self.panes.get(&terminal_id).is_some_and(|p| p.unread)
    }

    /// Aggregate over the given panes. `None` when none of them has any attention state.
    pub fn aggregate(&self, ids: impl IntoIterator<Item = usize>) -> Option<Aggregate> {
        let mut agg: Option<Aggregate> = None;
        for pane in ids.into_iter().filter_map(|id| self.panes.get(&id)) {
            let cur = agg.get_or_insert(Aggregate {
                state: AgentState::Idle,
                unread: false,
            });
            if pane.state.rank() > cur.state.rank() {
                cur.state = pane.state;
            }
            cur.unread |= pane.unread;
        }
        agg
    }

    /// Next unread pane after `current` in `order` (wrapping). `current` itself is considered
    /// last, so repeated presses cycle through every unread pane.
    pub fn next_unread(&self, order: &[usize], current: Option<usize>) -> Option<usize> {
        if order.is_empty() {
            return None;
        }
        let start = current
            .and_then(|c| order.iter().position(|&id| id == c))
            .map_or(0, |i| i + 1);
        (0..order.len())
            .map(|off| order[(start + off) % order.len()])
            .find(|&id| self.is_unread(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notify(body: &str) -> AttentionEvent {
        AttentionEvent::Notify {
            title: None,
            body: body.to_string(),
        }
    }

    #[test]
    fn unobserved_notify_marks_unread_and_alerts() {
        let mut t = AttentionTracker::new();
        let alert = t.apply(1, notify("hi"), false, false, Instant::now());
        assert_eq!(
            alert,
            Some(Alert {
                title: None,
                body: "hi".into()
            })
        );
        let p = t.get(1).unwrap();
        assert!(p.unread);
        assert_eq!(p.state, AgentState::NeedsInput);
        assert_eq!(p.last_message.as_deref(), Some("hi"));
    }

    #[test]
    fn observed_notify_does_not_alert_or_mark_unread() {
        let mut t = AttentionTracker::new();
        assert_eq!(t.apply(1, notify("hi"), true, false, Instant::now()), None);
        assert!(!t.is_unread(1));
    }

    #[test]
    fn bell_ignored_unless_enabled() {
        let mut t = AttentionTracker::new();
        let now = Instant::now();
        assert_eq!(t.apply(1, AttentionEvent::Bell, false, false, now), None);
        assert!(t.get(1).is_none());
        let alert = t.apply(1, AttentionEvent::Bell, false, true, now);
        assert_eq!(alert.map(|a| a.body), Some("Bell".to_string()));
        assert!(t.is_unread(1));
    }

    #[test]
    fn rate_limit_drops_rapid_events() {
        let mut t = AttentionTracker::new();
        let t0 = Instant::now();
        assert!(t.apply(1, notify("a"), false, false, t0).is_some());
        assert!(t
            .apply(
                1,
                notify("b"),
                false,
                false,
                t0 + Duration::from_millis(200)
            )
            .is_none());
        assert_eq!(t.get(1).unwrap().last_message.as_deref(), Some("a"));
        assert!(t
            .apply(
                1,
                notify("c"),
                false,
                false,
                t0 + Duration::from_millis(1100)
            )
            .is_some());
    }

    #[test]
    fn rate_limit_is_per_pane() {
        let mut t = AttentionTracker::new();
        let t0 = Instant::now();
        assert!(t.apply(1, notify("a"), false, false, t0).is_some());
        assert!(t.apply(2, notify("b"), false, false, t0).is_some());
    }

    #[test]
    fn clear_resets_unread_and_state() {
        let mut t = AttentionTracker::new();
        t.apply(1, notify("hi"), false, false, Instant::now());
        t.clear(1);
        let p = t.get(1).unwrap();
        assert!(!p.unread);
        assert_eq!(p.state, AgentState::Idle);
    }

    #[test]
    fn remove_forgets_pane() {
        let mut t = AttentionTracker::new();
        t.apply(1, notify("hi"), false, false, Instant::now());
        t.remove(1);
        assert!(t.get(1).is_none());
    }

    #[test]
    fn aggregate_takes_highest_priority_and_any_unread() {
        let mut t = AttentionTracker::new();
        let now = Instant::now();
        assert_eq!(t.aggregate([1, 2]), None);
        t.apply(1, notify("x"), true, false, now); // NeedsInput, read
        t.apply(2, notify("y"), false, false, now); // NeedsInput, unread
        let agg = t.aggregate([1, 2, 3]).unwrap();
        assert_eq!(agg.state, AgentState::NeedsInput);
        assert!(agg.unread);
        let only_read = t.aggregate([1]).unwrap();
        assert!(!only_read.unread);
    }

    #[test]
    fn next_unread_cycles_and_wraps() {
        let mut t = AttentionTracker::new();
        let now = Instant::now();
        t.apply(2, notify("a"), false, false, now);
        t.apply(4, notify("b"), false, false, now);
        let order = [1, 2, 3, 4];
        assert_eq!(t.next_unread(&order, None), Some(2));
        assert_eq!(t.next_unread(&order, Some(2)), Some(4));
        assert_eq!(t.next_unread(&order, Some(4)), Some(2));
        assert_eq!(t.next_unread(&order, Some(1)), Some(2));
        t.clear(2);
        t.clear(4);
        assert_eq!(t.next_unread(&order, Some(1)), None);
        assert_eq!(t.next_unread(&[], None), None);
    }
}
