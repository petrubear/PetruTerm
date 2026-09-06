// gpui chrome migration (M2 Task 5b): the ~33ms `cx.spawn` poll/wake loop
// spawned once from `GpuiShellRoot::new`. Split out of `mod.rs` for the
// 400-line convention.

use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::Context;

use super::config_watch::{CONFIG_CHANGED, PENDING_CONFIG_RELOAD};
use super::{font_state, leader, status_bar, GpuiShellRoot};

/// Spawn the M1a repaint-reliability poll loop (per the migration spec): PTY
/// output arrives on a background reader thread, decoupled from any gpui
/// entity/state mutation gpui itself would notice — without this, nothing
/// repaints the terminal grid until an unrelated event (e.g. the next
/// keystroke) incidentally triggers one, reproducing the exact
/// `gotcha_lost_pty_echo_wakeup`/Zed-vi-mode class of bug this spike exists
/// to catch. M0 called `cx.notify()` unconditionally on every tick (measured
/// ~21-24% idle CPU); M1a gates that on each terminal's `WakeupGate` so a
/// tick with no PTY activity since the last check is a no-op — still up to
/// 33ms repaint latency, but no wasted relayout/repaint when nothing
/// happened. gpui 0.2.2 has no `spawn_blocking`-style bridge from
/// `BackgroundExecutor` to drive a true cross-thread wake instead (see
/// `spawn_terminal`'s doc comment).
pub(super) fn spawn_poll_loop(cx: &mut Context<GpuiShellRoot>) {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(33))
                .await;

            if CONFIG_CHANGED.swap(false, Ordering::AcqRel) {
                if let Some(new_config) = PENDING_CONFIG_RELOAD.lock().unwrap().take() {
                    let font_config = new_config.font.clone();
                    // Apply the new config AND force a repaint unconditionally
                    // — a config change must show up even if no terminal has
                    // pending PTY output at this exact tick (the gate check
                    // below only fires on PTY activity, not config changes).
                    // reload_font_config needs `&mut App` (to drop the
                    // outgoing font's cached frames from the GPU sprite
                    // atlas, not just clear the Rust-side handles) — only
                    // available inside this closure via `cx`'s `DerefMut<
                    // Target = App>`, so the call lives here rather than
                    // before `this.update`.
                    let applied = this
                        .update(cx, |this: &mut GpuiShellRoot, cx| {
                            font_state::reload_font_config(font_config, cx);
                            this.leader_map = leader::build_leader_map(
                                &crate::config::keybind_view::leader_bindings_view(&new_config)
                                    .bindings,
                            );
                            this.config = new_config;
                            // M3b Task 2: pick up an edited `llm.*` block
                            // (provider, model, api key, base url) on the
                            // same hot-reload path every other config field
                            // already uses, rather than only at startup and
                            // via `/model`.
                            this.chat.rewire_provider(&this.config.llm);
                            // M3b Task 3: the inline AI block has its own
                            // provider instance (own channel/state -- see
                            // `ai_block.rs`'s doc comment), so it needs the
                            // same rewire the chat panel just got above.
                            this.ai_block.rewire_provider(&this.config.llm);
                            cx.notify();
                        })
                        .is_ok();
                    if !applied {
                        break; // window/entity gone
                    }
                }
            }

            let alive = this
                .update(cx, |this: &mut GpuiShellRoot, cx| {
                    let mut should_notify = this.wakeup_gates.values().any(|g| g.take_pending());

                    // Detect shells that exited on their own (typing
                    // `exit`, Ctrl+D, a crash) -- nothing else in this
                    // module reads `Pty::rx`, so without this an exited
                    // shell's pane just sits there dead until the user
                    // notices and closes it by hand. Only `Exit` is
                    // acted on here; other PtyEvent variants (title
                    // changes, bell, OSC 52 clipboard) are drained too
                    // so the channel can't grow unbounded, but are
                    // otherwise a known, pre-existing gap in gpui_shell
                    // (nothing ever consumed them before this loop
                    // existed either) -- not this fix's concern.
                    let mut exited_terminals = Vec::new();
                    for (&id, terminal) in &this.terminals {
                        while let Ok(event) = terminal.pty.rx.try_recv() {
                            if matches!(event, crate::term::PtyEvent::Exit(_)) {
                                exited_terminals.push(id);
                            }
                        }
                    }
                    for id in exited_terminals {
                        this.on_terminal_exited(id, cx);
                        should_notify = true;
                    }
                    // Blink at the same 530ms cadence the wgpu app uses
                    // (Input::update_cursor_blink). Piggybacks on this
                    // already-running 33ms poll loop instead of a new
                    // timer.
                    if this.cursor_last_blink.elapsed() >= std::time::Duration::from_millis(530) {
                        this.cursor_blink_on = !this.cursor_blink_on;
                        this.cursor_last_blink = std::time::Instant::now();
                        should_notify = true;
                    }
                    // Leader-deadline expiry: `on_key_down` only ever
                    // SETS `leader_active`/`leader_deadline` (a key press
                    // always means the deadline hasn't fired yet, by
                    // definition -- this loop would have cleared
                    // `leader_active` first if it had), so expiry has to
                    // be checked from somewhere that runs independently
                    // of keystrokes. Piggybacks on this same 33ms tick
                    // rather than a dedicated timer.
                    if this.leader_active {
                        if let Some(deadline) = this.leader_deadline {
                            if std::time::Instant::now() >= deadline {
                                this.leader_active = false;
                                this.leader_deadline = None;
                                this.leader_prefix = None;
                                should_notify = true; // status bar's leader indicator needs to clear
                            }
                        }
                    }
                    // Status bar: CWD, exit code, git branch -- all keyed
                    // off the active tab's focused terminal, all
                    // refreshed on this same 33ms tick rather than every
                    // `render()` call. CWD is a cheap syscall
                    // (proc_pidinfo), so unlike the wgpu app's own
                    // call-site-instrumented `refresh_status_cache`
                    // (called from every focus-changing path plus PTY
                    // data arrival) this just re-checks it every tick --
                    // simpler than chasing gpui_shell's many
                    // focus-changing call sites (tab switch, pane click,
                    // vim-style pane focus, a closed pane promoting a
                    // sibling...) and it's also the only way to notice a
                    // `cd` typed into the still-focused pane, which has
                    // no dedicated event either.
                    // AI streaming drain (M3b Task 2): bounded per tick
                    // (`ChatPanelView::drain_events`'s own `AI_POLL_CAP`) so
                    // a fast stream can't starve everything else sharing
                    // this tick -- PTY reads, cursor blink, and the status
                    // bar refresh right below it.
                    if this.chat.drain_events() {
                        should_notify = true;
                    }
                    // Same drain, independent channel (M3b Task 3) -- see
                    // `ai_block.rs`'s doc comment on why the block owns its
                    // own `AiEvent` pair rather than sharing the chat
                    // panel's.
                    if this.ai_block.drain_events() {
                        should_notify = true;
                    }

                    let active = this.workspaces.active().tabs.active_index();
                    let active_tid = this.workspaces.active().tab_panes[active].focused_terminal;
                    if let Some(terminal) = this.terminals.get(&active_tid) {
                        let pid = terminal.child_pid;

                        let cwd = crate::term::process_cwd(pid);
                        if cwd != this.cached_cwd {
                            this.cached_cwd = cwd;
                            should_notify = true;
                        }

                        if this.exit_code.poll(pid) {
                            should_notify = true;
                        }

                        let git_dirty = this.config.status_bar.git_dirty_check;
                        if status_bar::poll_git_branch(
                            &mut this.git_branch,
                            this.cached_cwd.as_deref(),
                            git_dirty,
                            std::time::Duration::from_secs(15),
                            &this.tokio_rt,
                        ) {
                            should_notify = true;
                        }
                    }
                    if should_notify {
                        cx.notify();
                    }
                })
                .is_ok();
            if !alive {
                break; // window/entity gone
            }
        }
    })
    .detach();
}
