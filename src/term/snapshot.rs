//! Serialize the visible grid of a `Term` into ANSI so a late-joining remote client can
//! rebuild the screen, then continue with the live byte stream.
//!
//! Covered: cell characters (with zero-width combiners), colors, text attributes, cursor
//! position and visibility, alt screen, and the input-affecting modes (application
//! cursor/keypad, bracketed paste, mouse, focus reporting). Not covered: scrollback, the
//! primary screen while the alt screen is active (alacritty keeps it private), scroll
//! regions, origin mode, and Kitty keyboard flags.

// No consumer outside tests until the remote server lands.
#![cfg_attr(not(test), allow(dead_code))]

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use alacritty_terminal::Term;
use std::fmt::Write;

/// Attributes that change how a blank cell looks.
const VISIBLE_ON_BLANK: Flags = Flags::INVERSE
    .union(Flags::ALL_UNDERLINES)
    .union(Flags::STRIKEOUT);
/// Attributes re-emitted as SGR.
const SGR_FLAGS: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::UNDERLINE)
    .union(Flags::DOUBLE_UNDERLINE)
    .union(Flags::INVERSE)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT);

pub fn snapshot_ansi<T: EventListener>(term: &Term<T>) -> Vec<u8> {
    let mode = *term.mode();
    let grid = term.grid();
    let mut out = String::new();

    if mode.contains(TermMode::ALT_SCREEN) {
        out.push_str("\x1b[?1049h");
    }
    out.push_str("\x1b[0m\x1b[2J");

    for row in 0..grid.screen_lines() {
        let line = &grid[Line(row as i32)];
        let mut end = grid.columns();
        while end > 0 && is_blank(&line[Column(end - 1)]) {
            end -= 1;
        }
        if end == 0 {
            continue;
        }
        let _ = write!(out, "\x1b[{};1H", row + 1);
        let mut current = Style::default();
        for col in 0..end {
            let cell = &line[Column(col)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let style = Style::of(cell);
            if style != current {
                style.write_sgr(&mut out);
                current = style;
            }
            out.push(cell.c);
            for &z in cell.zerowidth().unwrap_or(&[]) {
                out.push(z);
            }
        }
        out.push_str("\x1b[0m");
    }

    for (flag, seq) in [
        (TermMode::APP_CURSOR, "\x1b[?1h"),
        (TermMode::APP_KEYPAD, "\x1b="),
        (TermMode::BRACKETED_PASTE, "\x1b[?2004h"),
        (TermMode::MOUSE_REPORT_CLICK, "\x1b[?1000h"),
        (TermMode::MOUSE_DRAG, "\x1b[?1002h"),
        (TermMode::MOUSE_MOTION, "\x1b[?1003h"),
        (TermMode::UTF8_MOUSE, "\x1b[?1005h"),
        (TermMode::SGR_MOUSE, "\x1b[?1006h"),
        (TermMode::FOCUS_IN_OUT, "\x1b[?1004h"),
        (TermMode::ALTERNATE_SCROLL, "\x1b[?1007h"),
    ] {
        if mode.contains(flag) {
            out.push_str(seq);
        }
    }

    let cursor = grid.cursor.point;
    let _ = write!(
        out,
        "\x1b[{};{}H",
        cursor.line.0.max(0) + 1,
        cursor.column.0 + 1
    );
    out.push_str(if mode.contains(TermMode::SHOW_CURSOR) {
        "\x1b[?25h"
    } else {
        "\x1b[?25l"
    });
    out.into_bytes()
}

fn is_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && cell.bg == Color::Named(NamedColor::Background)
        && !cell.flags.intersects(VISIBLE_ON_BLANK)
        && cell.zerowidth().is_none()
}

#[derive(Clone, Copy, PartialEq)]
struct Style {
    fg: Color,
    bg: Color,
    flags: Flags,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
        }
    }
}

impl Style {
    fn of(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & SGR_FLAGS,
        }
    }

    /// Absolute SGR: resets first, so it does not depend on the previous style.
    fn write_sgr(&self, out: &mut String) {
        out.push_str("\x1b[0");
        for (flag, code) in [
            (Flags::BOLD, 1),
            (Flags::DIM, 2),
            (Flags::ITALIC, 3),
            (Flags::UNDERLINE, 4),
            (Flags::INVERSE, 7),
            (Flags::HIDDEN, 8),
            (Flags::STRIKEOUT, 9),
            (Flags::DOUBLE_UNDERLINE, 21),
        ] {
            if self.flags.contains(flag) {
                let _ = write!(out, ";{code}");
            }
        }
        write_color(out, self.fg, 30, 90, 38);
        write_color(out, self.bg, 40, 100, 48);
        out.push('m');
    }
}

