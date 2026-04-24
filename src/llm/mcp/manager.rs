use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use super::client::{McpClient, McpTool};
use super::config::McpConfig;

/// Manages all active MCP server connections for a workspace session.
///
/// Lifecycle: created in `UiManager`, started when the AI panel opens,
/// stopped when the app exits (Drop kills child processes via `kill_on_drop`).
#[derive(Default)]
pub struct McpManager {
    clients: HashMap<String, McpClient>,
    /// Maps tool name → server name for routing `tools/call` requests.
    tool_routes: HashMap<String, String>,
}

impl McpManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn all configured servers concurrently.
    ///
    /// Returns a list of `(server_name, error)` for servers that failed to start.
    /// Servers that start successfully are available immediately for tool calls.
    pub async fn start_all(&mut self, config: &McpConfig) -> Vec<(String, anyhow::Error)> {
        // Spawn all connections concurrently.
        let futures: Vec<_> = config
            .iter()
            .map(|(name, cfg)| {
                let name = name.clone();
                let cfg = cfg.clone();
                async move { (name.clone(), McpClient::connect(name, cfg).await) }
            })
            .collect();

        let results = futures_util::future::join_all(futures).await;

        let mut errors = Vec::new();
        for (name, result) in results {
            match result {
                Ok(client) => {
                    let tool_names: Vec<&str> =
                        client.tools.iter().map(|t| t.name.as_str()).collect();
                    log::info!(
                        "MCP server '{}' connected ({} tools: {})",
                        name,
                        tool_names.len(),
                        tool_names.join(", ")
                    );
                    // Register each tool, first-registered wins on name conflict.
                    for tool in &client.tools {
                        self.tool_routes
                            .entry(tool.name.clone())
                            .or_insert_with(|| name.clone());
                    }
                    self.clients.insert(name, client);
                }
                Err(e) => errors.push((name, e)),
            }
        }

        errors
    }

    /// All tools from all connected servers, in OpenAI function-calling format.
    pub fn all_tools_openai(&self) -> Vec<Value> {
        self.clients
            .values()
            .flat_map(|c| c.tools.iter().map(McpTool::to_openai_spec))
            .collect()
    }

    /// All raw tool definitions (used for display / palette).
    #[allow(dead_code)]
    pub fn all_tools(&self) -> Vec<(String, &McpTool)> {
        self.clients
            .iter()
            .flat_map(|(server, c)| c.tools.iter().map(move |t| (server.clone(), t)))
            .collect()
    }

    /// Route a tool call to the correct server and return the result text.
    ///
    /// `tool_name` is the bare name as returned by `tools/list` (no prefix).
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<String> {
        let server_name = self
            .tool_routes
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("No MCP server registered for tool '{tool_name}'"))?;

        let client = self.clients.get(server_name).ok_or_else(|| {
            anyhow::anyhow!("MCP server '{server_name}' registered but not connected")
        })?;

        client.call_tool(tool_name, arguments).await
    }

    /// True if at least one server is connected.
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        !self.clients.is_empty()
    }

    /// Returns the server name that owns `tool_name`, or `None` if not found.
    pub fn server_for_tool(&self, tool_name: &str) -> Option<&str> {
        self.tool_routes.get(tool_name).map(String::as_str)
    }

    /// Number of currently connected MCP servers.
    pub fn connected_count(&self) -> usize {
        self.clients.len()
    }

    /// Server names currently connected.
    #[allow(dead_code)]
    pub fn server_names(&self) -> Vec<&str> {
        self.clients.keys().map(String::as_str).collect()
    }
}
