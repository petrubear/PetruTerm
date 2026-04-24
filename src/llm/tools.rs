use serde_json::Value;
use std::path::Path;

/// Tool definitions available to the LLM agent.
#[derive(Debug, Clone)]
pub enum AgentTool {
    ReadFile,
    ListDir,
    WriteFile,
    RunCommand,
}

impl AgentTool {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            AgentTool::ReadFile => "read_file",
            AgentTool::ListDir => "list_dir",
            AgentTool::WriteFile => "write_file",
            AgentTool::RunCommand => "run_command",
        }
    }

    /// Serialize to OpenAI function-calling spec.
    pub fn to_openai_spec(&self) -> Value {
        match self {
            AgentTool::ReadFile => serde_json::json!({
                "type": "function",
                "function": {
                    "name": "read_file",
                    "description": "Read the full contents of a file within the working directory.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Relative path to the file from the working directory."
                            }
                        },
                        "required": ["path"]
                    }
                }
            }),
            AgentTool::ListDir => serde_json::json!({
                "type": "function",
                "function": {
                    "name": "list_dir",
                    "description": "List files and subdirectories at the given path within the working directory.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Relative path from the working directory. Use '.' for CWD itself."
                            }
                        },
                        "required": ["path"]
                    }
                }
            }),
            AgentTool::WriteFile => serde_json::json!({
                "type": "function",
                "function": {
                    "name": "write_file",
                    "description": "Overwrite a file with new content. Always shows a diff preview and asks the user to confirm before writing.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Relative path to the file from the working directory."
                            },
                            "content": {
                                "type": "string",
                                "description": "Complete new content for the file."
                            }
                        },
                        "required": ["path", "content"]
                    }
                }
            }),
            AgentTool::RunCommand => serde_json::json!({
                "type": "function",
                "function": {
                    "name": "run_command",
                    "description": "Run a shell command in the active terminal. Always asks the user to confirm before executing.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "cmd": {
                                "type": "string",
                                "description": "The shell command to execute."
                            }
                        },
                        "required": ["cmd"]
                    }
                }
            }),
        }
    }

    /// All available tools.
    pub fn all() -> Vec<AgentTool> {
        vec![
            AgentTool::ReadFile,
            AgentTool::ListDir,
            AgentTool::WriteFile,
            AgentTool::RunCommand,
        ]
    }

    /// Spec array for built-in tools, excluding any whose functionality is already
    /// covered by `mcp_tool_names`.
    ///
    /// Rules:
    /// - A built-in is excluded when its exact name appears in `mcp_tool_names`.
    /// - `list_dir` is additionally excluded when any MCP tool name contains "list"
    ///   or "director" (e.g. `list_directory` from the filesystem server).
    ///   This prevents the LLM from picking the workspace-restricted built-in when
    ///   a more capable MCP equivalent exists.
    pub fn specs_excluding(mcp_tool_names: &[String]) -> Vec<Value> {
        let has_list_overlap = mcp_tool_names
            .iter()
            .any(|n| n.contains("list") || n.contains("director"));

        Self::all()
            .into_iter()
            .filter(|t| {
                let n = t.name();
                if mcp_tool_names.iter().any(|m| m == n) {
                    return false; // exact name match — MCP overrides
                }
                if n == "list_dir" && has_list_overlap {
                    return false; // semantic overlap with MCP directory listing
                }
                true
            })
            .map(|t| t.to_openai_spec())
            .collect()
    }

    /// Spec array ready to include in the API request (all built-ins, no filtering).
    #[allow(dead_code)]
    pub fn all_specs() -> Vec<Value> {
        Self::all().iter().map(|t| t.to_openai_spec()).collect()
    }

    /// Returns true if `name` is a built-in tool handled by `execute_tool`.
    /// Any other name is assumed to be an MCP tool.
    pub fn is_builtin(name: &str) -> bool {
        matches!(
            name,
            "read_file" | "list_dir" | "write_file" | "run_command"
        )
    }
}

/// A single tool call requested by the LLM.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON string of arguments, e.g. `{"path":"src/main.rs"}`.
    pub arguments: String,
}

