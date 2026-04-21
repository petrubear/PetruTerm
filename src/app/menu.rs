use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use rust_i18n::t;

use crate::ui::palette::Action;
use crate::ui::panes::FocusDir;

/// Builds and owns the native menu bar. Maps MenuItemId → Action.
pub struct AppMenu {
    pub menu_bar: Menu,
    /// The "Window" submenu — registered as the macOS windows menu.
    pub window_submenu: Submenu,
    items: Vec<(MenuId, Action)>,
}

impl AppMenu {
    pub fn build() -> Self {
        let menu_bar = Menu::new();
        let mut items: Vec<(MenuId, Action)> = Vec::new();

        // ── PetruTerm app menu (macOS) ────────────────────────────────────────
        #[cfg(target_os = "macos")]
        {
            let app_menu = Submenu::new("PetruTerm", true);
            app_menu
                .append_items(&[
                    &PredefinedMenuItem::about(
                        None,
                        Some(muda::AboutMetadata {
                            name: Some("PetruTerm".to_string()),
                            version: Some(env!("CARGO_PKG_VERSION").to_string()),
                            copyright: Some("Copyright © 2026 petrubear".to_string()),
                            ..Default::default()
                        }),
                    ),
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::services(None),
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::hide(None),
                    &PredefinedMenuItem::hide_others(None),
                    &PredefinedMenuItem::show_all(None),
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::quit(None),
                ])
                .ok();
            menu_bar.append(&app_menu).ok();
        }

        // ── File: settings only ───────────────────────────────────────────────
        let file_menu = Submenu::new(t!("menu.file").as_ref(), true);
        let settings = MenuItem::new(t!("menu.settings").as_ref(), true, None);
        let reload_config = MenuItem::new(t!("menu.reload_config").as_ref(), true, None);
        file_menu
            .append_items(&[
                &settings,
                &reload_config,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ])
            .ok();
        items.push((settings.id().clone(), Action::OpenConfigFolder));
        items.push((reload_config.id().clone(), Action::ReloadConfig));

        // ── View ──────────────────────────────────────────────────────────────
        let view_menu = Submenu::new(t!("menu.view").as_ref(), true);
        let toggle_status = MenuItem::new(t!("menu.toggle_status_bar").as_ref(), true, None);
        let switch_theme = MenuItem::new(t!("menu.switch_theme").as_ref(), true, None);
        let toggle_fullscreen = MenuItem::new(t!("menu.toggle_fullscreen").as_ref(), true, None);
        view_menu
            .append_items(&[&toggle_status, &switch_theme, &toggle_fullscreen])
            .ok();
        items.push((toggle_status.id().clone(), Action::ToggleStatusBar));
        items.push((switch_theme.id().clone(), Action::OpenThemePicker));
        items.push((toggle_fullscreen.id().clone(), Action::ToggleFullscreen));

        // ── AI ────────────────────────────────────────────────────────────────
        let ai_menu = Submenu::new(t!("menu.ai").as_ref(), true);
        let toggle_ai = MenuItem::new(t!("menu.toggle_ai_panel").as_ref(), true, None);
        let explain = MenuItem::new(t!("menu.explain_output").as_ref(), true, None);
        let fix_error = MenuItem::new(t!("menu.fix_error").as_ref(), true, None);
        let undo_write = MenuItem::new(t!("menu.undo_write").as_ref(), true, None);
        let enable_ai = MenuItem::new(t!("menu.enable_ai").as_ref(), true, None);
        let disable_ai = MenuItem::new(t!("menu.disable_ai").as_ref(), true, None);
        ai_menu
            .append_items(&[
                &toggle_ai,
                &explain,
                &fix_error,
                &undo_write,
                &PredefinedMenuItem::separator(),
                &enable_ai,
                &disable_ai,
            ])
            .ok();
        items.push((toggle_ai.id().clone(), Action::ToggleAiPanel));
        items.push((explain.id().clone(), Action::ExplainLastOutput));
        items.push((fix_error.id().clone(), Action::FixLastError));
        items.push((undo_write.id().clone(), Action::UndoLastWrite));
        items.push((enable_ai.id().clone(), Action::EnableAiFeatures));
        items.push((disable_ai.id().clone(), Action::DisableAiFeatures));

        // ── Window: tab + pane management + macOS predefined ─────────────────
        let window_submenu = Submenu::new(t!("menu.window").as_ref(), true);

        // Tabs submenu
        let tabs_submenu = Submenu::new(t!("menu.tab").as_ref(), true);
        let new_tab = MenuItem::new(t!("menu.new_tab").as_ref(), true, None);
        let close_tab = MenuItem::new(t!("menu.close_tab").as_ref(), true, None);
        let rename_tab = MenuItem::new(t!("menu.rename_tab").as_ref(), true, None);
        let next_tab = MenuItem::new(t!("menu.next_tab").as_ref(), true, None);
        let prev_tab = MenuItem::new(t!("menu.prev_tab").as_ref(), true, None);
        tabs_submenu
            .append_items(&[
                &new_tab,
                &close_tab,
                &rename_tab,
                &PredefinedMenuItem::separator(),
                &next_tab,
                &prev_tab,
            ])
            .ok();
        items.push((new_tab.id().clone(), Action::NewTab));
        items.push((close_tab.id().clone(), Action::CloseTab));
        items.push((rename_tab.id().clone(), Action::RenameTab));
        items.push((next_tab.id().clone(), Action::NextTab));
        items.push((prev_tab.id().clone(), Action::PrevTab));

        // Panes submenu
        let panes_submenu = Submenu::new(t!("menu.pane").as_ref(), true);
        let split_h = MenuItem::new(t!("menu.split_horizontal").as_ref(), true, None);
        let split_v = MenuItem::new(t!("menu.split_vertical").as_ref(), true, None);
        let close_pane = MenuItem::new(t!("menu.close_pane").as_ref(), true, None);
        let focus_left = MenuItem::new(t!("menu.focus_left").as_ref(), true, None);
        let focus_right = MenuItem::new(t!("menu.focus_right").as_ref(), true, None);
        let focus_up = MenuItem::new(t!("menu.focus_up").as_ref(), true, None);
        let focus_down = MenuItem::new(t!("menu.focus_down").as_ref(), true, None);
        panes_submenu
            .append_items(&[
                &split_h,
                &split_v,
                &close_pane,
                &PredefinedMenuItem::separator(),
                &focus_left,
                &focus_right,
                &focus_up,
                &focus_down,
            ])
            .ok();
        items.push((split_h.id().clone(), Action::SplitHorizontal));
        items.push((split_v.id().clone(), Action::SplitVertical));
        items.push((close_pane.id().clone(), Action::ClosePane));
        items.push((focus_left.id().clone(), Action::FocusPane(FocusDir::Left)));
        items.push((focus_right.id().clone(), Action::FocusPane(FocusDir::Right)));
        items.push((focus_up.id().clone(), Action::FocusPane(FocusDir::Up)));
        items.push((focus_down.id().clone(), Action::FocusPane(FocusDir::Down)));

        window_submenu
            .append_items(&[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::fullscreen(None),
                &PredefinedMenuItem::separator(),
                &tabs_submenu,
                &panes_submenu,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::bring_all_to_front(None),
            ])
            .ok();

        menu_bar
            .append_items(&[&file_menu, &view_menu, &ai_menu, &window_submenu])
            .ok();

        Self {
            menu_bar,
            window_submenu,
            items,
        }
    }

    pub fn action_for(&self, event: &MenuEvent) -> Option<Action> {
        self.items
            .iter()
            .find(|(id, _)| *id == event.id)
            .map(|(_, action)| action.clone())
    }
}
