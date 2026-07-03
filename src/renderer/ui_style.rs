//! Centralized UI style tokens (spacing, radii, border width) for the app chrome.
//!
//! Phase 9 R-1: before this, radii/margins/insets were scattered literals across
//! the chat panel, sidebar, palette, tabs and status bar builders. All chrome
//! surfaces now derive their geometry from a single scaled token set so the
//! floating-card look stays consistent.
//!
//! Base constants are in logical pixels. Multiply by `scale_factor` via
//! [`UiStyle::new`] at the call site.

/// Spacing scale (logical px). Used for insets, gaps and padding.
pub const SP_1: f32 = 4.0;
pub const SP_2: f32 = 8.0;
pub const SP_3: f32 = 12.0;
pub const SP_4: f32 = 16.0;

/// Corner radius for outer panel/card surfaces (logical px).
pub const R_PANEL: f32 = 12.0;
/// Corner radius for nested containers (input fields, code blocks) (logical px).
pub const R_INNER: f32 = 8.0;
/// Corner radius for pills / buttons / item rows (logical px).
pub const R_PILL: f32 = 6.0;

/// Border stroke width (logical px).
pub const BORDER: f32 = 1.0;

/// UI style tokens pre-multiplied by the current `scale_factor`.
///
/// Cheap to construct (a handful of multiplications), so it is computed on
/// demand from `RenderContext::ui_style()` rather than cached, which avoids any
/// risk of drift when the DPI scale changes.
#[derive(Debug, Clone, Copy)]
// Fields are consumed incrementally by the chrome builders across Phase 9
// R-3..R-8; the allow is removed once every surface uses the token set.
#[allow(dead_code)]
pub struct UiStyle {
    /// Spacing scale, physical px.
    pub sp1: f32,
    pub sp2: f32,
    pub sp3: f32,
    pub sp4: f32,
    /// Outer panel/card radius, physical px.
    pub r_panel: f32,
    /// Nested container radius, physical px.
    pub r_inner: f32,
    /// Pill/button/item radius, physical px.
    pub r_pill: f32,
    /// Border stroke width, physical px.
    pub border: f32,
}

impl UiStyle {
    /// Build the scaled token set for a given DPI `scale_factor`.
    pub fn new(scale: f32) -> Self {
        Self {
            sp1: SP_1 * scale,
            sp2: SP_2 * scale,
            sp3: SP_3 * scale,
            sp4: SP_4 * scale,
            r_panel: R_PANEL * scale,
            r_inner: R_INNER * scale,
            r_pill: R_PILL * scale,
            border: BORDER * scale,
        }
    }
}
