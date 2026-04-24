use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::config::McpServerConfig;

// ── Public types ──────────────────────────────────────────────────────────────

/// A tool exposed by an MCP server.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the tool's input (type, properties, required).
    pub input_schema: Value,
}

impl McpTool {
    /// Serialize to the OpenAI function-calling spec format expected by LLM providers.
    pub fn to_openai_spec(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.input_schema,
            }
        })
    }
}

// ── Internal IO message types ─────────────────────────────────────────────────

enum OutboundMsg {
    /// JSON-RPC request — expects a response matched by `id`.
    Request {
        id: u64,
        method: String,
        params: Value,
        tx: oneshot::Sender<Result<Value>>,
    },
    /// JSON-RPC notification — no `id`, no response expected.
    Notification { method: String, params: Value },
}

// ── McpClient ─────────────────────────────────────────────────────────────────

/// Client for a single MCP server process.
///
/// Owns the background IO task that drives stdin/stdout communication.
/// Drop triggers `kill_on_drop` on the child process.
pub struct McpClient {
    pub name: String,
    pub tools: Vec<McpTool>,
    msg_tx: mpsc::Sender<OutboundMsg>,
    _task: JoinHandle<()>,
}

impl McpClient {
    /// Spawn the server, perform the MCP initialize handshake, fetch tools.
    pub async fn connect(name: String, cfg: McpServerConfig) -> Result<Self> {
        let (msg_tx, msg_rx) = mpsc::channel::<OutboundMsg>(32);

        let mut child = tokio::process::Command::new(&cfg.command)
            .args(&cfg.args)
            .envs(&cfg.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server '{name}' ({})", cfg.command))?;

        let stdin = child.stdin.take().context("MCP stdin unavailable")?;
        let stdout = child.stdout.take().context("MCP stdout unavailable")?;

        let task = tokio::spawn(run_io_loop(stdin, stdout, msg_rx));

        let mut client = Self {
            name: name.clone(),
            tools: vec![],
            msg_tx,
            _task: task,
        };

        client
            .initialize()
            .await
            .with_context(|| format!("MCP initialize failed for '{name}'"))?;

        client.tools = client
            .fetch_tools()
            .await
            .with_context(|| format!("tools/list failed for '{name}'"))?;

        Ok(client)
    }

    /// Call a tool on this server and return the result as text.
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<String> {
        let result = self
            .request(
                "tools/call",
                json!({ "name": tool_name, "arguments": arguments }),
            )
            .await?;
        extract_tool_result_text(&result)
    }

    // ── Private ───────────────────────────────────────────────────────────────

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let (tx, rx) = oneshot::channel();
        self.msg_tx
            .send(OutboundMsg::Request {
                id,
                method: method.to_string(),
                params,
                tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' is no longer running", self.name))?;

        rx.await
            .map_err(|_| anyhow::anyhow!("MCP '{}' dropped response for '{method}'", self.name))?
    }

    async fn notify(&self, method: &str, params: Value) {
        let _ = self
            .msg_tx
            .send(OutboundMsg::Notification {
                method: method.to_string(),
                params,
            })
            .await;
    }

    async fn initialize(&self) -> Result<()> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "PetruTerm",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }),
        )
        .await?;

        // Required notification after receiving the initialize response.
        self.notify("notifications/initialized", json!({})).await;
        Ok(())
    }

    async fn fetch_tools(&self) -> Result<Vec<McpTool>> {
        let result = self.request("tools/list", json!({})).await?;
        parse_tools_list(&result)
    }
}

// ── Background IO loop ────────────────────────────────────────────────────────

async fn run_io_loop(
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    mut msg_rx: mpsc::Receiver<OutboundMsg>,
) {
    let mut writer = BufWriter::new(stdin);
    let mut lines = BufReader::new(stdout).lines();
    let mut pending: HashMap<u64, oneshot::Sender<Result<Value>>> = HashMap::new();

    loop {
        tokio::select! {
            // Outgoing: request or notification from McpClient methods.
            msg = msg_rx.recv() => {
                let Some(msg) = msg else { break };
                match msg {
                    OutboundMsg::Request { id, method, params, tx } => {
                        let frame = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": method,
                            "params": params,
                        });
                        if write_frame(&mut writer, &frame).await.is_ok() {
                            pending.insert(id, tx);
                        } else {
                            let _ = tx.send(Err(anyhow::anyhow!("Failed to write to MCP server")));
                        }
                    }
                    OutboundMsg::Notification { method, params } => {
                        let frame = json!({
                            "jsonrpc": "2.0",
                            "method": method,
                            "params": params,
                        });
                        let _ = write_frame(&mut writer, &frame).await;
                    }
                }
            }

            // Incoming: response line from the server's stdout.
            line = lines.next_line() => {
                match line {
                    Ok(Some(text)) => dispatch_response(&text, &mut pending),
                    _ => break, // EOF or IO error → server exited
                }
            }
        }
    }

    // Drain any remaining pending requests with an error.
    for (_, tx) in pending.drain() {
        let _ = tx.send(Err(anyhow::anyhow!("MCP server process exited")));
    }
}

