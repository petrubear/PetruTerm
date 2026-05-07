use serde::Deserialize;

/// A structured action the LLM can propose inside its text response.
///
/// Emitted as `<action>{...json...}</action>` in the LLM's reply.
/// The user must confirm before execution (A-2/A-3).
#[derive(Debug, Clone, PartialEq)]
pub enum AgentAction {
    RunCommand { cmd: String, explanation: String },
    OpenFile { path: String },
    ExplainOutput { last_n_lines: usize },
}

/// Append to the system prompt so the LLM knows the action format.
pub fn system_prompt_instructions() -> &'static str {
    "When proposing an action, embed exactly one <action> tag with a JSON payload:\n\
     - Run command:    <action>{\"type\":\"run_command\",\"cmd\":\"<cmd>\",\"explanation\":\"<why>\"}</action>\n\
     - Open file:      <action>{\"type\":\"open_file\",\"path\":\"<path>\"}</action>\n\
     - Explain output: <action>{\"type\":\"explain_output\",\"last_n_lines\":<n>}</action>\n\
     The user will confirm before any action is executed."
}

#[derive(Deserialize)]
struct ActionPayload {
    #[serde(rename = "type")]
    kind: String,
    cmd: Option<String>,
    explanation: Option<String>,
    path: Option<String>,
    last_n_lines: Option<usize>,
}

/// Scan `text` for the first `<action>...</action>` block and parse its JSON payload.
/// Returns `None` if no valid action is found.
pub fn parse_action_from_response(text: &str) -> Option<AgentAction> {
    let start = text.find("<action>")?;
    let content_start = start + "<action>".len();
    let end = text[content_start..].find("</action>")?;
    let json = text[content_start..content_start + end].trim();

    let payload: ActionPayload = serde_json::from_str(json).ok()?;
    match payload.kind.as_str() {
        "run_command" => Some(AgentAction::RunCommand {
            cmd: payload.cmd?,
            explanation: payload.explanation.unwrap_or_default(),
        }),
        "open_file" => Some(AgentAction::OpenFile {
            path: payload.path?,
        }),
        "explain_output" => Some(AgentAction::ExplainOutput {
            last_n_lines: payload.last_n_lines.unwrap_or(20),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_run_command() {
        let text = "Let me build it.\n\
            <action>{\"type\":\"run_command\",\"cmd\":\"cargo build\",\"explanation\":\"Build the project\"}</action>\n\
            Done.";
        assert_eq!(
            parse_action_from_response(text).unwrap(),
            AgentAction::RunCommand {
                cmd: "cargo build".into(),
                explanation: "Build the project".into(),
            }
        );
    }

    #[test]
    fn parses_open_file() {
        let text = "Here: <action>{\"type\":\"open_file\",\"path\":\"src/main.rs\"}</action>";
        assert_eq!(
            parse_action_from_response(text).unwrap(),
            AgentAction::OpenFile {
                path: "src/main.rs".into()
            }
        );
    }

    #[test]
    fn parses_explain_output() {
        let text = "<action>{\"type\":\"explain_output\",\"last_n_lines\":30}</action>";
        assert_eq!(
            parse_action_from_response(text).unwrap(),
            AgentAction::ExplainOutput { last_n_lines: 30 }
        );
    }

    #[test]
    fn explain_output_defaults_last_n_lines() {
        let text = "<action>{\"type\":\"explain_output\"}</action>";
        assert_eq!(
            parse_action_from_response(text).unwrap(),
            AgentAction::ExplainOutput { last_n_lines: 20 }
        );
    }

    #[test]
    fn run_command_with_no_explanation_uses_empty_string() {
        let text = "<action>{\"type\":\"run_command\",\"cmd\":\"ls\"}</action>";
        assert_eq!(
            parse_action_from_response(text).unwrap(),
            AgentAction::RunCommand {
                cmd: "ls".into(),
                explanation: String::new(),
            }
        );
    }

    #[test]
    fn returns_first_action_when_multiple_present() {
        let text = "<action>{\"type\":\"open_file\",\"path\":\"a.rs\"}</action> \
                    <action>{\"type\":\"open_file\",\"path\":\"b.rs\"}</action>";
        assert_eq!(
            parse_action_from_response(text).unwrap(),
            AgentAction::OpenFile {
                path: "a.rs".into()
            }
        );
    }

    #[test]
    fn returns_none_when_no_action_tag() {
        assert!(parse_action_from_response("plain text response").is_none());
    }

    #[test]
    fn returns_none_for_unknown_type() {
        let text = "<action>{\"type\":\"teleport\"}</action>";
        assert!(parse_action_from_response(text).is_none());
    }

    #[test]
    fn returns_none_for_malformed_json() {
        let text = "<action>not json at all</action>";
        assert!(parse_action_from_response(text).is_none());
    }

    #[test]
    fn returns_none_for_run_command_missing_cmd() {
        let text = "<action>{\"type\":\"run_command\",\"explanation\":\"oops\"}</action>";
        assert!(parse_action_from_response(text).is_none());
    }

    #[test]
    fn returns_none_for_open_file_missing_path() {
        let text = "<action>{\"type\":\"open_file\"}</action>";
        assert!(parse_action_from_response(text).is_none());
    }

    #[test]
    fn handles_whitespace_around_json() {
        let text = "<action>  \n{\"type\":\"open_file\",\"path\":\"foo.rs\"}\n  </action>";
        assert_eq!(
            parse_action_from_response(text).unwrap(),
            AgentAction::OpenFile {
                path: "foo.rs".into()
            }
        );
    }
}
