/// Detects CSI erase sequences that invalidate block decorations:
///   ESC [ 2 J  — erase display (clear moves viewport to scrollback)
///   ESC [ 3 J  — erase saved (clears scrollback)
///
/// Returns `true` on the final `J` byte of a matching sequence.
pub struct EraseScanner {
    state: EraseState,
}

enum EraseState {
    Normal,
    Esc,
    CsiBracket,
    CsiParam(u8), // accumulates the last digit seen
}

impl EraseScanner {
    pub fn new() -> Self {
        Self {
            state: EraseState::Normal,
        }
    }

    /// Feed one byte. Returns `true` if a `CSI 2 J` or `CSI 3 J` was completed.
    pub fn scan(&mut self, b: u8) -> bool {
        use EraseState::*;
        match self.state {
            Normal => {
                if b == 0x1b {
                    self.state = Esc;
                }
                false
            }
            Esc => {
                self.state = if b == b'[' { CsiBracket } else { Normal };
                false
            }
            CsiBracket => match b {
                b'2' | b'3' => {
                    self.state = CsiParam(b);
                    false
                }
                0x1b => {
                    self.state = Esc;
                    false
                }
                _ => {
                    self.state = Normal;
                    false
                }
            },
            CsiParam(n) => {
                self.state = Normal;
                b == b'J' && (n == b'2' || n == b'3')
            }
        }
    }
}

/// OSC 133 semantic prompt markers emitted by the shell.
///
/// Protocol (FTCS / shell integration):
///   ESC ] 133 ; A ST  — prompt start
///   ESC ] 133 ; B ST  — command start (user pressed Enter)
///   ESC ] 133 ; C ST  — output start
///   ESC ] 133 ; D ; N ST — command end (N = exit code)
///
/// ST = BEL (\x07) or ESC \ (\x1b\x5c)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Osc133Marker {
    PromptStart,
    /// Carries the raw command text embedded in the OSC sequence (`B;<cmd>`).
    CommandStart(String),
    OutputStart,
    CommandEnd(i32),
}

enum ScanState {
    Normal,
    Esc,
    OscNum(u32),
    OscOther,
    OscOtherEsc,
    Osc133Body(Vec<u8>),
    Osc133St(Vec<u8>),
}

/// Byte-level scanner for OSC 133 sequences.
/// Feed every raw PTY byte through `scan()`; it returns `Some(marker)` when a
/// complete OSC 133 sequence is recognised.
pub struct Osc133Scanner {
    state: ScanState,
}

impl Osc133Scanner {
    pub fn new() -> Self {
        Self {
            state: ScanState::Normal,
        }
    }

    pub fn scan(&mut self, b: u8) -> Option<Osc133Marker> {
        use ScanState::*;
        let (next_state, result) = match &mut self.state {
            Normal => match b {
                0x1b => (Esc, None),
                _ => (Normal, None),
            },
            Esc => match b {
                b']' => (OscNum(0), None),
                _ => (Normal, None),
            },
            OscNum(n) => match b {
                b'0'..=b'9' => {
                    let new_n = n.saturating_mul(10).saturating_add((b - b'0') as u32);
                    (OscNum(new_n), None)
                }
                b';' => {
                    if *n == 133 {
                        (Osc133Body(Vec::new()), None)
                    } else {
                        (OscOther, None)
                    }
                }
                0x07 | 0x9c => (Normal, None), // empty OSC
                0x1b => (Esc, None),
                _ => (OscOther, None),
            },
            OscOther => match b {
                0x07 | 0x9c => (Normal, None),
                0x1b => (OscOtherEsc, None),
                _ => (OscOther, None),
            },
            OscOtherEsc => match b {
                b'\\' => (Normal, None),
                _ => (OscOther, None),
            },
            Osc133Body(buf) => match b {
                0x07 | 0x9c => {
                    let marker = parse_marker(buf);
                    (Normal, marker)
                }
                0x1b => {
                    let buf = std::mem::take(buf);
                    (Osc133St(buf), None)
                }
                _ => {
                    buf.push(b);
                    return None; // stay in same variant
                }
            },
            Osc133St(buf) => match b {
                b'\\' => {
                    let marker = parse_marker(buf);
                    (Normal, marker)
                }
                _ => {
                    // Not ST — the ESC was part of the body
                    let mut buf = std::mem::take(buf);
                    buf.push(0x1b);
                    buf.push(b);
                    (Osc133Body(buf), None)
                }
            },
        };
        self.state = next_state;
        result
    }
}

fn parse_marker(buf: &[u8]) -> Option<Osc133Marker> {
    match buf.first()? {
        b'A' => Some(Osc133Marker::PromptStart),
        b'B' => {
            // Optional embedded command: "B;<cmd text>"
            let cmd = if buf.len() > 2 && buf[1] == b';' {
                String::from_utf8_lossy(&buf[2..]).trim_end().to_string()
            } else {
                String::new()
            };
            Some(Osc133Marker::CommandStart(cmd))
        }
        b'C' => Some(Osc133Marker::OutputStart),
        b'D' => {
            // "D;exitcode"
            let exit_code = if buf.len() >= 3 && buf[1] == b';' {
                std::str::from_utf8(&buf[2..])
                    .ok()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0)
            } else {
                0
            };
            Some(Osc133Marker::CommandEnd(exit_code))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_all(bytes: &[u8]) -> Vec<Osc133Marker> {
        let mut scanner = Osc133Scanner::new();
        bytes.iter().filter_map(|&b| scanner.scan(b)).collect()
    }

    #[test]
    fn prompt_start_bel() {
        let seq = b"\x1b]133;A\x07";
        assert_eq!(scan_all(seq), vec![Osc133Marker::PromptStart]);
    }

    #[test]
    fn command_start_st() {
        let seq = b"\x1b]133;B\x1b\\";
        assert_eq!(
            scan_all(seq),
            vec![Osc133Marker::CommandStart(String::new())]
        );
    }

    #[test]
    fn command_start_with_cmd() {
        let seq = b"\x1b]133;B;ls -la\x07";
        assert_eq!(
            scan_all(seq),
            vec![Osc133Marker::CommandStart("ls -la".to_string())]
        );
    }

    #[test]
    fn command_start_cmd_with_semicolons() {
        let seq = b"\x1b]133;B;ls; echo done\x07";
        assert_eq!(
            scan_all(seq),
            vec![Osc133Marker::CommandStart("ls; echo done".to_string())]
        );
    }

    #[test]
    fn command_end_exit_code() {
        let seq = b"\x1b]133;D;42\x07";
        assert_eq!(scan_all(seq), vec![Osc133Marker::CommandEnd(42)]);
    }

    #[test]
    fn non_133_osc_ignored() {
        let seq = b"\x1b]0;My Title\x07\x1b]133;A\x07";
        assert_eq!(scan_all(seq), vec![Osc133Marker::PromptStart]);
    }

    #[test]
    fn noise_between_sequences() {
        let seq = b"some output\x1b]133;C\x07more output\x1b]133;D;0\x07";
        assert_eq!(
            scan_all(seq),
            vec![Osc133Marker::OutputStart, Osc133Marker::CommandEnd(0)]
        );
    }
}