impl ToolCall {
    fn args_json(&self) -> Option<Value> {
        serde_json::from_str(&self.arguments).ok()
    }

    /// Extract the `path` argument, if present.
    pub fn path_arg(&self) -> Option<String> {
        self.args_json()
            .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(String::from))
    }

    /// Extract the `content` argument (for `write_file`), if present.
    pub fn content_arg(&self) -> Option<String> {
        self.args_json()
            .and_then(|v| v.get("content").and_then(|p| p.as_str()).map(String::from))
    }

    /// Extract the `cmd` argument (for `run_command`), if present.
    pub fn cmd_arg(&self) -> Option<String> {
        self.args_json()
            .and_then(|v| v.get("cmd").and_then(|p| p.as_str()).map(String::from))
    }

    /// Returns true if this tool call requires user confirmation before execution.
    pub fn requires_confirmation(&self) -> bool {
        matches!(self.name.as_str(), "write_file" | "run_command")
    }
}

/// Result from one `agent_step` call.
#[derive(Debug)]
pub enum AgentStepResult {
    /// The LLM produced a text response — no further tool calls needed.
    Text(String),
    /// The LLM wants to invoke tools before continuing.
    ToolCalls {
        /// The raw assistant message JSON (with `tool_calls`) to append to history.
        assistant_msg: Value,
        calls: Vec<ToolCall>,
    },
}

/// Execute a single tool call, restricting filesystem access to within `cwd`.
///
/// Returns the result string to feed back to the LLM as a tool message.
pub fn execute_tool(call: &ToolCall, cwd: &Path) -> String {
    match call.name.as_str() {
        "read_file" => {
            const MAX_CHARS: usize = 50_000;
            const MAX_BYTES: u64 = 200 * 1024; // generous guard before char-truncation
            let Some(rel) = call.path_arg() else {
                return "Error: missing 'path' argument".to_string();
            };
            let abs = cwd.join(&rel);
            match abs.canonicalize() {
                Ok(canon) if canon.starts_with(cwd) => {
                    let file_size = std::fs::metadata(&canon).map(|m| m.len()).unwrap_or(0);
                    let text = if file_size > MAX_BYTES {
                        use std::io::Read;
                        let mut buf = vec![0u8; MAX_BYTES as usize];
                        match std::fs::File::open(&canon).and_then(|mut f| f.read(&mut buf)) {
                            Ok(n) => String::from_utf8_lossy(&buf[..n]).into_owned(),
                            Err(e) => return format!("Error reading file: {e}"),
                        }
                    } else {
                        match std::fs::read_to_string(&canon) {
                            Ok(s) => s,
                            Err(e) => return format!("Error reading file: {e}"),
                        }
                    };
                    if text.chars().count() > MAX_CHARS {
                        let truncated: String = text.chars().take(MAX_CHARS).collect();
                        format!("{truncated}\n[truncated at 50 000 chars]")
                    } else {
                        text
                    }
                }
                _ => format!("Error: path '{rel}' is outside the working directory"),
            }
        }
        "list_dir" => {
            let rel = call.path_arg().unwrap_or_else(|| ".".to_string());
            let abs = cwd.join(&rel);
            match abs.canonicalize() {
                Ok(canon) if canon.starts_with(cwd) => match std::fs::read_dir(&canon) {
                    Ok(entries) => {
                        let mut lines: Vec<String> = entries
                            .flatten()
                            .map(|e| {
                                let name = e.file_name().to_string_lossy().to_string();
                                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                                if is_dir {
                                    format!("{name}/")
                                } else {
                                    name
                                }
                            })
                            .collect();
                        lines.sort();
                        if lines.is_empty() {
                            "(empty directory)".to_string()
                        } else {
                            lines.join("\n")
                        }
                    }
                    Err(e) => format!("Error listing directory: {e}"),
                },
                _ => format!("Error: path '{rel}' is outside the working directory"),
            }
        }
        // write_file and run_command are handled by the agent loop (require confirmation).
        "write_file" | "run_command" => {
            "Error: this tool must be handled by the agent loop, not execute_tool.".to_string()
        }
        other => format!("Error: unknown tool '{other}'"),
    }
}
