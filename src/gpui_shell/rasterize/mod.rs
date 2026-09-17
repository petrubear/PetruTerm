// gpui chrome migration (M1b): ANSI color resolution + cosmic-text
// rasterization of the terminal grid into an RGBA bitmap, plus the GPU
// sprite-atlas frame cache that avoids re-rasterizing/re-uploading when the
// grid hasn't changed. Originally split out of `terminal_element.rs` (M1a's
// own whole-branch review flagged that file as over the project's 400-line
// convention).
//
// Split again into this directory (TD-GPUI-03, 2026-09-17): the single
// `rasterize.rs` this replaces had grown to 880 lines -- `rasterize_grid`
// alone, the cosmic-text shape+blit pipeline, was ~510 lines and never named
// in any milestone's exit criteria, so it stayed over the 400-line
// convention from M1b all the way through M5. `cache.rs` holds the
// per-terminal GPU sprite-atlas frame cache, `colors.rs` the pure color/
// geometry helpers `rasterize_grid` builds on, `grid.rs` `rasterize_grid`
// itself (further broken into three private phase helpers along boundaries
// its own comments already called out). Pure code motion throughout: no
// logic changed, same computation order.

mod cache;
mod colors;
mod glyphs;
mod grid;
mod spans;

pub use cache::{evict_all, evict_terminal};
pub use grid::rasterize_grid;
