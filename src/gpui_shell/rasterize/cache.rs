// gpui chrome migration (TD-GPUI-03 split): the per-terminal GPU sprite-atlas
// frame cache. Split out of the single `rasterize.rs` (M1b) for the
// 400-line convention -- pure code motion, no logic changed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, RenderImage};

// Caches the last rasterized frame per terminal (keyed by `Rc<Terminal>`'s
// heap address — stable for the terminal's lifetime, and distinct per
// split pane). Without this, `rasterize_grid` (driven by the ~30Hz
// repaint poll in `gpui_shell/mod.rs`) would mint a brand-new
// `RenderImage` — and therefore a brand-new globally-unique `ImageId`
// (gpui 0.2.2's `assets.rs` static counter) — on every single paint, and
// gpui's Metal sprite atlas is insert-only: nothing prunes an entry
// except an explicit `window.drop_image(...)` call. That leaked one
// full-grid GPU texture per paint, unbounded (measured: 598MB -> 2.52GB
// RSS in 17s, idle, one terminal — the M0 leak). Caching the previous
// frame and explicitly dropping it before inserting the next bounds the
// atlas to one live texture per terminal, and skipping the
// rasterize+upload entirely when the grid content hasn't changed also
// avoids needless GPU uploads while idle.
thread_local! {
    pub(super) static LAST_IMAGE: RefCell<HashMap<usize, CachedFrame>> = RefCell::new(HashMap::new());
}

pub(super) struct CachedFrame {
    /// Hash of the shaped row text, per-cell colors/style, and the bitmap's
    /// pixel dimensions — covers content changes (typing, scrolling, color
    /// changes, selection changes) and resize/rescale (cell size or window
    /// scale factor changing bitmap resolution).
    pub(super) content_hash: u64,
    pub(super) image: Arc<RenderImage>,
}

/// Drop every cached frame's GPU sprite-atlas entry across all windows —
/// called by `font_state::reload_font_config` when the font changes, since
/// every cached frame is stale the moment the font changes (keyed on content
/// hash, not font identity) and `cache.clear()` alone would free only the
/// Rust-side `Arc<RenderImage>` handles while leaking the underlying GPU
/// texture (gpui's atlas is insert-only — see `LAST_IMAGE`'s doc comment).
pub fn evict_all(cx: &mut App) {
    let evicted: Vec<Arc<RenderImage>> =
        LAST_IMAGE.with_borrow_mut(|cache| cache.drain().map(|(_, frame)| frame.image).collect());
    for image in evicted {
        cx.drop_image(image, None);
    }
}

/// Drop ONE terminal's cached frame and its GPU sprite-atlas entry, for a
/// pane that is going away.
///
/// `evict_all` (font reload) and the same-key replacement inside
/// `rasterize_grid` were the only two things that ever pruned `LAST_IMAGE`,
/// and neither fires when a pane closes — so every closed pane used to
/// strand one full-grid texture in gpui's insert-only Metal atlas for the
/// rest of the session. That is the M0 leak this cache exists to prevent
/// (see `LAST_IMAGE`'s doc comment), just at pane granularity instead of
/// per-paint: unreachable until M2 made panes and tabs closable at all.
///
/// `terminal_key` is the `Rc<Terminal>` heap address, so the caller MUST
/// call this while it still holds that `Rc` — once the last handle drops,
/// the address is gone and can be recycled by a later pane, which would
/// leave this entry stranded and hand the new pane a dead one.
pub fn evict_terminal(terminal_key: usize, cx: &mut App) {
    let evicted = LAST_IMAGE.with_borrow_mut(|cache| cache.remove(&terminal_key));
    if let Some(frame) = evicted {
        cx.drop_image(frame.image, None);
    }
}
