/// Desktop-notification OSC sequences emitted by terminal programs (agents, scripts):
///   ESC ] 9 ; <text> ST                       — iTerm2 / ConEmu style
///   ESC ] 777 ; notify ; <title> ; <body> ST  — urxvt / foot style
///   ESC ] 99 ; <metadata> ; <payload> ST      — kitty style (single-chunk form only)
///
/// ST = BEL (\x07) or ESC \ (\x1b\x5c). `0x9c` is deliberately NOT a terminator: it is a
/// UTF-8 continuation byte (e.g. U+2705 is E2 9C 85), so treating it as ST would truncate
/// notification text that contains emoji.
///
/// The text comes from the PTY, so it is untrusted; [`sanitize`] strips control characters
/// and caps the length before it reaches any UI.

/// Max bytes of body kept while scanning. Anything beyond is discarded.
const BODY_CAP: usize = 4096;
/// Max chars of a sanitized title/body.
pub const MAX_TEXT_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscNotification {
    pub title: Option<String>,
    pub body: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    Esc,
    Num,
    Body,
    BodyEsc,
    Skip,
    SkipEsc,
}

pub struct OscNotifyScanner {
    state: State,
    num: u32,
    buf: Vec<u8>,
}

impl OscNotifyScanner {
    pub fn new() -> Self {
        Self {
            state: State::Normal,
            num: 0,
            buf: Vec::new(),
        }
    }

    /// Feed one raw PTY byte. Returns a notification when a complete, recognised sequence ends.
    pub fn scan(&mut self, b: u8) -> Option<OscNotification> {
        use State::*;
        match self.state {
            Normal => {
                if b == 0x1b {
                    self.state = Esc;
                }
            }
            Esc => match b {
                b']' => {
                    self.num = 0;
                    self.state = Num;
                }
                0x1b => {}
                _ => self.state = Normal,
            },
            Num => match b {
                b'0'..=b'9' => {
                    self.num = self
                        .num
                        .saturating_mul(10)
                        .saturating_add((b - b'0') as u32);
                }
                b';' => {
                    if matches!(self.num, 9 | 99 | 777) {
                        self.buf.clear();
                        self.state = Body;
                    } else {
                        self.state = Skip;
                    }
                }
                0x07 => self.state = Normal,
                0x1b => self.state = Esc,
                _ => self.state = Skip,
            },
            Skip => match b {
                0x07 => self.state = Normal,
                0x1b => self.state = SkipEsc,
                _ => {}
            },
            SkipEsc => match b {
                b'\\' => self.state = Normal,
                b']' => {
                    self.num = 0;
                    self.state = Num;
                }
                _ => self.state = Skip,
            },
            Body => match b {
                0x07 => return self.finish(),
                0x1b => self.state = BodyEsc,
                _ => {
                    if self.buf.len() < BODY_CAP {
                        self.buf.push(b);
                    }
                }
            },
            BodyEsc => match b {
                b'\\' => return self.finish(),
                b']' => {
                    // A new OSC starts: the previous one was never terminated, drop it.
                    self.num = 0;
                    self.state = Num;
                }
                _ => self.state = Normal,
            },
        }
        None
    }

    fn finish(&mut self) -> Option<OscNotification> {
        self.state = State::Normal;
        let result = parse(self.num, &self.buf);
        self.buf.clear();
        result
    }
}

impl Default for OscNotifyScanner {
    fn default() -> Self {
        Self::new()
    }
}

fn parse(num: u32, buf: &[u8]) -> Option<OscNotification> {
    let text = String::from_utf8_lossy(buf);
    match num {
        9 => parse_osc9(&text),
        777 => parse_osc777(&text),
        99 => parse_osc99(&text),
        _ => None,
    }
}

fn parse_osc9(text: &str) -> Option<OscNotification> {
    // ConEmu control forms (`9;4;1;50` progress, `9;1;ms` sleep, ...) are digits followed by
    // `;`. Agents emit progress updates continuously; they are not notifications.
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && text.as_bytes().get(digits) == Some(&b';') {
        return None;
    }
    let body = sanitize(text);
    (!body.is_empty()).then_some(OscNotification { title: None, body })
}

fn parse_osc777(text: &str) -> Option<OscNotification> {
    // `notify;<title>;<body>` -- split at most twice so `;` inside the body survives.
    let mut parts = text.splitn(3, ';');
    if parts.next()? != "notify" {
        return None;
    }
    let title = sanitize(parts.next().unwrap_or(""));
    let body = sanitize(parts.next().unwrap_or(""));
    match (title.is_empty(), body.is_empty()) {
        (true, true) => None,
        (false, true) => Some(OscNotification {
            title: None,
            body: title,
        }),
        (true, false) => Some(OscNotification { title: None, body }),
        (false, false) => Some(OscNotification {
            title: Some(title),
            body,
        }),
    }
}

