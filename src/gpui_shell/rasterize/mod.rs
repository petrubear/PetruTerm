// ANSI color resolution + cosmic-text rasterization of the terminal grid
// into an RGBA bitmap. `grid.rs` holds `rasterize_grid`, `glyphs.rs` the
// glyph-draw pass, `spans.rs` the shaping spans, `colors.rs` color/geometry
// helpers, `cache.rs` the per-terminal GPU sprite-atlas frame cache.

mod cache;
mod colors;
mod glyphs;
mod grid;
mod spans;

pub use cache::{evict_all, evict_terminal};
pub use grid::rasterize_grid;
