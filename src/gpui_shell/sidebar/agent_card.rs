// State color of an Agents card: a faint tint over the row plus an opaque left edge.
// Pure function of the state and the theme tokens, so it is unit-tested without gpui.
// See `specs/003-agent-activity-cards/contracts/sidebar-agent-card.md`.

use crate::config::schema::ColorScheme;
use crate::term::attention::{AgentState, ATTENTION_COLOR};

/// Opacity of the tint drawn over the row. Low on purpose: it must not hurt text contrast.
pub const TINT_ALPHA: f32 = 0.12;
const _: () = assert!(TINT_ALPHA < 0.2);

/// Width in px of the left edge. Always reserved (transparent when unused) so the text does
/// not shift when a card changes state.
pub const EDGE_WIDTH: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardStyle {
    pub tint: Option<[f32; 4]>,
    pub edge: Option<[f32; 4]>,
}

/// Idle and not-yet-reported agents stay neutral; the other states get their color.
pub fn card_style(state: Option<AgentState>, colors: &ColorScheme) -> CardStyle {
    let base = match state {
        Some(AgentState::Working) => colors.ui_accent,
        Some(AgentState::NeedsInput) => ATTENTION_COLOR,
        Some(AgentState::Done) => colors.ui_success,
        Some(AgentState::Idle) | None => {
            return CardStyle {
                tint: None,
                edge: None,
            }
        }
    };
    CardStyle {
        tint: Some([base[0], base[1], base[2], TINT_ALPHA]),
        edge: Some([base[0], base[1], base[2], 1.0]),
    }
}

/// Outline of the selected card: the agent's own color (the one its name is drawn in), or
/// `fallback` for an agent without one.
pub fn selected_outline(agent_color: Option<[u8; 3]>, fallback: [f32; 4]) -> [f32; 4] {
    match agent_color {
        Some([r, g, b]) => [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ],
        None => fallback,
    }
}

/// Opaque card background: the state tint over `base` (no tint leaves `base`).
pub fn card_background(style: &CardStyle, base: [f32; 4]) -> [f32; 4] {
    match style.tint {
        Some(t) => {
            let a = t[3];
            [
                t[0] * a + base[0] * (1.0 - a),
                t[1] * a + base[1] * (1.0 - a),
                t[2] * a + base[2] * (1.0 - a),
                base[3],
            ]
        }
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> [f32; 4] {
        let v = |i: usize| f32::from(u8::from_str_radix(&s[i..i + 2], 16).unwrap()) / 255.0;
        [v(1), v(3), v(5), 1.0]
    }

    /// Tokens of the two shipped petru themes that matter for a card.
    #[allow(clippy::too_many_arguments)]
    fn scheme(
        fg: &str,
        hover: &str,
        active: &str,
        muted: &str,
        accent: &str,
        ok: &str,
    ) -> ColorScheme {
        let z = [0.0; 4];
        ColorScheme {
            foreground: hex(fg),
            background: z,
            cursor_bg: z,
            cursor_fg: z,
            cursor_border: z,
            selection_bg: z,
            selection_fg: z,
            ansi: [z; 8],
            brights: [z; 8],
            ui_accent: hex(accent),
            ui_surface: z,
            ui_surface_active: hex(active),
            ui_surface_hover: hex(hover),
            ui_muted: hex(muted),
            ui_success: hex(ok),
            ui_overlay: z,
            ui_border: z,
        }
    }

    fn dark() -> ColorScheme {
        scheme(
            "#eef0f2", "#1f262d", "#3b4754", "#a4adb7", "#b983fc", "#8dfc83",
        )
    }

    fn light() -> ColorScheme {
        scheme(
            "#1d1d20", "#dcdce5", "#d1d0dd", "#504d6a", "#903ef3", "#128108",
        )
    }

    fn luminance(c: [f32; 4]) -> f32 {
        let lin = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    }

    fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn each_state_has_its_own_color() {
        let c = dark();
        let working = card_style(Some(AgentState::Working), &c);
        let needs = card_style(Some(AgentState::NeedsInput), &c);
        let done = card_style(Some(AgentState::Done), &c);
        assert_eq!(working.edge.unwrap()[..3], c.ui_accent[..3]);
        assert_eq!(needs.edge.unwrap()[..3], ATTENTION_COLOR[..3]);
        assert_eq!(done.edge.unwrap()[..3], c.ui_success[..3]);
        assert_ne!(working.edge, needs.edge);
        assert_ne!(needs.edge, done.edge);
        assert_ne!(working.edge, done.edge);
    }

    #[test]
    fn idle_and_unreported_are_neutral() {
        let neutral = CardStyle {
            tint: None,
            edge: None,
        };
        assert_eq!(card_style(Some(AgentState::Idle), &dark()), neutral);
        assert_eq!(card_style(None, &dark()), neutral);
    }

    #[test]
    fn tint_is_faint_and_edge_is_opaque() {
        for state in [
            AgentState::Working,
            AgentState::NeedsInput,
            AgentState::Done,
        ] {
            let s = card_style(Some(state), &light());
            assert_eq!(s.tint.unwrap()[3], TINT_ALPHA);
            assert_eq!(s.edge.unwrap()[3], 1.0);
        }
    }

    /// Text readable (4.5:1) on the plain card stays readable under the tint; text already
    /// below that cannot get more than 10% worse. Muted text on the resting card, foreground
    /// text on the selected one.
    #[test]
    fn the_tint_keeps_text_readable_in_either_theme() {
        for (name, c) in [("dark", dark()), ("light", light())] {
            for (base, text) in [
                (c.ui_surface_hover, c.ui_muted),
                (c.ui_surface_active, c.foreground),
            ] {
                let plain = contrast(text, base);
                for state in [
                    AgentState::Working,
                    AgentState::NeedsInput,
                    AgentState::Done,
                ] {
                    let bg = card_background(&card_style(Some(state), &c), base);
                    let tinted = contrast(text, bg);
                    assert!(
                        tinted >= 4.5_f32.min(plain * 0.9),
                        "{name} {state:?}: contrast {plain:.2} fell to {tinted:.2}"
                    );
                }
            }
        }
    }

    #[test]
    fn selected_outline_uses_the_agent_color_or_the_fallback() {
        let fb = [0.1, 0.2, 0.3, 1.0];
        assert_eq!(
            selected_outline(Some([255, 0, 128]), fb),
            [1.0, 0.0, 128.0 / 255.0, 1.0]
        );
        assert_eq!(selected_outline(None, fb), fb);
    }

    #[test]
    fn background_is_the_tint_over_the_base_and_neutral_keeps_it() {
        let c = dark();
        let base = c.ui_surface_hover;
        assert_eq!(card_background(&card_style(None, &c), base), base);
        let bg = card_background(&card_style(Some(AgentState::Working), &c), base);
        assert_ne!(bg, base);
        assert_eq!(bg[3], 1.0);
        // the tint pulls the base toward the state color, by its alpha
        let want = c.ui_accent[0] * TINT_ALPHA + base[0] * (1.0 - TINT_ALPHA);
        assert!((bg[0] - want).abs() < 1e-6);
    }
}