fn parse_osc99(text: &str) -> Option<OscNotification> {
    // `<metadata>;<payload>`; metadata is `:`-separated `key=value` pairs.
    let (meta, payload) = text.split_once(';')?;
    let mut kind = "title"; // kitty's default payload type
    for pair in meta.split(':') {
        match pair.split_once('=') {
            // More chunks follow: multi-part notifications are not supported yet.
            Some(("d", "0")) => return None,
            // Base64-encoded payload: not supported yet.
            Some(("e", "1")) => return None,
            Some(("p", p)) => kind = p,
            _ => {}
        }
    }
    if kind != "title" && kind != "body" {
        return None; // close / alive / icon / query / ...
    }
    let body = sanitize(payload);
    (!body.is_empty()).then_some(OscNotification { title: None, body })
}

/// Strip control characters and cap the length. Safe to show in any UI.
pub fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_all(bytes: &[u8]) -> Vec<OscNotification> {
        let mut scanner = OscNotifyScanner::new();
        bytes.iter().filter_map(|&b| scanner.scan(b)).collect()
    }

    fn body(s: &str) -> OscNotification {
        OscNotification {
            title: None,
            body: s.to_string(),
        }
    }

    #[test]
    fn osc9_bel_and_st() {
        assert_eq!(scan_all(b"\x1b]9;hello\x07"), vec![body("hello")]);
        assert_eq!(scan_all(b"\x1b]9;hello\x1b\\"), vec![body("hello")]);
    }

    #[test]
    fn osc9_emoji_is_not_truncated() {
        // U+2705 = E2 9C 85; 0x9C must not be treated as ST.
        assert_eq!(
            scan_all("\x1b]9;listo ✅ ya\x07".as_bytes()),
            vec![body("listo ✅ ya")]
        );
    }

    #[test]
    fn osc9_conemu_control_forms_ignored() {
        assert!(scan_all(b"\x1b]9;4;1;50\x07").is_empty());
        assert!(scan_all(b"\x1b]9;4;0\x07").is_empty());
        assert!(scan_all(b"\x1b]9;1;500\x07").is_empty());
    }

    #[test]
    fn osc9_plain_number_text_is_a_notification() {
        assert_eq!(scan_all(b"\x1b]9;42\x07"), vec![body("42")]);
    }

    #[test]
    fn osc9_empty_body_ignored() {
        assert!(scan_all(b"\x1b]9;\x07").is_empty());
    }

    #[test]
    fn osc777_title_and_body_with_semicolons() {
        let got = scan_all(b"\x1b]777;notify;Build;done; 3 errors; 2 warnings\x07");
        assert_eq!(
            got,
            vec![OscNotification {
                title: Some("Build".into()),
                body: "done; 3 errors; 2 warnings".into(),
            }]
        );
    }

    #[test]
    fn osc777_title_only_becomes_body() {
        assert_eq!(scan_all(b"\x1b]777;notify;Ping\x07"), vec![body("Ping")]);
    }

    #[test]
    fn osc777_non_notify_ignored() {
        assert!(scan_all(b"\x1b]777;precmd\x07").is_empty());
    }

    #[test]
    fn osc99_default_and_body_payloads() {
        assert_eq!(scan_all(b"\x1b]99;;Hello\x1b\\"), vec![body("Hello")]);
        assert_eq!(
            scan_all(b"\x1b]99;i=1:p=body;Details\x1b\\"),
            vec![body("Details")]
        );
    }

    #[test]
    fn osc99_unsupported_forms_ignored() {
        assert!(scan_all(b"\x1b]99;i=1:d=0;part\x1b\\").is_empty());
        assert!(scan_all(b"\x1b]99;e=1;SGVsbG8=\x1b\\").is_empty());
        assert!(scan_all(b"\x1b]99;i=1:p=close;\x1b\\").is_empty());
    }

    #[test]
    fn other_osc_ignored_and_does_not_break_following_ones() {
        let got = scan_all(b"\x1b]0;Title\x07\x1b]133;A\x07\x1b]9;after\x07");
        assert_eq!(got, vec![body("after")]);
    }

    #[test]
    fn oversized_body_is_capped() {
        let mut seq = b"\x1b]9;".to_vec();
        seq.extend(std::iter::repeat(b'a').take(100_000));
        seq.push(0x07);
        let got = scan_all(&seq);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn sequence_split_across_reads() {
        let mut scanner = OscNotifyScanner::new();
        let seq = "\x1b]9;split ✅ text\x07".as_bytes();
        let (a, b) = seq.split_at(9); // split mid-emoji
        let mut got: Vec<_> = a.iter().filter_map(|&x| scanner.scan(x)).collect();
        got.extend(b.iter().filter_map(|&x| scanner.scan(x)));
        assert_eq!(got, vec![body("split ✅ text")]);
    }

    #[test]
    fn control_chars_are_stripped() {
        assert_eq!(scan_all(b"\x1b]9;a\x01b\x7fc\x07"), vec![body("abc")]);
    }

    #[test]
    fn unterminated_osc_then_new_osc_recovers() {
        assert_eq!(
            scan_all(b"\x1b]9;lost\x1b]9;found\x07"),
            vec![body("found")]
        );
    }

    #[test]
    fn noise_between_sequences() {
        let got = scan_all(b"output\x1b]9;one\x07more\x1b]9;two\x07");
        assert_eq!(got, vec![body("one"), body("two")]);
    }
}
