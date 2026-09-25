// Builds the chat panel drawer element (panel + left-edge drag handle).
// Called only from `render()`'s `middle_row` construction, inside
// `.when(self.chat.is_visible(), ...)`. Mirrors `render_sidebar.rs`.

use std::rc::Rc;
use std::time::Duration;

use gpui::{div, ease_out_quint, prelude::*, px, Animation, AnimationExt as _, Context};

use super::chat_panel::{self, ChatPillCallback, MAX_PANEL_WIDTH_PX, MIN_PANEL_WIDTH_PX};
use super::pane_view::to_rgba;
use super::render::CARD_GAP_PX;
use super::resize_handle::{self, ResizeHandleElement, ResizeHandleId};
use super::GpuiShellRoot;

/// Duration of the drawer's opening grow animation.
const CHAT_PANEL_OPEN_ANIM: Duration = Duration::from_millis(180);

/// Keep at least this much of the window for the terminal card.
const MIN_TERMINAL_WIDTH_PX: f32 = 320.0;

impl GpuiShellRoot {
    pub(super) fn render_chat_drawer(
        &mut self,
        on_fix_last_error: ChatPillCallback,
        on_explain_last_output: ChatPillCallback,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let width_px = self.chat_width_px;
        let panel = chat_panel::render_chat_panel(
            &self.chat,
            &self.config.llm,
            &self.config.colors,
            width_px,
            on_fix_last_error,
            on_explain_last_output,
        );
        let panel = panel.with_animation(
            "chat-panel-drawer",
            Animation::new(CHAT_PANEL_OPEN_ANIM).with_easing(ease_out_quint()),
            move |panel, delta| panel.w(px(width_px * delta)),
        );

        // Same floating-over-the-gap handle as the sidebar's, on the panel's
        // LEFT edge. The panel's right edge is fixed at the window's right
        // padding, so the new width is measured from there; the handle's
        // centre sits half a gap left of the panel edge, so the pointer
        // doesn't make the edge jump on grab.
        let drag_view = cx.entity().downgrade();
        let on_drag: resize_handle::ResizeDragCallback = Rc::new(move |position, window, cx| {
            let viewport_w = f32::from(window.viewport_size().width);
            drag_view
                .update(cx, |root, cx| {
                    let max = MAX_PANEL_WIDTH_PX
                        .min(viewport_w - MIN_TERMINAL_WIDTH_PX)
                        .max(MIN_PANEL_WIDTH_PX);
                    let new_width =
                        (viewport_w - CARD_GAP_PX - f32::from(position.x) - CARD_GAP_PX / 2.0)
                            .clamp(MIN_PANEL_WIDTH_PX, max);
                    if root.chat_width_px != new_width {
                        root.chat_width_px = new_width;
                        cx.notify();
                    }
                })
                .ok();
        });
        let handle = div()
            .absolute()
            .top_0()
            .left(px(-CARD_GAP_PX))
            .h_full()
            .w(px(CARD_GAP_PX))
            .cursor_col_resize()
            .child(ResizeHandleElement {
                id: ResizeHandleId::Chat,
                color: to_rgba(self.config.colors.ui_border),
                on_drag,
            });

        div()
            .relative()
            .flex_shrink_0()
            .h_full()
            .child(panel)
            .child(handle)
    }
}
