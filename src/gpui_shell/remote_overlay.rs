// Modal popup with the phone-pairing QR. Opened by the palette's "Remote: Pair a phone" and by
// starting remote access; closes on Escape or after the code expires. Same modal contract as
// `info_overlay.rs`: the backdrop swallows clicks, the key guard swallows keys.

use gpui::{
    div, prelude::*, px, rgb, rgba, App, Context, FontWeight, KeyDownEvent, MouseButton,
    MouseDownEvent, Window,
};
use qrcode::{Color, QrCode};
use std::time::Instant;

use crate::config::schema::ColorScheme;

use super::font_state;
use super::pane_view::to_rgba;
use super::GpuiShellRoot;

const CELL_PX: f32 = 6.0;

#[derive(Default)]
pub struct RemoteOverlay {
    shown: Option<Shown>,
}

struct Shown {
    code: String,
    relay: String,
    /// `rows[y]` is the run-length encoding of one QR row: `(dark, length)`.
    rows: Vec<Vec<(bool, usize)>>,
    until: Instant,
}

impl RemoteOverlay {
    pub fn is_visible(&self) -> bool {
        self.shown.is_some()
    }

    pub fn open(&mut self, code: String, relay: String, ttl: std::time::Duration) {
        let rows = qr_rows(&code);
        self.shown = Some(Shown {
            code,
            relay,
            rows,
            until: Instant::now() + ttl,
        });
    }

    pub fn close(&mut self) {
        self.shown = None;
    }

    /// Close once the code can no longer be used. Returns whether it closed.
    pub fn expire(&mut self) -> bool {
        let gone = self
            .shown
            .as_ref()
            .is_some_and(|s| Instant::now() >= s.until);
        if gone {
            self.shown = None;
        }
        gone
    }
}

fn qr_rows(data: &str) -> Vec<Vec<(bool, usize)>> {
    let Ok(qr) = QrCode::new(data.as_bytes()) else {
        return Vec::new();
    };
    let width = qr.width();
    let colors = qr.to_colors();
    colors
        .chunks(width)
        .map(|row| {
            let mut runs: Vec<(bool, usize)> = Vec::new();
            for c in row {
                let dark = *c == Color::Dark;
                match runs.last_mut() {
                    Some((d, n)) if *d == dark => *n += 1,
                    _ => runs.push((dark, 1)),
                }
            }
            runs
        })
        .collect()
}

pub fn render(overlay: &RemoteOverlay, colors: &ColorScheme) -> impl IntoElement {
    let shown = overlay.shown.as_ref();
    let text = to_rgba(colors.foreground);
    let quiet = 2usize; // white border so scanners find the code on a dark theme
    let width = shown
        .and_then(|s| s.rows.first())
        .map(|r| r.iter().map(|(_, n)| n).sum::<usize>())
        .unwrap_or(0);
    let blank = |n: usize| {
        div()
            .w(px(CELL_PX * n as f32))
            .h(px(CELL_PX))
            .bg(rgb(0xffffff))
    };
    let mut grid = div().flex().flex_col();
    if let Some(s) = shown {
        for _ in 0..quiet {
            grid = grid.child(blank(width + quiet * 2));
        }
        for row in &s.rows {
            let mut line = div().flex().child(blank(quiet));
            for (dark, n) in row {
                let c = if *dark { rgb(0x000000) } else { rgb(0xffffff) };
                line = line.child(div().w(px(CELL_PX * *n as f32)).h(px(CELL_PX)).bg(c));
            }
            grid = grid.child(line.child(blank(quiet)));
        }
        for _ in 0..quiet {
            grid = grid.child(blank(width + quiet * 2));
        }
    }
    let (relay, code) = shown
        .map(|s| (s.relay.clone(), s.code.clone()))
        .unwrap_or_default();

    div()
        .id("remote-overlay-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0000_00aa))
        .on_mouse_down(
            MouseButton::Left,
            |_: &MouseDownEvent, _w: &mut Window, cx: &mut App| cx.stop_propagation(),
        )
        .child(
            div()
                .id("remote-overlay-content")
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .p_4()
                .max_w(px(560.0))
                .bg(to_rgba(colors.ui_surface))
                .border_1()
                .border_color(to_rgba(colors.ui_border))
                .font_family(font_state::font_family())
                .text_color(text)
                .on_mouse_down(
                    MouseButton::Left,
                    |_: &MouseDownEvent, _w: &mut Window, cx: &mut App| cx.stop_propagation(),
                )
                .child(div().font_weight(FontWeight::BOLD).child("Pair your phone"))
                .child(grid)
                .child(format!(
                    "Scan with the PetruTerm app. Phone connects to {relay}"
                ))
                .child(div().text_size(px(11.0)).max_w(px(520.0)).child(code))
                .child(
                    div()
                        .text_size(px(11.0))
                        .child("Valid for 2 minutes, works once. Esc closes."),
                ),
        )
}

impl GpuiShellRoot {
    /// Key guard: Esc closes, everything else is swallowed while the QR is up.
    pub(super) fn maybe_handle_remote_overlay_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.remote_overlay.is_visible() {
            return false;
        }
        if event.keystroke.key == "escape" {
            self.remote_overlay.close();
            cx.notify();
        }
        true
    }
}
