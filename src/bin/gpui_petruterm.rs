use gpui::{prelude::*, px, size, App, Application, Bounds, WindowBounds, WindowOptions};
use petruterm::gpui_shell::{font_state, spawn_config_watcher, GpuiShellRoot};

fn main() {
    // Unlike src/main.rs (the wgpu binary), nothing here ever initialized a
    // logger -- every `log::info!`/`log::warn!` call anywhere in gpui_shell
    // (config hot-reload, font loading, PUA lookups, the cell-size line
    // added while investigating a font-spacing dogfood report) was silently
    // a no-op with no backend registered to receive it. Same init as
    // src/main.rs so `RUST_LOG=info cargo run --bin gpui-petruterm` actually
    // shows something.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Real user config (~/.config/petruterm/config.lua, falling back to the
    // embedded default) — the same function the wgpu app uses at startup.
    // This replaces M0's hardcoded `Config::default()` + inline font override.
    let (config, _lua) = petruterm::config::load().expect("load config for gpui-petruterm");
    font_state::set_font_config(config.font.clone());

    // Startup-once, like `set_font_config` above — not per-window. See
    // `spawn_config_watcher`'s doc comment for why calling it more than once
    // would race two watcher threads over one process-global slot.
    spawn_config_watcher();

    Application::new().run(move |cx: &mut App| {
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
