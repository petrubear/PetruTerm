//! wgpu `App` glue for the agent attention layer: turns queued `PtyEvent::Notify` / `Bell`
//! events into per-pane attention state, delivers notifications, and handles jump-to-unread.
//! State lives in `term::attention::AttentionTracker`; see
//! `docs/superpowers/specs/2026-10-15-agent-attention-design.md`.

use super::App;
use crate::config::schema::NotificationStyle;
use std::time::{Duration, Instant};

impl App {
    /// Drain `mux.attention_events`, update the tracker, deliver notifications, and clear the
    /// indicator of the pane the user is looking at. Called once per loop iteration.
    pub(super) fn process_attention_events(&mut self) {
        crate::remote::bridge::service(&self.mux);
        self.mux.collect_ipc_events();
        let events = std::mem::take(&mut self.mux.attention_events);
        let focused = self.mux.focused_terminal_id();
        let now = Instant::now();
        let mut changed = false;
        for (id, event) in events {
            if let crate::term::attention::AttentionEvent::State { state, message, .. } = &event {
                if let Some(Some(t)) = self.mux.terminals.get(id) {
                    crate::remote::notify_attention(
                        t.pane_uid,
                        state.label(),
                        message.as_deref().unwrap_or(""),
                    );
                }
            }
            let observed = self.window_focused && id == focused;
            let alert =
                self.attention
                    .apply(id, event, observed, self.config.agents.bell_attention, now);
            changed = true;
            if let Some(alert) = alert {
                if self.config.agents.notifications {
                    let title = alert
                        .title
                        .or_else(|| self.mux.tab_title_of(id).map(str::to_owned))
                        .unwrap_or_else(|| "PetruTerm".to_string());
                    self.deliver_attention(id, &title, &alert.body);
                }
            }
        }
        if self.window_focused && self.attention.is_unread(focused) {
            self.attention.clear(focused);
            changed = true;
        }
        if changed {
            self.request_redraw();
        }
    }

    fn deliver_attention(&mut self, terminal_id: usize, title: &str, body: &str) {
        // Native delivery needs an `.app` bundle; fall back to the toast otherwise.
        let native = self.config.notifications.style == NotificationStyle::Native
            && crate::platform::notifications::native_available();
        match native {
            true => {
                crate::platform::notifications::send_with(
                    title,
                    body,
                    &format!("petruterm.pane.{terminal_id}"),
                );
            }
            false => {
                let deadline = Instant::now() + Duration::from_secs(4);
                self.toast = Some((format!("{title}: {body}"), deadline));
            }
        }
    }

    /// Leader+u / Cmd+Shift+U: focus the next pane that needs attention (cycles).
    pub(super) fn jump_to_next_unread(&mut self) {
        let order = self.mux.all_terminal_ids();
        let current = Some(self.mux.focused_terminal_id());
        let Some(id) = self.attention.next_unread(&order, current) else {
            return;
        };
        if self.mux.focus_terminal(id) {
            self.attention.clear(id);
            // Same follow-up the sidebar's workspace switch does.
            self.refresh_status_cache();
            self.resize_terminals_for_panel();
            self.request_redraw();
        }
    }

    /// Per-tab unread flags for the active workspace, aligned with `mux.tabs.tabs()`.
    /// Empty when the ring/dot indicators are disabled.
    pub(super) fn tab_attention_flags(&self) -> Vec<bool> {
        if !self.config.agents.ring {
            return Vec::new();
        }
        (0..self.mux.tabs.tab_count())
            .map(|i| {
                self.attention
                    .aggregate(self.mux.tab_terminal_ids(i))
                    .is_some_and(|a| a.unread)
            })
            .collect()
    }
}
