// The `cx.spawn` poll/wake loop spawned once from `GpuiShellRoot::new`.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use gpui::Context;

use super::config_watch::{CONFIG_CHANGED, PENDING_CONFIG_RELOAD};
use super::{font_state, leader, status_bar, GpuiShellRoot};

/// Tick interval used whenever our window is OS-focused (input
/// responsiveness, PTY-echo repaint latency).
const FOCUSED_TICK: Duration = Duration::from_millis(33);

/// Tick interval used while our window is NOT the OS-focused window (the
/// app is backgrounded -- nothing on screen for the user to see lag on).
/// Cuts the CPU-wakeup rate roughly 12x with no perceptible cost: a
/// keystroke or click that refocuses the window is its own gpui input
/// event, handled by gpui's normal (non-poll-loop) dispatch immediately,
/// not gated on this timer -- only *silent* background changes (PTY output
/// on an unfocused window, wall-clock deadlines) see the coarser cadence,
/// and nobody is watching those while the app isn't focused anyway.
const UNFOCUSED_TICK: Duration = Duration::from_millis(400);

/// Spawn the poll loop: PTY output arrives on a background reader thread
/// gpui doesn't observe, so this ticks every 33ms focused / 400ms
/// unfocused and gates `cx.notify()` on each terminal's `WakeupGate` (plus
/// config reloads, deadlines, AI drains and status bar changes).
pub(super) fn spawn_poll_loop(cx: &mut Context<GpuiShellRoot>) {
    cx.spawn(async move |this, cx| {
        let mut tick = FOCUSED_TICK;
        loop {
            cx.background_executor().timer(tick).await;

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
                            this.palette.rebuild_snippets(&new_config.snippets);
                            this.config = new_config;
                            // TD-GPUI-05: reinject the new theme's colors into
                            // every long-lived TextInput -- `TextInput::new`
                            // only ever snapshots `ColorScheme` at
                            // construction, so without this these four would
                            // keep rendering the outgoing theme after every
                            // other chrome element (which re-reads
                            // `ColorScheme` fresh each frame) had already
                            // switched. Rename editors are excluded
                            // deliberately: they live seconds, not minutes.
                            let colors = this.config.colors.clone();
                            this.chat
                                .composer
                                .update(cx, |input, cx| input.set_colors(&colors, cx));
                            this.ai_block
                                .composer
                                .update(cx, |input, cx| input.set_colors(&colors, cx));
                            this.palette_query
                                .update(cx, |input, cx| input.set_colors(&colors, cx));
                            this.search_query
                                .update(cx, |input, cx| input.set_colors(&colors, cx));
                            // Pick up an edited `llm.*` block
                            // (provider, model, api key, base url) on the
                            // same hot-reload path every other config field
                            // already uses, rather than only at startup and
                            // via `/model`.
                            this.chat.rewire_backend(&this.config, &this.tokio_rt);
                            // The inline AI block has its own
                            // provider instance (own channel/state -- see
                            // `ai_block.rs`'s doc comment), so it needs the
                            // same rewire the chat panel just got above.
                            this.ai_block.rewire_provider(&this.config.llm);
                            this.show_toast(
                                "Config reloaded.",
                                std::time::Duration::from_millis(3000),
                                cx,
                            );
                            cx.notify();
                        })
                        .is_ok();
                    if !applied {
                        break; // window/entity gone
                    }
                }
            }

            let result = this.update(cx, |this: &mut GpuiShellRoot, cx| {
                let mut should_notify = this.wakeup_gates.values().any(|g| g.take_pending());

                // Detect shells that exited on their own (typing
                // `exit`, Ctrl+D, a crash) -- nothing else in this
                // module reads `Pty::rx`, so without this an exited
                // shell's pane just sits there dead until the user
                // notices and closes it by hand. `TitleChanged` and
                // `Bell` are drained with no action (TD-GPUI-04:
                // matches the wgpu build's own handling of both --
                // `Mux::poll_pty_events` only `log::debug!`s a title
                // change and does nothing at all for a bell, so there
                // is no real behavior to port for either). OSC 52
                // clipboard (`ClipboardStore`/`ClipboardLoad`) DOES
                // have real wgpu behavior worth porting -- see their
                // arms below.
                let mut exited_terminals = Vec::new();
                let mut exit_codes = Vec::new();
                for (&id, terminal) in &this.terminals {
                    while let Ok(event) = terminal.pty.rx.try_recv() {
                        match event {
                            crate::term::PtyEvent::Exit(code) => {
                                exit_codes.push((id, code));
                                exited_terminals.push(id);
                            }
                            crate::term::PtyEvent::Osc133(marker) => {
                                let is_prompt_start = matches!(
                                    marker,
                                    crate::term::osc133::Osc133Marker::PromptStart
                                );
                                let command_text = match &marker {
                                    crate::term::osc133::Osc133Marker::CommandStart(cmd) => {
                                        cmd.clone()
                                    }
                                    _ => String::new(),
                                };
                                let absolute_row = terminal.with_term(|t| {
                                    use alacritty_terminal::grid::Dimensions;
                                    let content = t.renderable_content();
                                    let history = t.grid().history_size() as i64;
                                    let cursor_vp = content.cursor.point.line.0.max(0) as i64;
                                    let disp_off = content.display_offset as i64;
                                    history + cursor_vp - disp_off
                                });
                                if let Some(manager) = this.block_managers.get_mut(&id) {
                                    manager.on_marker(marker, absolute_row, command_text);
                                }
                                if is_prompt_start {
                                    let active_ws = this.workspaces.active();
                                    let active_tid = active_ws.tab_panes
                                        [active_ws.tabs.active_index()]
                                    .focused_terminal;
                                    if id == active_tid {
                                        this.snippet_word.clear();
                                    }
                                }
                            }
                            crate::term::PtyEvent::ScreenCleared => {
                                if let Some(manager) = this.block_managers.get_mut(&id) {
                                    manager.clear();
                                }
                            }
                            // TD-GPUI-04: OSC 52 clipboard, ported from
                            // `Mux::poll_pty_events` (`src/app/mux/mod.rs`)
                            // verbatim -- same two-phase shape: read/write
                            // the OS clipboard off-thread (never block the
                            // poll loop on it), and for a load, loop the
                            // result back through the terminal's own PTY
                            // channel as a synthetic `PtyWrite` so the
                            // actual write happens back on this loop, not
                            // on the clipboard thread.
                            crate::term::PtyEvent::ClipboardStore(text) => {
                                std::thread::spawn(move || {
                                    let _ = arboard::Clipboard::new()
                                        .and_then(|mut cb| cb.set_text(text));
                                });
                            }
                            crate::term::PtyEvent::ClipboardLoad(fmt) => {
                                let tx = terminal.pty.tx.clone();
                                std::thread::spawn(move || {
                                    let text = arboard::Clipboard::new()
                                        .ok()
                                        .and_then(|mut cb| cb.get_text().ok())
                                        .unwrap_or_default();
                                    let _ = tx.send(crate::term::PtyEvent::PtyWrite(fmt(&text)));
                                });
                            }
                            crate::term::PtyEvent::PtyWrite(text) => {
                                terminal.write_input(text.as_bytes());
                            }
                            _ => {}
                        }
                    }
                }
                for (id, code) in exit_codes {
                    this.record_terminal_exit_code(id, code);
                }
                for id in exited_terminals {
                    this.on_terminal_exited(id, cx);
                    should_notify = true;
                }
                // Battery-saver gate -- read from `this.battery.cache` as
                // it stands (up to 30s stale, from `poll_battery`'s own
                // TTL below), not re-queried here, since a gating decision
                // doesn't need fresher data than that.
                let battery_saver_active = status_bar::resolve_battery_saver_active(
                    this.config.battery_saver.clone(),
                    this.battery.cache,
                );
                // Blink at the same 530ms cadence the wgpu app uses
                // (Input::update_cursor_blink), piggybacking on this
                // already-running poll loop instead of a new timer --
                // except on battery-saver, where (matching the wgpu
                // app) the cursor stays solid instead of blinking.
                // Forced to `true` rather than just skipped: leaving it
                // untouched could freeze it mid-toggle, i.e. invisible.
                if battery_saver_active {
                    if !this.cursor_blink_on {
                        this.cursor_blink_on = true;
                        should_notify = true;
                    }
                } else if this.cursor_last_blink.elapsed() >= std::time::Duration::from_millis(530)
                {
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
                // Toast auto-dismiss: cleared once its deadline passes,
                // same shape as leader-deadline expiry just above --
                // piggybacks on this same tick rather than a
                // dedicated timer. See `toast.rs`'s own doc comment.
                if this
                    .toast
                    .as_ref()
                    .is_some_and(|(_, deadline)| Instant::now() >= *deadline)
                {
                    this.toast = None;
                    should_notify = true;
                }
                // AI streaming drain: bounded per tick
                // (`ChatPanelView::drain_events`'s own `AI_POLL_CAP`) so
                // a fast stream can't starve everything else sharing
                // this tick -- PTY reads, cursor blink, and the status
                // bar refresh right below it.
                if this.chat.drain_events() {
                    should_notify = true;
                }
                // Same drain, independent channel -- see
                // `ai_block.rs`'s doc comment on why the block owns its
                // own `AiEvent` pair rather than sharing the chat
                // panel's.
                if this.ai_block.drain_events() {
                    should_notify = true;
                }

                // Status bar: CWD, exit code, git branch -- all keyed
                // off the active tab's focused terminal, all
                // refreshed on this same tick rather than every
                // `render()` call. CWD is a cheap syscall
                // (proc_pidinfo), so this just re-checks it every tick
                // rather than chasing every focus-changing call site,
                // and it's also the only way to notice a `cd` typed
                // into the still-focused pane.
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

                    // Same battery-saver backoff as the wgpu app: 15s
                    // TTL / dirty-check on AC, 60s / dirty-check
                    // disabled (skips an extra subprocess) on battery.
                    let git_ttl = if battery_saver_active {
                        std::time::Duration::from_secs(60)
                    } else {
                        std::time::Duration::from_secs(15)
                    };
                    let git_dirty = this.config.status_bar.git_dirty_check && !battery_saver_active;
                    if status_bar::poll_git_branch(
                        &mut this.git_branch,
                        this.cached_cwd.as_deref(),
                        git_dirty,
                        git_ttl,
                        &this.tokio_rt,
                    ) {
                        should_notify = true;
                    }
                }
                // Battery: cheap, local IOKit call (no subprocess), so
                // this piggybacks on the same 33ms tick like the leader-
                // deadline/toast checks above rather than needing its
                // own timer -- `poll_battery`'s own TTL guard keeps the
                // actual IOKit call down to once every 30s.
                if status_bar::poll_battery(
                    &mut this.battery,
                    Instant::now(),
                    std::time::Duration::from_secs(30),
                ) {
                    should_notify = true;
                    // `RUST_LOG=debug` visibility into a state that's
                    // otherwise only observable indirectly (frozen cursor,
                    // slower git refresh) -- logged only on an actual
                    // change, not every tick, via `poll_battery`'s own
                    // "did the cache change" gate above.
                    let now_active = status_bar::resolve_battery_saver_active(
                        this.config.battery_saver.clone(),
                        this.battery.cache,
                    );
                    log::debug!(
                        "battery status changed: {:?} -> battery_saver_active={now_active}",
                        this.battery.cache
                    );
                }
                if this.poll_branch_scan() {
                    should_notify = true;
                }
                if this.chat.poll_file_scan() {
                    should_notify = true;
                }
                if this.chat.poll_acp_connect() {
                    should_notify = true;
                }
                this.handle_acp_terminal_requests(cx);
                if should_notify {
                    cx.notify();
                }
                // Next tick's interval: full speed while our window is
                // OS-focused, much coarser while it isn't (see
                // `UNFOCUSED_TICK`'s doc comment). PetruTerm is
                // single-window (`bin/gpui_petruterm.rs`'s only
                // `cx.open_window` call), so "some window is active" is
                // equivalent to "our window is active".
                if cx.active_window().is_some() {
                    FOCUSED_TICK
                } else {
                    UNFOCUSED_TICK
                }
            });
            let alive = result.is_ok();
            if let Ok(next_tick) = result {
                if next_tick != tick {
                    log::debug!("poll loop tick: {tick:?} -> {next_tick:?}");
                }
                tick = next_tick;
            }
            if !alive {
                break; // window/entity gone
            }
        }
    })
    .detach();
}
