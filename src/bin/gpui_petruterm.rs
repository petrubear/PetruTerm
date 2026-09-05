use gpui::{
    actions, prelude::*, px, size, App, Application, Bounds, KeyBinding, Menu, MenuItem,
    WindowBounds, WindowOptions,
};
use petruterm::gpui_shell::{font_state, spawn_config_watcher, GpuiShellRoot};

// gpui has no default app menu/Cmd+Q binding of its own (confirmed against
// gpui 0.2.2's own `examples/set_menus.rs`, the canonical pattern this
// mirrors) -- without an explicit menu bar carrying a "Quit" item, macOS has
// nothing to route Cmd+Q to, so it silently does nothing. Reported in
// dogfood as "cmd+q is not closing the application".
actions!(gpui_petruterm, [Quit]);

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}

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
        cx.on_action(quit);
        // `set_menus` alone is NOT enough: it renders a clickable Quit item,
        // but the keystroke shown beside it (and the one macOS actually
        // routes) is looked up from the keymap -- `set_menus(menus,
        // &self.keymap.borrow())` in gpui's own `App::set_menus`. With no
        // binding registered, Cmd+Q matches nothing and silently does
        // nothing, which is exactly what the first attempt at this fix
        // shipped. gpui's own `examples/image_gallery.rs` pairs the two calls
        // for this reason.
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![Menu {
            name: "petruterm".into(),
            items: vec![MenuItem::action("Quit", Quit)],
        }]);

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
