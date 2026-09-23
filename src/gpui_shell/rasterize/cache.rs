// The per-terminal GPU sprite-atlas frame cache.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, RenderImage};

// Caches the last rasterized frame per terminal (keyed by `Rc<Terminal>`'s
// heap address — stable for the terminal's lifetime, and distinct per
// split pane). Without this, `rasterize_grid` (driven by every repaint)
// would mint a brand-new `RenderImage` -- and therefore a brand-new
// globally-unique `ImageId` -- on every single paint, and gpui's Metal
// sprite atlas is insert-only: nothing prunes an entry except an explicit
// `window.drop_image(...)` call, so it would leak one full-grid GPU
// texture per paint. Caching the previous
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
/// Neither `evict_all` nor the same-key replacement inside `rasterize_grid`
/// fires when a pane closes, so without this a closed pane would strand
/// one full-grid texture in gpui's insert-only atlas.
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
