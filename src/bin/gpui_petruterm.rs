use gpui::{
    actions, prelude::*, px, size, App, Application, Bounds, KeyBinding, Menu, MenuItem,
    TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use petruterm::config::schema::{TitleBarStyle, WindowBlur};
use petruterm::gpui_shell::{font_state, spawn_config_watcher, GpuiShellRoot};

// gpui has no default app menu/Cmd+Q binding: Cmd+Q needs both the menu item
// and a keymap binding (see `main`).
actions!(gpui_petruterm, [Quit]);

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}

fn main() {
    // Init env_logger (default info), same as src/main.rs.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Real user config (~/.config/petruterm/config.lua, falling back to the
    // embedded default) — the same function the wgpu app uses at startup.
    let (config, _lua) = petruterm::config::load().expect("load config for gpui-petruterm");
    font_state::set_font_config(config.font.clone());

    // Startup-once, like `set_font_config` above — not per-window. See
    // `spawn_config_watcher`'s doc comment for why calling it more than once
    // would race two watcher threads over one process-global slot.
    spawn_config_watcher();

    Application::new().run(move |cx: &mut App| {
        cx.on_action(quit);
        // Cmd+Q needs both the menu item and a keymap binding.
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        petruterm::gpui_shell::text_input::register_key_bindings(cx);
        cx.set_menus(vec![Menu {
            name: "petruterm".into(),
            items: vec![MenuItem::action("Quit", Quit)],
        }]);

        let (width, height) = match (config.window.initial_width, config.window.initial_height) {
            (Some(w), Some(h)) => (w as f32, h as f32),
            _ => (900.0, 600.0),
        };
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        let window_bounds = if config.window.start_maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        };
        let window_background = if config.window.blur != WindowBlur::None {
            WindowBackgroundAppearance::Blurred
        } else if config.window.opacity < 1.0 {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        };
        let mut options = WindowOptions {
            window_bounds: Some(window_bounds),
            window_background,
            ..Default::default()
        };
        // `Native` keeps the standard opaque system titlebar (this chrome
        // draws no titlebar of its own). `None` drops the title and makes the
        // titlebar transparent/full-size; the traffic lights stay, and
        // `GpuiShellRoot` insets the chrome to clear them. `Custom` keeps the
        // titlebar (native drag region + traffic lights, unlike `None`) but
        // makes it transparent too, so the traffic lights float on the app's
        // own themed background instead of a separate opaque bar -- matching
        // what the wgpu binary's own Custom mode already does via raw AppKit
        // calls (`App::apply_macos_custom_titlebar`), but through gpui's own
        // first-class `appears_transparent` option instead. Same
        // `GpuiShellRoot` inset as `None` (below) applies here too, since a
        // transparent titlebar also extends the content view full-size.
        match config.window.title_bar_style {
            TitleBarStyle::None => options.titlebar = None,
            TitleBarStyle::Custom => {
                options.titlebar = Some(TitlebarOptions {
                    appears_transparent: true,
                    ..Default::default()
                })
            }
            TitleBarStyle::Native => {}
        }
        let config = config.clone();
        cx.open_window(options, move |_, cx| {
            cx.new(move |cx| GpuiShellRoot::new(cx, config))
        })
        .unwrap();
        cx.activate(true);
    });
}
