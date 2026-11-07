//! `petruterm msg [--no-reply] <name> <text...|->`: send a message to another agent by its
//! custom name. Prints one outcome line and exits with the outcome's code
//! (contracts/cli-msg.md). The caller's identity is never an argument: the app derives it from
//! the connection.

use super::{Message, MsgPayload};
use crate::term::messaging::Outcome;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

const USAGE: &str = "usage: petruterm msg [--no-reply] <name> <text...>\n       petruterm msg [--no-reply] <name> -   (text from stdin)";
/// Raw text above this is refused before sending; the app enforces the real limit after
/// sanitizing. Keeps the request line well under the socket's line cap.
const MAX_RAW_CHARS: usize = 12_000;

pub fn msg(args: &[String]) -> i32 {
    let mut no_reply = false;
    let mut rest: Vec<&String> = Vec::new();
    for a in args {
        match a.as_str() {
            "--no-reply" if rest.is_empty() => no_reply = true,
            _ => rest.push(a),
        }
    }
    let Some((name, text_args)) = rest.split_first() else {
        eprintln!("{USAGE}");
        return 2;
    };
    if text_args.is_empty() || name.starts_with("--") {
        eprintln!("{USAGE}");
        return 2;
    }
    let text = if text_args.len() == 1 && text_args[0] == "-" {
        let mut buf = String::new();
        let _ = std::io::stdin()
            .take(MAX_RAW_CHARS as u64 * 4)
            .read_to_string(&mut buf);
        buf
    } else {
        text_args
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let outcome = send(name, &text, no_reply);
    println!("{}", outcome.cli_line());
    outcome.exit_code()
}

fn send(name: &str, text: &str, no_reply: bool) -> Outcome {
    let (Some(pane), Some(sock)) = (
        std::env::var("PETRUTERM_PANE_ID")
            .ok()
            .and_then(|p| p.parse::<u64>().ok()),
        std::env::var_os("PETRUTERM_SOCKET"),
    ) else {
        return Outcome::SenderUnknown;
    };
    if text.chars().count() > MAX_RAW_CHARS {
        return Outcome::TooLong;
    }
    request(Path::new(&sock), pane, name, text, no_reply)
}

/// Send one request to the socket at `sock` and wait for the outcome.
pub fn request(sock: &Path, pane: u64, to: &str, text: &str, no_reply: bool) -> Outcome {
    let msg = Message {
        pane,
        msg: Some(MsgPayload {
            to: to.to_string(),
            text: text.to_string(),
            no_reply,
        }),
        ..Message::default()
    };
    let Ok(mut line) = serde_json::to_vec(&msg) else {
        return Outcome::Timeout;
    };
    line.push(b'\n');
    let Ok(mut stream) = UnixStream::connect(sock) else {
        return Outcome::Timeout;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    if stream.write_all(&line).is_err() {
        return Outcome::Timeout;
    }
    let mut reply = String::new();
    if BufReader::new(stream).read_line(&mut reply).is_err() {
        return Outcome::Timeout;
    }
    parse_reply(&reply)
}

/// Parse the app's reply line; anything unreadable is a timeout.
pub fn parse_reply(line: &str) -> Outcome {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
        return Outcome::Timeout;
    };
    Outcome::from_wire(
        v.get("result").and_then(|r| r.as_str()).unwrap_or(""),
        v.get("detail").and_then(|d| d.as_str()),
        v.get("retry_after_s").and_then(|r| r.as_u64()),
    )
}

/// The reply line for `outcome` (what the server writes back).
pub fn reply_line(outcome: &Outcome) -> String {
    let mut v = serde_json::json!({ "result": outcome.result_name() });
    if let Some(d) = outcome.detail() {
        v["detail"] = d.into();
    }
    if let Some(r) = outcome.retry_after_s() {
        v["retry_after_s"] = r.into();
    }
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_lines_round_trip() {
        for o in [
            Outcome::Delivered("kterm".into()),
            Outcome::RateLimited { retry_after_s: 9 },
            Outcome::NotAccepting("kterm".into()),
            Outcome::SenderUnknown,
            Outcome::Unsupported,
        ] {
            assert_eq!(parse_reply(&reply_line(&o)), o);
        }
        assert_eq!(parse_reply("garbage"), Outcome::Timeout);
        assert_eq!(parse_reply(""), Outcome::Timeout);
    }

    #[test]
    fn usage_errors_exit_two() {
        assert_eq!(msg(&[]), 2);
        assert_eq!(msg(&["kterm".to_string()]), 2);
        assert_eq!(msg(&["--no-reply".to_string()]), 2);
    }
}
