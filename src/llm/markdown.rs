#![allow(dead_code)]
use crate::llm::chat_panel::word_wrap;

#[derive(Debug, Clone)]
pub enum BlockKind {
    Normal,
    Heading(u8),
    CodeBlock { lang: String },
    ListItem { indent: u8, ordered: bool, number: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Keyword,
    StringLit,
    Comment,
    Number,
    Operator,
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpanKind {
    Bold,
    Italic,
    Code,
    Syntax(TokenKind),
}

#[derive(Debug, Clone)]
pub struct AnnotatedLine {
    pub display: String,
    pub kind: BlockKind,
    pub spans: Vec<(usize, usize, SpanKind)>,
}

#[derive(Debug, Clone, Default)]
pub struct ParseState {
    pub in_fence: bool,
    pub fence_lang: String,
}

pub fn parse_markdown(
    content: &str,
    width: usize,
    mut state: ParseState,
) -> (Vec<AnnotatedLine>, ParseState) {
    let mut out: Vec<AnnotatedLine> = Vec::new();

    for line in content.lines() {
        if state.in_fence {
            if line.starts_with("```") {
                state.in_fence = false;
                // fence-close line not displayed
            } else {
                let display: String = line.chars().take(width).collect();
                let spans = highlight_code(&state.fence_lang, &display);
                out.push(AnnotatedLine {
                    display,
                    kind: BlockKind::CodeBlock { lang: state.fence_lang.clone() },
                    spans,
                });
            }
            continue;
        }

        // fence open
        if line.starts_with("```") {
            let lang = line[3..].trim().to_string();
            state.in_fence = true;
            state.fence_lang = lang;
            continue;
        }

        // headings
        if let Some(rest) = line.strip_prefix("### ") {
            let (display, spans) = parse_inline(rest);
            out.push(AnnotatedLine { display, kind: BlockKind::Heading(3), spans });
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            let (display, spans) = parse_inline(rest);
            out.push(AnnotatedLine { display, kind: BlockKind::Heading(2), spans });
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            let (display, spans) = parse_inline(rest);
            out.push(AnnotatedLine { display, kind: BlockKind::Heading(1), spans });
            continue;
        }

        // unordered list — indented first
        if let Some(rest) = line.strip_prefix("  - ").or_else(|| line.strip_prefix("  * ")) {
            emit_list_item(&mut out, rest, 1, false, 0, width);
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            emit_list_item(&mut out, rest, 0, false, 0, width);
            continue;
        }

        // ordered list — indented first
        if let Some(rest) = line.strip_prefix("   ") {
            if let Some((n, body)) = parse_ordered_prefix(rest) {
                emit_list_item(&mut out, body, 1, true, n, width);
                continue;
            }
        }
        if let Some((n, body)) = parse_ordered_prefix(line) {
            emit_list_item(&mut out, body, 0, true, n, width);
            continue;
        }

        // empty line
        if line.trim().is_empty() {
            out.push(AnnotatedLine {
                display: String::new(),
                kind: BlockKind::Normal,
                spans: vec![],
            });
            continue;
        }

        // normal prose — word-wrap
        let wrapped = word_wrap(line, width);
        for sub in wrapped {
            let (display, spans) = parse_inline(&sub);
            out.push(AnnotatedLine { display, kind: BlockKind::Normal, spans });
        }
    }

    (out, state)
}

// Returns (number, body_str) if line starts with "<digits>. "
fn parse_ordered_prefix(line: &str) -> Option<(u32, &str)> {
    let end = line.find(". ")?;
    let num_str = &line[..end];
    if num_str.is_empty() || !num_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = num_str.parse().ok()?;
    Some((n, &line[end + 2..]))
}

fn emit_list_item(
    out: &mut Vec<AnnotatedLine>,
    body: &str,
    indent: u8,
    ordered: bool,
    number: u32,
    width: usize,
) {
    let (bullet, continuation) = if ordered {
        let prefix = format!("{}. ", number);
        let cont = " ".repeat(prefix.len());
        (prefix, cont)
    } else {
        let prefix = if indent == 0 { "• ".to_string() } else { "  • ".to_string() };
        let cont = " ".repeat(prefix.chars().count());
        (prefix, cont)
    };

    let bullet_chars = bullet.chars().count();
    let inner_width = width.saturating_sub(bullet_chars);
    let wrapped = if inner_width > 0 { word_wrap(body, inner_width) } else { vec![body.to_string()] };

    for (i, sub) in wrapped.into_iter().enumerate() {
        let raw_with_prefix = if i == 0 {
            format!("{}{}", bullet, sub)
        } else {
            format!("{}{}", continuation, sub)
        };
        let (display, spans) = parse_inline(&raw_with_prefix);
        out.push(AnnotatedLine {
            display,
            kind: BlockKind::ListItem { indent, ordered, number },
            spans,
        });
    }
}

fn parse_inline(raw: &str) -> (String, Vec<(usize, usize, SpanKind)>) {
    let chars: Vec<char> = raw.chars().collect();
    let mut display = String::new();
    let mut spans: Vec<(usize, usize, SpanKind)> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        // triple backtick inline (rare but handle it)
        if chars[i] == '`' {
            // count opening backticks
            let tick_count = chars[i..].iter().take_while(|&&c| c == '`').count();
            let close_tick: String = "`".repeat(tick_count);
            let after_open = i + tick_count;
            // search for closing sequence
            if let Some(rel) = find_close(&chars[after_open..], &close_tick) {
                let start_disp = display.chars().count();
                for &c in &chars[after_open..after_open + rel] {
                    display.push(c);
                }
                let end_disp = display.chars().count();
                if end_disp > start_disp {
                    spans.push((start_disp, end_disp, SpanKind::Code));
                }
                i = after_open + rel + tick_count;
                continue;
            } else {
                // no close — emit literally
                for &c in &chars[i..i + tick_count] {
                    display.push(c);
                }
                i += tick_count;
                continue;
            }
        }

        // bold **...**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            let after_open = i + 2;
            if let Some(rel) = find_close(&chars[after_open..], "**") {
                let start_disp = display.chars().count();
                for &c in &chars[after_open..after_open + rel] {
                    display.push(c);
                }
                let end_disp = display.chars().count();
                if end_disp > start_disp {
                    spans.push((start_disp, end_disp, SpanKind::Bold));
                }
                i = after_open + rel + 2;
                continue;
            }
        }

        // italic *...*
        if chars[i] == '*' {
            let after_open = i + 1;
            if let Some(rel) = find_close(&chars[after_open..], "*") {
                let start_disp = display.chars().count();
                for &c in &chars[after_open..after_open + rel] {
                    display.push(c);
                }
                let end_disp = display.chars().count();
                if end_disp > start_disp {
                    spans.push((start_disp, end_disp, SpanKind::Italic));
                }
                i = after_open + rel + 1;
                continue;
            }
        }

        display.push(chars[i]);
        i += 1;
    }

