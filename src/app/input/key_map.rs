use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};
use alacritty_terminal::term::TermMode;

/// Translates a winit key event into an ANSI escape sequence.
pub fn translate_key(
    key: &Key,
    mods: Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let state = mods.state();
    let shift = state.shift_key();
    let ctrl = state.control_key();
    let alt = state.alt_key();
    let logo = state.super_key();

    // 1. Handle Characters (with Ctrl/Alt modifiers)
    if let Key::Character(s) = key {
        let c = s.chars().next()?;
        
        // Ctrl + Key
        if ctrl && !alt && !logo {
            let byte = c.to_ascii_lowercase() as u8;
            if byte.is_ascii_lowercase() {
                return Some(vec![byte - b'a' + 1]);
            }
            // Other common ctrl mappings
            return match byte {
                b'[' => Some(vec![0x1b]),
                b'\\' => Some(vec![0x1c]),
                b']' => Some(vec![0x1d]),
                b'^' => Some(vec![0x1e]),
                b'_' => Some(vec![0x1f]),
                b' ' => Some(vec![0x00]),
                _ => Some(s.as_bytes().to_vec()),
            };
        }
        
        // Alt + Key (Escape prefix)
        if alt && !ctrl && !logo {
            let mut seq = vec![0x1b];
            seq.extend_from_slice(s.as_bytes());
            return Some(seq);
        }

        // Just the character
        return Some(s.as_bytes().to_vec());
    }

    // 2. Handle Named Keys (Arrows, Function Keys, etc.)
    if let Key::Named(named) = key {
        // Determine xterm modifier code
        // 2=Shift, 3=Alt, 4=Shift+Alt, 5=Ctrl, 6=Shift+Ctrl, 7=Alt+Ctrl, 8=Shift+Alt+Ctrl
        let mod_code = match (shift, alt, ctrl) {
            (true,  false, false) => Some(2),
            (false, true,  false) => Some(3),
            (true,  true,  false) => Some(4),
            (false, false, true ) => Some(5),
            (true,  false, true ) => Some(6),
            (false, true,  true ) => Some(7),
            (true,  true,  true ) => Some(8),
            _ => None,
        };

        let app_cursor = mode.contains(TermMode::APP_CURSOR);
        let _app_keypad = mode.contains(TermMode::APP_KEYPAD);

        return match named {
            NamedKey::ArrowUp => Some(format_csi('A', mod_code, app_cursor)),
            NamedKey::ArrowDown => Some(format_csi('B', mod_code, app_cursor)),
            NamedKey::ArrowRight => Some(format_csi('C', mod_code, app_cursor)),
            NamedKey::ArrowLeft => Some(format_csi('D', mod_code, app_cursor)),
            
            NamedKey::Home => Some(format_csi('H', mod_code, app_cursor)),
            NamedKey::End => Some(format_csi('F', mod_code, app_cursor)),
            
            NamedKey::PageUp => Some(format_tilde(5, mod_code)),
            NamedKey::PageDown => Some(format_tilde(6, mod_code)),
            NamedKey::Insert => Some(format_tilde(2, mod_code)),
            NamedKey::Delete => Some(format_tilde(3, mod_code)),

            NamedKey::F1 => Some(format_fkey('P', mod_code, 11)),
            NamedKey::F2 => Some(format_fkey('Q', mod_code, 12)),
            NamedKey::F3 => Some(format_fkey('R', mod_code, 13)),
            NamedKey::F4 => Some(format_fkey('S', mod_code, 14)),
            NamedKey::F5 => Some(format_tilde(15, mod_code)),
            NamedKey::F6 => Some(format_tilde(17, mod_code)),
            NamedKey::F7 => Some(format_tilde(18, mod_code)),
            NamedKey::F8 => Some(format_tilde(19, mod_code)),
            NamedKey::F9 => Some(format_tilde(20, mod_code)),
            NamedKey::F10 => Some(format_tilde(21, mod_code)),
            NamedKey::F11 => Some(format_tilde(23, mod_code)),
            NamedKey::F12 => Some(format_tilde(24, mod_code)),

            NamedKey::Tab => {
                if shift {
                    Some(b"\x1b[Z".to_vec())  // Shift+Tab → reverse-tab (CSI Z)
                } else {
                    Some(b"\t".to_vec())
                }
            }
            NamedKey::Enter => Some(b"\r".to_vec()),
            NamedKey::Escape => Some(b"\x1b".to_vec()),
            NamedKey::Backspace => Some(b"\x7f".to_vec()),
            NamedKey::Space => Some(b" ".to_vec()),

            _ => None,
        };
    }

    None
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

fn format_fkey(code: char, mod_code: Option<u8>, _num: u8) -> Vec<u8> {
    if let Some(m) = mod_code {
        format!("\x1b[1;{m}{code}").into_bytes()
    } else {
        // F1-F4 use \x1bO<char> in normal mode
        format!("\x1bO{code}").into_bytes()
    }
}
