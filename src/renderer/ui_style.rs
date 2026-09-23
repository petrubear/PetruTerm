//! Centralized UI style tokens (spacing, radii, border width) for the app chrome.
//!
//! Single scaled token set so every chrome surface shares geometry.
//!
//! Base constants are in logical pixels. Multiply by `scale_factor` via
//! [`UiStyle::new`] at the call site.

/// Spacing scale (logical px). Used for insets, gaps and padding.
pub const SP_1: f32 = 4.0;
pub const SP_2: f32 = 8.0;

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
pub struct UiStyle {
    /// Spacing scale, physical px.
    pub sp1: f32,
    pub sp2: f32,
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
            r_panel: R_PANEL * scale,
            r_inner: R_INNER * scale,
            r_pill: R_PILL * scale,
            border: BORDER * scale,
        }
    }
}
