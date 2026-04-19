use crate::config::schema::ColorScheme;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

/// Resolve an alacritty terminal color to linear RGBA f32.
///
/// Named/Indexed colors map to the active theme palette.
/// Spec colors are passed through directly (already sRGB).
pub fn resolve_color(color: AnsiColor, scheme: &ColorScheme) -> [f32; 4] {
    match color {
        AnsiColor::Named(named) => resolve_named(named, scheme),
        AnsiColor::Indexed(idx) => resolve_indexed(idx, scheme),
        AnsiColor::Spec(rgb) => [
            rgb.r as f32 / 255.0,
            rgb.g as f32 / 255.0,
            rgb.b as f32 / 255.0,
            1.0,
        ],
    }
}

fn resolve_named(name: NamedColor, scheme: &ColorScheme) -> [f32; 4] {
    match name {
        NamedColor::Black => scheme.ansi[0],
        NamedColor::Red => scheme.ansi[1],
        NamedColor::Green => scheme.ansi[2],
        NamedColor::Yellow => scheme.ansi[3],
        NamedColor::Blue => scheme.ansi[4],
        NamedColor::Magenta => scheme.ansi[5],
        NamedColor::Cyan => scheme.ansi[6],
        NamedColor::White => scheme.ansi[7],
        NamedColor::BrightBlack => scheme.brights[0],
        NamedColor::BrightRed => scheme.brights[1],
        NamedColor::BrightGreen => scheme.brights[2],
        NamedColor::BrightYellow => scheme.brights[3],
        NamedColor::BrightBlue => scheme.brights[4],
        NamedColor::BrightMagenta => scheme.brights[5],
        NamedColor::BrightCyan => scheme.brights[6],
        NamedColor::BrightWhite => scheme.brights[7],
        NamedColor::Foreground => scheme.foreground,
        NamedColor::Background => scheme.background,
        NamedColor::Cursor => scheme.cursor_bg,
        // Dim variants — use normal colors at reduced alpha.
        NamedColor::DimBlack => dim(scheme.ansi[0]),
        NamedColor::DimRed => dim(scheme.ansi[1]),
        NamedColor::DimGreen => dim(scheme.ansi[2]),
        NamedColor::DimYellow => dim(scheme.ansi[3]),
        NamedColor::DimBlue => dim(scheme.ansi[4]),
        NamedColor::DimMagenta => dim(scheme.ansi[5]),
        NamedColor::DimCyan => dim(scheme.ansi[6]),
        NamedColor::DimWhite => dim(scheme.ansi[7]),
        NamedColor::DimForeground => dim(scheme.foreground),
        // Catch-all for any future named colors added upstream.
        _ => scheme.foreground,
    }
}

fn resolve_indexed(idx: u8, scheme: &ColorScheme) -> [f32; 4] {
    match idx {
        0..=15 => scheme.index_color(idx),
        16..=231 => {
            // 6x6x6 color cube
            let i = idx - 16;
            let b = i % 6;
            let g = (i / 6) % 6;
            let r = i / 36;
            let to_f = |v: u8| {
                if v == 0 {
                    0.0
                } else {
                    (55 + v * 40) as f32 / 255.0
                }
            };
            [to_f(r), to_f(g), to_f(b), 1.0]
        }
        232..=255 => {
            // Grayscale ramp
            let v = (idx - 232) * 10 + 8;
            let f = v as f32 / 255.0;
            [f, f, f, 1.0]
        }
    }
}

fn dim(color: [f32; 4]) -> [f32; 4] {
    [color[0] * 0.6, color[1] * 0.6, color[2] * 0.6, color[3]]
}
