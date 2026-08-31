use gpui::{
    prelude::*, px, size, App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions,
};
use petruterm::gpui_shell::{spawn_config_watcher, terminal_element, GpuiShellRoot, SplitDemo};

fn main() {
    // Real user config (~/.config/petruterm/config.lua, falling back to the
    // embedded default) — the same function the wgpu app uses at startup.
    // This replaces M0's hardcoded `Config::default()` + inline font override.
    let (config, _lua) = petruterm::config::load().expect("load config for gpui-petruterm");
    terminal_element::set_font_config(config.font.clone());

    // Startup-once, like `set_font_config` above — not per-window. See
    // `spawn_config_watcher`'s doc comment for why calling it more than once
    // would race two watcher threads over one process-global slot.
    spawn_config_watcher();

    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([KeyBinding::new("ctrl-f %", SplitDemo, None)]);

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