async fn write_frame(
    writer: &mut BufWriter<tokio::process::ChildStdin>,
    frame: &Value,
) -> Result<()> {
    let mut line = serde_json::to_string(frame)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

fn dispatch_response(text: &str, pending: &mut HashMap<u64, oneshot::Sender<Result<Value>>>) {
    let Ok(val) = serde_json::from_str::<Value>(text) else {
        return; // ignore unparseable lines (e.g. server debug output)
    };

    // Notifications from server (no `id`) are ignored for now.
    let Some(id) = val.get("id").and_then(|v| v.as_u64()) else {
        return;
    };

    let Some(tx) = pending.remove(&id) else {
        return;
    };

    if let Some(err) = val.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown MCP error");
        let _ = tx.send(Err(anyhow::anyhow!("{}", msg)));
    } else {
        let _ = tx.send(Ok(val["result"].clone()));
    }
}

// ── Parsing helpers (pure, testable) ─────────────────────────────────────────

/// Parse a `tools/list` result payload into `McpTool` vec.
pub(crate) fn parse_tools_list(result: &Value) -> Result<Vec<McpTool>> {
    let tools = result
        .get("tools")
        .and_then(|v| v.as_array())
        .context("tools/list response missing 'tools' array")?;

    let parsed = tools
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?.to_string();
            let description = t
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input_schema = t
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
            Some(McpTool { name, description, input_schema })
        })
        .collect();

    Ok(parsed)
}

/// Extract the text content from a `tools/call` result payload.
pub(crate) fn extract_tool_result_text(result: &Value) -> Result<String> {
    // MCP spec: result.content is an array of content blocks.
    // We join all text blocks.
    if let Some(content) = result.get("content").and_then(|v| v.as_array()) {
        let text: String = content
            .iter()
            .filter_map(|block| {
                if block.get("type")?.as_str()? == "text" {
                    block.get("text")?.as_str().map(str::to_string)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if !text.is_empty() {
            return Ok(text);
        }
    }

    // Fallback: serialize the whole result as JSON.
    Ok(serde_json::to_string_pretty(result).unwrap_or_default())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tools_list_standard() {
        let result = json!({
            "tools": [
                {
                    "name": "read_file",
                    "description": "Read a file",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }
                },
                {
                    "name": "list_dir",
                    "description": "List directory"
                }
            ]
        });

        let tools = parse_tools_list(&result).unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[0].description, "Read a file");
        assert_eq!(tools[1].name, "list_dir");
        // Missing inputSchema gets default
        assert_eq!(tools[1].input_schema["type"], "object");
    }

    #[test]
    fn parse_tools_list_missing_array_errors() {
        let result = json!({ "other": "field" });
        assert!(parse_tools_list(&result).is_err());
    }

    #[test]
    fn extract_tool_result_text_content_blocks() {
        let result = json!({
            "content": [
                { "type": "text", "text": "Hello" },
                { "type": "image", "data": "base64..." },
                { "type": "text", "text": "World" }
            ]
        });
        let text = extract_tool_result_text(&result).unwrap();
        assert_eq!(text, "Hello\nWorld");
    }

    #[test]
    fn extract_tool_result_text_fallback_to_json() {
        let result = json!({ "unknown": "format" });
        let text = extract_tool_result_text(&result).unwrap();
        assert!(text.contains("unknown"));
    }

    #[test]
    fn to_openai_spec_shape() {
        let tool = McpTool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        };
        let spec = tool.to_openai_spec();
        assert_eq!(spec["type"], "function");
        assert_eq!(spec["function"]["name"], "read_file");
        assert_eq!(spec["function"]["parameters"]["type"], "object");
    }
}
