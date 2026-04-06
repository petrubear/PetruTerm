use std::path::Path;
use serde_json::Value;

/// Tool definitions available to the LLM agent.
#[derive(Debug, Clone)]
pub enum AgentTool {
    ReadFile,
    ListDir,
}

impl AgentTool {
    pub fn name(&self) -> &'static str {
        match self {
            AgentTool::ReadFile => "read_file",
            AgentTool::ListDir  => "list_dir",
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
        }
    }

    /// All available tools.
    pub fn all() -> Vec<AgentTool> {
        vec![AgentTool::ReadFile, AgentTool::ListDir]
    }

    /// Spec array ready to include in the API request.
    pub fn all_specs() -> Vec<Value> {
        Self::all().iter().map(|t| t.to_openai_spec()).collect()
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
    /// Extract the `path` argument, if present.
    pub fn path_arg(&self) -> Option<String> {
        serde_json::from_str::<Value>(&self.arguments)
            .ok()
            .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(String::from))
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
            let Some(rel) = call.path_arg() else {
                return "Error: missing 'path' argument".to_string();
            };
            let abs = cwd.join(&rel);
            match abs.canonicalize() {
                Ok(canon) if canon.starts_with(cwd) => {
                    std::fs::read_to_string(&canon)
                        .unwrap_or_else(|e| format!("Error reading file: {e}"))
                }
                _ => format!("Error: path '{rel}' is outside the working directory"),
            }
        }
        "list_dir" => {
            let rel = call.path_arg().unwrap_or_else(|| ".".to_string());
            let abs = cwd.join(&rel);
            match abs.canonicalize() {
                Ok(canon) if canon.starts_with(cwd) => {
                    match std::fs::read_dir(&canon) {
                        Ok(entries) => {
                            let mut lines: Vec<String> = entries
                                .flatten()
                                .map(|e| {
                                    let name = e.file_name().to_string_lossy().to_string();
                                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                                    if is_dir { format!("{name}/") } else { name }
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
                    }
                }
                _ => format!("Error: path '{rel}' is outside the working directory"),
            }
        }
        other => format!("Error: unknown tool '{other}'"),
    }
}