    (display, spans)
}

// Find the first occurrence of `needle` (as chars) in `haystack`, returning the index before it.
fn find_close(haystack: &[char], needle: &str) -> Option<usize> {
    let needle_chars: Vec<char> = needle.chars().collect();
    let nlen = needle_chars.len();
    if nlen == 0 {
        return None;
    }
    for i in 0..haystack.len() {
        if i + nlen > haystack.len() {
            break;
        }
        if &haystack[i..i + nlen] == needle_chars.as_slice() {
            return Some(i);
        }
    }
    None
}

pub fn highlight_code(lang: &str, line: &str) -> Vec<(usize, usize, SpanKind)> {
    let lang = match lang {
        "rust" => "rs",
        "python" => "py",
        "javascript" => "js",
        "typescript" => "ts",
        "bash" | "shell" | "zsh" | "sh" => "sh",
        other => other,
    };
    let keywords: &[&str] = match lang {
        "rs" => &[
            "fn", "let", "mut", "pub", "use", "struct", "enum", "impl", "trait", "type",
            "where", "if", "else", "match", "return", "for", "while", "loop", "break",
            "continue", "async", "await", "move", "unsafe", "extern", "crate", "mod",
            "self", "super", "true", "false",
        ],
        "py" => &[
            "def", "class", "if", "elif", "else", "for", "while", "return", "import",
            "from", "as", "with", "try", "except", "finally", "raise", "lambda", "yield",
            "pass", "break", "continue", "True", "False", "None", "and", "or", "not",
            "in", "is",
        ],
        "js" | "ts" => &[
            "function", "const", "let", "var", "return", "if", "else", "for", "while",
            "class", "import", "export", "default", "async", "await", "new", "this",
            "typeof", "instanceof", "void", "null", "undefined", "true", "false",
            // ts extras
            "interface", "type", "enum", "readonly", "namespace", "declare", "abstract",
        ],
        "sh" => &[
            "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case",
            "esac", "function", "export", "local", "echo", "return", "cd",
        ],
        "json" => &["true", "false", "null"],
        _ => &[],
    };

    let line_comment = match lang {
        "rs" | "js" | "ts" | "json" => Some("//"),
        "py" | "sh" => Some("#"),
        _ => None,
    };

    let operator_chars: &[char] = &[
        '{', '}', '(', ')', '[', ']', ',', ';', ':', '=', '<', '>', '+', '-', '*', '/',
        '!', '&', '|', '^', '~', '%', '.',
    ];

    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut spans: Vec<(usize, usize, SpanKind)> = Vec::new();
    let mut i = 0;

    while i < len {
        // line comment — consume rest of line
        if let Some(lc) = line_comment {
            let lc_chars: Vec<char> = lc.chars().collect();
            let lclen = lc_chars.len();
            if i + lclen <= len && &chars[i..i + lclen] == lc_chars.as_slice() {
                spans.push((i, len, SpanKind::Syntax(TokenKind::Comment)));
                break;
            }
        }

        // string literals
        if chars[i] == '"' || chars[i] == '\'' {
            let delim = chars[i];
            let start = i;
            i += 1;
            while i < len {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                } else if chars[i] == delim {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            spans.push((start, i, SpanKind::Syntax(TokenKind::StringLit)));
            continue;
        }

        // hex number
        if chars[i] == '0' && i + 1 < len && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
            let start = i;
            i += 2;
            while i < len && chars[i].is_ascii_hexdigit() {
                i += 1;
            }
            spans.push((start, i, SpanKind::Syntax(TokenKind::Number)));
            continue;
        }

        // decimal number
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < len && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            spans.push((start, i, SpanKind::Syntax(TokenKind::Number)));
            continue;
        }

        // identifier / keyword
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if keywords.contains(&word.as_str()) {
                spans.push((start, i, SpanKind::Syntax(TokenKind::Keyword)));
            }
            continue;
        }

        // operators
        if operator_chars.contains(&chars[i]) {
            spans.push((i, i + 1, SpanKind::Syntax(TokenKind::Operator)));
            i += 1;
            continue;
        }

        i += 1;
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Vec<AnnotatedLine> {
        parse_markdown(s, 80, ParseState::default()).0
    }

    #[test]
    fn headings() {
        let lines = parse("# H1\n## H2\n### H3");
        assert!(matches!(lines[0].kind, BlockKind::Heading(1)));
        assert_eq!(lines[0].display, "H1");
        assert!(matches!(lines[1].kind, BlockKind::Heading(2)));
        assert!(matches!(lines[2].kind, BlockKind::Heading(3)));
    }

    #[test]
    fn inline_bold_italic_code() {
        let lines = parse("Hello **world** and *hi* and `code`");
        assert_eq!(lines[0].display, "Hello world and hi and code");
        let kinds: Vec<&SpanKind> = lines[0].spans.iter().map(|(_, _, k)| k).collect();
        assert!(kinds.contains(&&SpanKind::Bold));
        assert!(kinds.contains(&&SpanKind::Italic));
        assert!(kinds.contains(&&SpanKind::Code));
    }

    #[test]
    fn code_fence_syntax_highlight() {
        let md = "```rust\nfn main() {\n    let x = 42;\n}\n```";
        let lines = parse(md);
        assert!(lines.iter().all(|l| matches!(l.kind, BlockKind::CodeBlock { .. })));
        // "fn" and "let" should produce Keyword spans
        let kw_line = lines.iter().find(|l| l.display.contains("fn")).unwrap();
        assert!(kw_line.spans.iter().any(|(_, _, k)| matches!(k, SpanKind::Syntax(TokenKind::Keyword))));
        // 42 should produce a Number span
        let num_line = lines.iter().find(|l| l.display.contains("42")).unwrap();
        assert!(num_line.spans.iter().any(|(_, _, k)| matches!(k, SpanKind::Syntax(TokenKind::Number))));
    }

    #[test]
    fn unordered_list() {
        let lines = parse("- foo\n- bar");
        assert!(lines[0].display.contains('•'));
        assert!(matches!(lines[0].kind, BlockKind::ListItem { .. }));
    }

    #[test]
    fn fence_state_carries_across_calls() {
        // Simulate streaming: first call opens fence, second closes it
        let (lines1, state1) = parse_markdown("```rs\nfn foo()", 80, ParseState::default());
        assert!(state1.in_fence);
        assert!(lines1.iter().all(|l| matches!(l.kind, BlockKind::CodeBlock { .. })));
        let (lines2, state2) = parse_markdown("{\n}\n```", 80, state1);
        assert!(!state2.in_fence);
        assert!(lines2.iter().any(|l| l.display.contains('}')));
    }

    #[test]
    fn wrap_respects_width() {
        let long = "word ".repeat(30);
        let lines = parse(&long);
        assert!(lines.iter().all(|l| l.display.chars().count() <= 80));
    }
}
