use gpui::{
    prelude::*, px, size, App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions,
};
use petruterm::gpui_shell::{terminal_element, Backspace, GpuiShellRoot, SplitDemo};

fn main() {
    // Real user config (~/.config/petruterm/config.lua, falling back to the
    // embedded default) — the same function the wgpu app uses at startup.
    // No hot-reload yet (M1a Task 4 adds it); this replaces M0's hardcoded
    // `Config::default()` + inline font override.
    let (config, _lua) = petruterm::config::load().expect("load config for gpui-petruterm");
    terminal_element::set_font_config(config.font.clone());

    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("ctrl-f %", SplitDemo, None),
            KeyBinding::new("backspace", Backspace, None),
        ]);

        let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
        let config = config.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| cx.new(move |cx| GpuiShellRoot::new(cx, config)),
        )
        .unwrap();
        cx.activate(true);
    });
}
