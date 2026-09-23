// Full key-event mapping to PTY escape sequences.
//
// Ported from `crate::app::input::key_map::translate_key` (the wgpu app's
// proven implementation): same escape sequences, same modifier semantics,
// but built on gpui's `Keystroke`/`Modifiers` instead of winit's `Key`/
// `Modifiers` -- `gpui_shell` must not import winit (see `spawn_terminal.rs`'s
// `spawn_terminal` doc comment for why), so this is a genuine port, not a
// shared function.

use alacritty_terminal::term::TermMode;
use gpui::{Keystroke, Modifiers};

/// Translates a gpui key event into the ANSI escape sequence to write to the
/// PTY, or `None` if this keystroke has nothing to send (e.g. a bare
/// modifier, or an unbound Cmd-combo gpui's own keybinding layer didn't
/// claim).
///
/// `option_as_meta`: when true, Alt/Option + character sends `ESC <char>`
/// (Meta key for Emacs/readline). When false (default), the OS-composed
/// character is sent as-is, which is correct for non-US keyboards where
/// Option produces `{`, `}`, `@`, `#`, etc.
pub fn translate_key(
    keystroke: &Keystroke,
    mode: TermMode,
    option_as_meta: bool,
) -> Option<Vec<u8>> {
    let Modifiers {
        control: ctrl,
        alt,
        shift,
        ..
    } = keystroke.modifiers;

    // gpui only populates `key_char` when none of ctrl/cmd/fn are held (see
    // gpui's platform/mac/events.rs `parse_keystroke`) -- so `key_char`
    // being `None` below always means ctrl, cmd, or fn is down, or this is
    // a named key with no printable form.

    // 1. Ctrl + character: compute the control byte ourselves (mirrors the
    // wgpu app's translate_key -- gpui doesn't hand us an OS-composed
    // control char here, `key_char` is None whenever ctrl is held).
    if ctrl && !alt {
        let c = if keystroke.key == "space" {
            Some(' ')
        } else if keystroke.key.chars().count() == 1 {
            keystroke.key.chars().next()
        } else {
            None
        };
        if let Some(c) = c {
            let byte = c.to_ascii_lowercase() as u8;
            if byte.is_ascii_lowercase() {
                return Some(vec![byte - b'a' + 1]);
            }
            return match byte {
                b'[' => Some(vec![0x1b]),
                b'\\' => Some(vec![0x1c]),
                b']' => Some(vec![0x1d]),
                b'^' => Some(vec![0x1e]),
                b'_' => Some(vec![0x1f]),
                b' ' => Some(vec![0x00]),
                _ => Some(keystroke.key.as_bytes().to_vec()),
            };
        }
    }

    // 2. Alt + character, Meta-key mode: only add the ESC prefix when
    // option_as_meta is explicitly enabled.
    if alt && !ctrl && option_as_meta {
        if let Some(ch) = &keystroke.key_char {
            let mut seq = vec![0x1b];
            seq.extend_from_slice(ch.as_bytes());
            return Some(seq);
        }
    }

    // 3. Enter is a special case: gpui always populates `key_char` with
    // "\n" (LF) for it, same as any other printable key, but terminal
    // convention sends "\r" (CR) -- and Shift+Enter needs the disambiguate
    // escape when that mode is active. Must be handled before the generic
    // key_char passthrough below, which would otherwise ship the wrong byte.
    if keystroke.key == "enter" {
        return if shift && mode.contains(TermMode::DISAMBIGUATE_ESC_CODES) {
            Some(b"\x1b[13;2u".to_vec())
        } else {
            Some(b"\r".to_vec())
        };
    }

    // 4. Plain printable character -- OS-composed, includes Alt-composed
    // characters when option_as_meta is false (default).
    if let Some(ch) = &keystroke.key_char {
        return Some(ch.as_bytes().to_vec());
    }

    // 5. Named keys (arrows, function keys, etc).
    // 2=Shift, 3=Alt, 4=Shift+Alt, 5=Ctrl, 6=Shift+Ctrl, 7=Alt+Ctrl, 8=Shift+Alt+Ctrl
    let mod_code = match (shift, alt, ctrl) {
        (true, false, false) => Some(2),
        (false, true, false) => Some(3),
        (true, true, false) => Some(4),
        (false, false, true) => Some(5),
        (true, false, true) => Some(6),
        (false, true, true) => Some(7),
        (true, true, true) => Some(8),
        _ => None,
    };

    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    match keystroke.key.as_str() {
        "up" => Some(format_csi('A', mod_code, app_cursor)),
        "down" => Some(format_csi('B', mod_code, app_cursor)),
        "right" => Some(format_csi('C', mod_code, app_cursor)),
        "left" => Some(format_csi('D', mod_code, app_cursor)),

        "home" => Some(format_csi('H', mod_code, app_cursor)),
        "end" => Some(format_csi('F', mod_code, app_cursor)),

        "pageup" => Some(format_tilde(5, mod_code)),
        "pagedown" => Some(format_tilde(6, mod_code)),
        "insert" => Some(format_tilde(2, mod_code)),
        "delete" => Some(format_tilde(3, mod_code)),

        "f1" => Some(format_fkey('P', mod_code)),
        "f2" => Some(format_fkey('Q', mod_code)),
        "f3" => Some(format_fkey('R', mod_code)),
        "f4" => Some(format_fkey('S', mod_code)),
        "f5" => Some(format_tilde(15, mod_code)),
        "f6" => Some(format_tilde(17, mod_code)),
        "f7" => Some(format_tilde(18, mod_code)),
        "f8" => Some(format_tilde(19, mod_code)),
        "f9" => Some(format_tilde(20, mod_code)),
        "f10" => Some(format_tilde(21, mod_code)),
        "f11" => Some(format_tilde(23, mod_code)),
        "f12" => Some(format_tilde(24, mod_code)),

        "tab" => {
            if shift {
                Some(b"\x1b[Z".to_vec()) // Shift+Tab -> reverse-tab (CSI Z)
            } else {
                Some(b"\t".to_vec())
            }
        }
        // "enter" is handled above (step 3), before the generic key_char
        // passthrough -- never reached here.
        "escape" => Some(b"\x1b".to_vec()),
        "backspace" => Some(b"\x7f".to_vec()),
        "space" => Some(b" ".to_vec()),

        // Not a named key we handle, and key_char was None (cmd/fn held, or
        // an unhandled named key like f13+) -- e.g. an unbound Cmd-combo
        // falls through here. gpui's keybinding layer already claims bound
        // Cmd-combos as app shortcuts before `on_key_down` ever sees them
        // (a matched action stops propagation), so this correctly swallows
        // the rest rather than leaking a stray byte into the shell.
        _ => None,
    }
}

