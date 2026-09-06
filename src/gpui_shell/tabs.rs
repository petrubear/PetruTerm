// gpui chrome migration (M2): tab list + the tab bar that renders it. The
// data half (Tab/TabManager/tab_display_label) is ported from src/ui/tabs.rs
// verbatim -- pure data + one pure string-formatting function, zero I/O.
// `render_tab_bar` (M2 Task 3) is the "tab-bar Render impl" the port's
// original header anticipated; it's here rather than in mod.rs so the labels
// it paints and any future hit-testing both sit next to `tab_display_label`,
// their single source of truth (they diverged once in the wgpu app, which is
// what TD-P9-02 was).

#![allow(dead_code)]

use std::rc::Rc;

use gpui::{div, prelude::*, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::pane_view::to_rgba;

/// Max glyph width of a tab pill label.
pub const TAB_LABEL_MAX_CHARS: usize = 18;

/// The visible label for a tab pill: `" title: N "` (1-based), or the rename
/// buffer with a cursor when that tab is being renamed. Truncated to
/// [`TAB_LABEL_MAX_CHARS`].
///
/// Shared by the tab-bar renderer (`build_tab_bar_instances`) and the click
/// hit-test (`hit_test_tab_bar`) so both agree on each pill's column width —
/// they diverged before, which made tab clicks land on the wrong tab (TD-P9-02).
pub fn tab_display_label(
    title: &str,
    index: usize,
    is_active: bool,
    rename_input: Option<&str>,
) -> String {
    let raw = match (is_active, rename_input) {
        (true, Some(input)) => format!(" {input}\u{258c} "),
        _ => format!(" {title}: {} ", index + 1),
    };
    raw.chars().take(TAB_LABEL_MAX_CHARS).collect()
}

/// A single terminal tab.
#[derive(Debug)]
pub struct Tab {
    pub id: usize,
    pub title: String,
    /// Index into the pane tree (one pane tree per tab).
    pub pane_tree_id: usize,
    /// Optional accent color override. None → use theme ui_accent.
    pub accent_color: Option<[f32; 4]>,
}

/// Manages the ordered list of tabs.
pub struct TabManager {
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
        }
    }

    /// Create a new tab, returning its ID.
    pub fn new_tab(&mut self, title: impl Into<String>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let pane_tree_id = id; // 1:1 mapping for now
        self.tabs.push(Tab {
            id,
            title: title.into(),
            pane_tree_id,
            accent_color: None,
        });
        self.active = self.tabs.len() - 1;
        id
    }

    /// Close the tab with the given ID. Returns true if a tab was removed.
    ///
    /// `close_tab` can be asked to remove any tab, not just the active one
    /// (`gpui_shell`'s `close_tab_at` reaches this for a background tab
    /// whose last pane exited on its own while a different tab was in
    /// view). Removing an element before `active` shifts every later
    /// element's index down by one, so `active` must shift down with it to
    /// keep pointing at the same logical tab -- clamping alone (the
    /// previous implementation) only protects against `active` overflowing
    /// the new length; it does nothing when `pos < active` and `active`
    /// isn't already at the last index, which silently switches the
    /// visible tab to whatever now occupies the old `active` position.
    pub fn close_tab(&mut self, id: usize) -> bool {
        let Some(pos) = self.tabs.iter().position(|t| t.id == id) else {
            return false;
        };
        self.tabs.remove(pos);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if pos < self.active {
            self.active -= 1;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
        true
    }

    /// Switch to the tab at the given index (0-based). Returns true if successful.
    pub fn switch_to_index(&mut self, idx: usize) -> bool {
        if idx < self.tabs.len() {
            self.active = idx;
            true
        } else {
            false
        }
    }

    /// Switch to the next tab (wraps around).
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// Switch to the previous tab (wraps around).
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// Set the accent color for the tab at `idx`. Pass None to reset to theme default.
    pub fn set_tab_color(&mut self, idx: usize, color: Option<[f32; 4]>) {
        if let Some(tab) = self.tabs.get_mut(idx) {
            tab.accent_color = color;
        }
    }

    /// Returns the active tab's accent color, falling back to `default`.
    pub fn active_accent(&self, default: [f32; 4]) -> [f32; 4] {
        self.tabs
            .get(self.active)
            .and_then(|t| t.accent_color)
            .unwrap_or(default)
    }

    /// Rename the active tab.
    pub fn rename_active(&mut self, title: impl Into<String>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.title = title.into();
        }
    }

    /// Rename the tab with the given id, wherever it currently sits and
    /// regardless of which tab is active. Returns whether a tab with that id
    /// was found. Used by `gpui_shell`'s rename flow, which pins its edit to
    /// a tab id rather than "the active tab" precisely so a tab switch mid-
    /// rename can't redirect the commit to the wrong tab.
    pub fn rename_tab(&mut self, id: usize, title: impl Into<String>) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        tab.title = title.into();
        true
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Called with the clicked tab's index. A callback rather than a direct
/// `TabManager` mutation because the click also has to reach `GpuiShellRoot`
/// (switching tabs changes which pane tree renders, so the view has to be
/// notified) -- built from `Context::listener` at the call site.
pub type TabSelectCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// The tab bar row: one flat, clickable cell per tab. The active tab gets a
/// filled background plus an accent underline and full-strength text; the
/// rest are dimmed -- matching the wgpu app's own flat-rect-plus-underline
/// treatment. Inactive tabs carry the same 2px bottom border in the bar's own
/// color so switching tabs never shifts the row's height.
pub fn render_tab_bar(
    tabs: &TabManager,
    colors: &ColorScheme,
    on_select: TabSelectCallback,
    rename: Option<(usize, gpui::AnyElement)>,
) -> Div {
    let active_index = tabs.active_index();
    let accent = to_rgba(tabs.active_accent(colors.ui_accent));
    let surface = to_rgba(colors.ui_surface);
    // `rename`'s element can't be cloned into every loop iteration
    // (`AnyElement` isn't `Clone`), and `.children()`'s closure must be
    // `FnMut` -- so it's built out here and `take()`n exactly once, on the
    // cell whose tab id matches (NOT on `is_active`: the rename is pinned to
    // a tab id precisely so it keeps rendering on the right cell even after
    // a tab switch moves `is_active` elsewhere).
    let mut rename = rename;
    let cells: Vec<_> = tabs
        .tabs()
        .iter()
        .enumerate()
        .map(|(idx, tab)| {
            let is_active = idx == active_index;
            let is_renaming = rename.as_ref().is_some_and(|(id, _)| *id == tab.id);
            let on_select = on_select.clone();
            let cell = div()
                .px_2()
                .py_1()
                .border_b_2()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    on_select(&idx, window, cx)
                });
            let cell = if is_renaming {
                cell.child(rename.take().expect("checked is_some").1)
            } else {
                cell.child(tab_display_label(&tab.title, idx, is_active, None))
            };
            if is_active {
                cell.bg(to_rgba(colors.ui_surface_active))
                    .border_color(accent)
                    .text_color(to_rgba(colors.foreground))
            } else {
                cell.border_color(surface)
                    .text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();
    div()
        .flex()
        .flex_row()
        .w_full()
        .flex_shrink_0()
        .bg(surface)
        // Same fix as `status_bar::render_status_bar`: without an explicit
        // font, tab labels render in gpui's own default UI font instead of
        // the terminal grid's configured monospace face.
        .font_family(super::font_state::font_family())
        .text_size(gpui::px(super::font_state::font_size()))
        .children(cells)
}

#[cfg(test)]
mod tab_label_tests {
    use super::{tab_display_label, TAB_LABEL_MAX_CHARS};

    #[test]
    fn label_format_and_truncation() {
        // Normal label: " title: N " (1-based index).
        assert_eq!(tab_display_label("zsh", 0, false, None), " zsh: 1 ");
        assert_eq!(tab_display_label("zsh", 1, true, None), " zsh: 2 ");
        // Rename buffer on the active tab shows the input + cursor.
        assert_eq!(
            tab_display_label("zsh", 0, true, Some("newname")),
            " newname\u{258c} "
        );
        // Rename is ignored on inactive tabs.
        assert_eq!(tab_display_label("zsh", 0, false, Some("x")), " zsh: 1 ");
        // Truncated to the pill max width.
        let long = tab_display_label("a-very-long-tab-title-indeed", 0, false, None);
        assert_eq!(long.chars().count(), TAB_LABEL_MAX_CHARS);
    }
}

#[cfg(test)]
mod tab_manager_tests {
    use super::TabManager;

    #[test]
    fn closing_a_background_tab_before_active_keeps_the_same_tab_active() {
        let mut mgr = TabManager::new();
        let a = mgr.new_tab("a");
        let _b = mgr.new_tab("b");
        let _c = mgr.new_tab("c");
        let _d = mgr.new_tab("d");
        // 4 tabs: a b c d, active on d after each new_tab() (its own
        // behavior). Move active to b (index 1) before closing a (index 0),
        // reproducing the exact shape that silently mis-clamped before this
        // fix: `pos (0) < active (1)`, and active was nowhere near the last
        // index, so the old `.min(len-1)` clamp was a no-op and left
        // `active` unchanged while every tab past `pos` shifted down.
        mgr.switch_to_index(1);
        assert_eq!(mgr.tabs()[mgr.active_index()].title, "b");

        assert!(mgr.close_tab(a));

        // b (was index 1) is now at index 0; active must have followed it
        // down rather than staying at 1 (which would now be c).
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.tabs()[mgr.active_index()].title, "b");
        assert_eq!(
            mgr.tabs()
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "d"]
        );
    }

    #[test]
    fn closing_a_background_tab_after_active_leaves_active_index_unchanged() {
        let mut mgr = TabManager::new();
        let _a = mgr.new_tab("a");
        let _b = mgr.new_tab("b");
        let c = mgr.new_tab("c");
        mgr.switch_to_index(0);
        assert_eq!(mgr.tabs()[mgr.active_index()].title, "a");

        assert!(mgr.close_tab(c));

        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.tabs()[mgr.active_index()].title, "a");
    }

    #[test]
    fn closing_the_active_tab_still_clamps_to_the_new_last_index() {
        let mut mgr = TabManager::new();
        let _a = mgr.new_tab("a");
        let b = mgr.new_tab("b");
        // new_tab() leaves `b` (the last one created) active.
        assert_eq!(mgr.active_index(), 1);

        assert!(mgr.close_tab(b));

        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.tabs()[mgr.active_index()].title, "a");
    }

    #[test]
    fn rename_tab_by_id_renames_a_non_active_tab_and_leaves_active_alone() {
        let mut mgr = TabManager::new();
        let a = mgr.new_tab("a");
        let _b = mgr.new_tab("b");
        // new_tab() leaves "b" active; renaming by "a"'s id must not touch
        // whichever tab is active -- this is the exact regression a
        // rename-by-"active tab" implementation would get wrong if the
        // active tab changed after the rename editor was opened for `a`.
        assert_eq!(mgr.tabs()[mgr.active_index()].title, "b");

        assert!(mgr.rename_tab(a, "notes"));

        assert_eq!(mgr.tabs()[0].title, "notes");
        assert_eq!(mgr.tabs()[mgr.active_index()].title, "b");
    }

    #[test]
    fn rename_tab_with_unknown_id_returns_false_and_mutates_nothing() {
        let mut mgr = TabManager::new();
        let _a = mgr.new_tab("a");
        let _b = mgr.new_tab("b");
        let titles_before: Vec<_> = mgr.tabs().iter().map(|t| t.title.clone()).collect();

        assert!(!mgr.rename_tab(999, "notes"));

        let titles_after: Vec<_> = mgr.tabs().iter().map(|t| t.title.clone()).collect();
        assert_eq!(titles_before, titles_after);
    }
}
