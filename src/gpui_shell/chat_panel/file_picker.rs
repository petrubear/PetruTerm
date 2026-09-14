// gpui chrome migration (M5b Task 3): the composer's file attachment
// picker -- async directory scan + fuzzy filter + attach/detach, all
// driven through already-ported, already-pure ChatPanel/picker.rs
// methods (src/llm/chat_panel/picker.rs). Mirrors UiManager::
// open_file_picker_async/poll_file_scan (src/app/ui/mod.rs:650-680)
// exactly, minus the winit-specific bits neither needs.

use std::path::PathBuf;

use super::ChatPanelView;

impl ChatPanelView {
    /// Open the picker in-place (composer keeps focus; see `input.rs`'s
    /// own key-guard doc comment on why this is a mode flag, not a real
    /// widget focus change) and kick off a background scan of `cwd`. The
    /// picker shows immediately with an empty list while the scan runs.
    pub(in crate::gpui_shell) fn open_file_picker_async(&mut self, cwd: PathBuf) {
        self.panel.file_picker_query.clear();
        self.panel.file_picker_cursor = 0;
        self.panel.file_picker_open = true;
        self.panel.file_picker_items.clear();

        let (tx, rx) = crossbeam_channel::bounded(1);
        self.file_scan_rx = Some(rx);
        std::thread::spawn(move || {
            let mut items = crate::llm::chat_panel::scan_files(&cwd, 3);
            items.sort();
            let _ = tx.send(items);
        });
    }

    /// Drain a completed scan into `panel.file_picker_items`. Returns
    /// `true` if it updated anything (caller should `cx.notify()`).
    /// Called from `poll.rs`'s existing 33ms tick as `this.chat.
    /// poll_file_scan()`.
    pub(in crate::gpui_shell) fn poll_file_scan(&mut self) -> bool {
        let Some(rx) = &self.file_scan_rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(items) => {
                self.file_scan_rx = None;
                self.panel.file_picker_items = items;
                true
            }
            Err(_) => false,
        }
    }
}
