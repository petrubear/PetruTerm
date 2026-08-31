use gpui::{App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions, prelude::*, px, size};
use petruterm::gpui_shell::{Backspace, GpuiShellRoot, SplitDemo};

fn main() {
    Application::new().run(|cx: &mut App| {
        // M0 spike: leader-key chord (`Ctrl+F %`, per AGENTS.md's leader key)
        // spawns a second live terminal, proving gpui's native keymap matcher
        // reaches real business logic. See `GpuiShellRoot::on_split_demo`.
        cx.bind_keys([
            KeyBinding::new("ctrl-f %", SplitDemo, None),
            // Backspace has no `key_char` (it's a control key, not printable
            // text), so `on_key_down`'s minimal M0 handling never sees it.
            // Full key-event mapping is still out of scope for the spike
            // (see `src/app/input/mod.rs`'s real `key_map::translate_key` for
            // what the actual chrome migration will need) — this one binding
            // is a targeted fix for the single most common key that blocks
            // basic dogfooding, not a start on reimplementing that module.
            KeyBinding::new("backspace", Backspace, None),
        ]);

        let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(GpuiShellRoot::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
