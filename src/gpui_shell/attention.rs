// gpui glue for the agent attention layer: turns drained `PtyEvent::Notify` / `Bell` events
// into per-pane attention state (`term::attention::AttentionTracker`, keyed by terminal id),
// delivers notifications, and handles jump-to-unread. The wgpu build's counterpart is
// `src/app/attention.rs`; see `docs/superpowers/specs/2026-10-15-agent-attention-design.md`.

use std::time::{Duration, Instant};

use gpui::Context;

use super::GpuiShellRoot;
use crate::config::schema::NotificationStyle;
use crate::term::attention::AttentionEvent;

impl GpuiShellRoot {
    /// Apply this tick's attention events, deliver notifications, and clear the indicator of
    /// the pane the user is looking at. `window_active` is "this window is the active one".
    /// Returns true when anything visible changed (caller re-renders).
    pub(super) fn process_attention_events(
        &mut self,
        events: Vec<(usize, AttentionEvent)>,
        window_active: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let focused = self.focused_terminal_id();
        let now = Instant::now();
        let mut changed = false;
        for (id, event) in events {
            let observed = window_active && id == focused;
            let alert =
                self.attention
                    .apply(id, event, observed, self.config.agents.bell_attention, now);
            changed = true;
            if let Some(alert) = alert {
                if self.config.agents.notifications {
                    let title = alert
                        .title
                        .or_else(|| self.tab_title_of(id))
                        .unwrap_or_else(|| "PetruTerm".to_string());
                    self.deliver_attention(id, &title, &alert.body, cx);
                }
            }
        }
        if window_active && self.attention.is_unread(focused) {
            self.attention.clear(focused);
            changed = true;
        }
        changed
    }

    fn deliver_attention(
        &mut self,
        terminal_id: usize,
        title: &str,
        body: &str,
        cx: &mut Context<Self>,
    ) {
        match self.config.notifications.style {
            NotificationStyle::Native => crate::platform::notifications::send_with(
                title,
                body,
                &format!("petruterm.pane.{terminal_id}"),
            ),
            NotificationStyle::Toast => {
                self.show_toast(format!("{title}: {body}"), Duration::from_secs(4), cx)
            }
        }
    }

    /// Focused terminal of the active tab in the active workspace.
    fn focused_terminal_id(&self) -> usize {
        let ws = self.workspaces.active();
        ws.tab_panes[ws.tabs.active_index()].focused_terminal
    }

    /// Title of the tab (in the active workspace) that holds `terminal_id`.
    fn tab_title_of(&self, terminal_id: usize) -> Option<String> {
        let ws = self.workspaces.active();
        let idx = ws
            .tab_panes
            .iter()
            .position(|f| f.root.leaf_ids().contains(&terminal_id))?;
        ws.tabs.tabs().get(idx).map(|t| t.title.clone())
    }

    /// Terminal ids of every pane in every workspace: the active workspace first (tab order,
    /// then pane order), then the others.
    fn all_terminal_ids(&self) -> Vec<usize> {
        let active = self.workspaces.active_index();
        let all = self.workspaces.workspaces();
        let collect = |i: usize| -> Vec<usize> {
            all[i]
                .tab_panes
                .iter()
                .flat_map(|f| f.root.leaf_ids())
                .collect()
        };
        let mut ids = collect(active);
        for i in (0..all.len()).filter(|&i| i != active) {
            ids.extend(collect(i));
        }
        ids
    }

    /// Bring `terminal_id` into view: switch workspace and tab, focus the pane, and unzoom if a
    /// different pane is zoomed. Returns false if no pane owns that terminal.
    fn focus_terminal(&mut self, terminal_id: usize) -> bool {
        let found = self
            .workspaces
            .workspaces()
            .iter()
            .enumerate()
            .find_map(|(wi, ws)| {
                ws.tab_panes
                    .iter()
                    .position(|f| f.root.leaf_ids().contains(&terminal_id))
                    .map(|ti| (wi, ti))
            });
        let Some((wi, ti)) = found else {
            return false;
        };
        if wi != self.workspaces.active_index() {
            self.switch_workspace_to_index(wi);
        }
        let ws = self.workspaces.active_mut();
        ws.tabs.switch_to_index(ti);
        ws.tab_panes[ti].focused_terminal = terminal_id;
        if ws.zoomed_pane.is_some_and(|z| z != terminal_id) {
            ws.zoomed_pane = None;
        }
        true
    }

    /// Leader+u / Cmd+Shift+U: focus the next pane that needs attention (cycles).
    pub(super) fn jump_to_next_unread(&mut self) {
        let order = self.all_terminal_ids();
        let current = Some(self.focused_terminal_id());
        let Some(id) = self.attention.next_unread(&order, current) else {
            return;
        };
        if self.focus_terminal(id) {
            self.attention.clear(id);
        }
    }

    /// Per-tab unread flags for the active workspace, aligned with its tabs. Empty when the
    /// ring/dot indicators are disabled.
    pub(super) fn tab_attention_flags(&self) -> Vec<bool> {
        if !self.config.agents.ring {
            return Vec::new();
        }
        self.workspaces
            .active()
            .tab_panes
            .iter()
            .map(|f| {
                self.attention
                    .aggregate(f.root.leaf_ids())
                    .is_some_and(|a| a.unread)
            })
            .collect()
    }
}
