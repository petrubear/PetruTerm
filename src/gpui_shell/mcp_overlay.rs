// Builds the markdown content the info overlay shows for an MCP server,
// ported from the wgpu build's `UiManager::mcp_overlay_content`.

use crate::llm::mcp::manager::McpManager;

pub fn mcp_overlay_content(mcp: &McpManager, server_name: &str) -> String {
    let tools = mcp.tools_for_server(server_name);
    let mut out = format!("# {server_name}\n\n");
    if tools.is_empty() {
        out.push_str("*No tools registered (server not connected or no tools).*\n");
        return out;
    }
    out.push_str(&format!("## Tools ({})\n\n", tools.len()));
    for tool in tools {
        out.push_str(&format!("### {}\n", tool.name));
        if !tool.description.is_empty() {
            out.push_str(&format!("{}\n\n", tool.description));
        }
        let schema = serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default();
        if schema != "null" && !schema.is_empty() {
            out.push_str(&format!("```json\n{schema}\n```\n\n"));
        }
    }
    out
}