fn write_color(out: &mut String, color: Color, base: u8, bright: u8, ext: u8) {
    match color {
        Color::Named(named) => match named_index(named) {
            Some(i) if i < 8 => {
                let _ = write!(out, ";{}", base + i);
            }
            Some(i) => {
                let _ = write!(out, ";{}", bright + i - 8);
            }
            None => {}
        },
        Color::Indexed(i) => {
            let _ = write!(out, ";{ext};5;{i}");
        }
        Color::Spec(rgb) => {
            let _ = write!(out, ";{ext};2;{};{};{}", rgb.r, rgb.g, rgb.b);
        }
    }
}

/// ANSI palette index 0..=15 for named colors; `None` for the default fg/bg/cursor.
fn named_index(named: NamedColor) -> Option<u8> {
    let n = named as usize;
    match n {
        0..=15 => Some(n as u8),
        n if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&n) => {
            Some((n - NamedColor::DimBlack as usize) as u8)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::TermSize;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    fn term(cols: usize, rows: usize) -> Term<VoidListener> {
        let size = TermSize {
            cols,
            rows,
            scrollback: 100,
        };
        Term::new(Config::default(), &size, VoidListener)
    }

    fn feed(term: &mut Term<VoidListener>, bytes: &[u8]) {
        let mut p: Processor<StdSyncHandler> = Processor::new();
        p.advance(term, bytes);
    }

    /// Feed `input` into one term, replay its snapshot into a fresh one, compare screens.
    fn assert_round_trip(input: &[u8]) {
        let (cols, rows) = (30, 8);
        let mut a = term(cols, rows);
        feed(&mut a, input);
        let mut b = term(cols, rows);
        feed(&mut b, &snapshot_ansi(&a));

        let compare = SGR_FLAGS | Flags::WIDE_CHAR;
        for r in 0..rows {
            for c in 0..cols {
                let (ca, cb) = (
                    &a.grid()[Line(r as i32)][Column(c)],
                    &b.grid()[Line(r as i32)][Column(c)],
                );
                assert_eq!(ca.c, cb.c, "char at {r},{c}");
                assert_eq!(ca.fg, cb.fg, "fg at {r},{c}");
                assert_eq!(ca.bg, cb.bg, "bg at {r},{c}");
                assert_eq!(ca.flags & compare, cb.flags & compare, "flags at {r},{c}");
            }
        }
        assert_eq!(a.grid().cursor.point, b.grid().cursor.point, "cursor");
        let modes = TermMode::ALT_SCREEN
            | TermMode::SHOW_CURSOR
            | TermMode::APP_CURSOR
            | TermMode::BRACKETED_PASTE
            | TermMode::SGR_MOUSE
            | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(*a.mode() & modes, *b.mode() & modes, "modes");
    }

    #[test]
    fn plain_text_and_cursor() {
        assert_round_trip(b"hello\r\nworld");
    }

    #[test]
    fn colors_and_attributes() {
        assert_round_trip(
            b"\x1b[1;31mred bold\x1b[0m \x1b[48;5;200;38;2;1;2;3mx\x1b[0m\r\n\
              \x1b[4;7mul inv\x1b[0m\x1b[93m bright\x1b[44m on blue\x1b[0m",
        );
    }

    #[test]
    fn blank_cells_with_background_survive() {
        assert_round_trip(b"\x1b[42m      \x1b[0mtail");
    }

    #[test]
    fn wide_and_combining_chars() {
        assert_round_trip("a\u{4e2d}\u{6587}b e\u{301}x".as_bytes());
    }

    #[test]
    fn full_width_row() {
        assert_round_trip(b"123456789012345678901234567890\r\nnext");
    }

    #[test]
    fn alt_screen_modes_and_hidden_cursor() {
        assert_round_trip(
            b"primary\x1b[?1049h\x1b[?25l\x1b[?2004h\x1b[?1006h\x1b[?1000h\x1b[3;4Hui",
        );
    }

    #[test]
    fn empty_screen() {
        assert_round_trip(b"");
    }
}