fn format_csi(code: char, mod_code: Option<u8>, app_mode: bool) -> Vec<u8> {
    if let Some(m) = mod_code {
        format!("\x1b[1;{m}{code}").into_bytes()
    } else if app_mode {
        format!("\x1bO{code}").into_bytes()
    } else {
        format!("\x1b[{code}").into_bytes()
    }
}

fn format_tilde(num: u8, mod_code: Option<u8>) -> Vec<u8> {
    if let Some(m) = mod_code {
        format!("\x1b[{num};{m}~").into_bytes()
    } else {
        format!("\x1b[{num}~").into_bytes()
    }
}

fn format_fkey(code: char, mod_code: Option<u8>) -> Vec<u8> {
    if let Some(m) = mod_code {
        format!("\x1b[1;{m}{code}").into_bytes()
    } else {
        // F1-F4 use \x1bO<char> in normal mode
        format!("\x1bO{code}").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(|s| s.to_string()),
        }
    }

    #[test]
    fn plain_character_forwards_key_char() {
        let k = key("a", Some("a"), Modifiers::default());
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(b"a".to_vec())
        );
    }

    #[test]
    fn ctrl_c_sends_control_byte() {
        let k = key(
            "c",
            None,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(vec![0x03])
        );
    }

    #[test]
    fn ctrl_space_sends_nul() {
        let k = key(
            "space",
            None,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(vec![0x00])
        );
    }

    #[test]
    fn escape_sends_esc() {
        let k = key("escape", None, Modifiers::default());
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn arrow_up_normal_mode_sends_csi() {
        let k = key("up", None, Modifiers::default());
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn arrow_up_app_cursor_mode_sends_ss3() {
        let k = key("up", None, Modifiers::default());
        assert_eq!(
            translate_key(&k, TermMode::APP_CURSOR, false),
            Some(b"\x1bOA".to_vec())
        );
    }

    #[test]
    fn shift_arrow_up_sends_modified_csi() {
        let k = key(
            "up",
            None,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(b"\x1b[1;2A".to_vec())
        );
    }

    #[test]
    fn alt_character_without_meta_sends_composed_char() {
        let k = key(
            "2",
            Some("@"),
            Modifiers {
                alt: true,
                ..Default::default()
            },
        );
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(b"@".to_vec())
        );
    }

    #[test]
    fn alt_character_with_meta_prefixes_esc() {
        let k = key(
            "s",
            Some("s"),
            Modifiers {
                alt: true,
                ..Default::default()
            },
        );
        assert_eq!(
            translate_key(&k, TermMode::empty(), true),
            Some(b"\x1bs".to_vec())
        );
    }

    #[test]
    fn backspace_sends_del() {
        let k = key("backspace", None, Modifiers::default());
        assert_eq!(
            translate_key(&k, TermMode::empty(), false),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn shift_enter_disambiguate_sends_csi_u() {
        let k = key(
            "enter",
            Some("\n"),
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(
            translate_key(&k, TermMode::DISAMBIGUATE_ESC_CODES, false),
            Some(b"\x1b[13;2u".to_vec())
        );
    }

    #[test]
    fn unbound_cmd_combo_is_swallowed() {
        let k = key(
            "k",
            None,
            Modifiers {
                platform: true,
                ..Default::default()
            },
        );
        assert_eq!(translate_key(&k, TermMode::empty(), false), None);
    }
}
