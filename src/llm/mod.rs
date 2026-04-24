pub mod ai_block;
pub mod chat_panel;
pub mod mcp;
pub mod copilot;
pub mod diff;
pub mod openai_compat;
pub mod openrouter;
pub mod shell_context;
pub mod skills;
pub mod tools;

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::Stream;
use serde::Deserialize;
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::schema::LlmConfig;
use tools::{AgentStepResult, ToolCall};

/// A single message in a chat conversation.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }

    /// Create a tool-result message (response to an LLM tool call).
    #[allow(dead_code)]
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool(tool_call_id.into()),
            content: content.into(),
        }
    }

    /// Serialize to the JSON format expected by OpenAI-compatible APIs.
    /// Regular roles produce `{role, content}`; Tool roles add `tool_call_id`.
    pub fn to_api_value(&self) -> Value {
        match &self.role {
            ChatRole::Tool(id) => serde_json::json!({
                "role": "tool",
                "tool_call_id": id,
                "content": self.content,
            }),
            _ => serde_json::json!({
                "role": self.role.as_str(),
                "content": self.content,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    /// Tool-result message. Inner string is the `tool_call_id` from the LLM's request.
    #[allow(dead_code)]
    Tool(String),
}

impl ChatRole {
    pub fn as_str(&self) -> &str {
        match self {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool(_) => "tool",
        }
    }
}

/// Streamed token chunks from a provider.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<String>> + Send>>;

/// Core LLM provider interface. Implementors must be Send + Sync.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send messages and stream response tokens as they arrive.
    async fn stream(&self, messages: Vec<ChatMessage>) -> Result<TokenStream>;

    /// Run one agentic step: send pre-serialized messages with tool definitions.
    ///
    /// `api_messages` is a Vec of JSON Values already in OpenAI wire format —
    /// this allows mixing regular messages, tool-call assistant turns, and
    /// tool-result turns without a complex enum hierarchy.
    ///
    /// Returns either the assistant's text response or tool calls to execute.
    async fn agent_step(
        &self,
        api_messages: &[Value],
        tool_specs: &[Value],
    ) -> Result<AgentStepResult>;
}

// ── Shared SSE / agent-response parsing ──────────────────────────────────────
// Used by openrouter, openai_compat, and copilot — all speak the same wire format.

#[derive(Deserialize)]
pub(crate) struct SseResponse {
    pub choices: Vec<SseChoice>,
}

#[derive(Deserialize)]
pub(crate) struct SseChoice {
    pub delta: Option<SseDelta>,
}

#[derive(Deserialize)]
pub(crate) struct SseDelta {
    pub content: Option<String>,
}

/// Parse one or more SSE `data:` lines from a raw chunk.
pub(crate) fn parse_sse_chunk(chunk: &str) -> Result<Option<String>> {
    let mut tokens = String::new();
    for line in chunk.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        let Ok(val) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(msg) = val.pointer("/error/message").and_then(|v| v.as_str()) {
            anyhow::bail!("{msg}");
        }
        if let Ok(resp) = serde_json::from_value::<SseResponse>(val) {
            for choice in resp.choices {
                if let Some(delta) = choice.delta {
                    if let Some(content) = delta.content {
                        tokens.push_str(&content);
                    }
                }
            }
        }
    }
    Ok(if tokens.is_empty() {
        None
    } else {
        Some(tokens)
    })
}

/// Parse a non-streaming agent response (OpenAI-compatible).
pub(crate) fn parse_agent_response(resp: Value) -> Result<AgentStepResult> {
    let choice = resp["choices"]
        .as_array()
        .and_then(|a| a.first())
        .context("Agent response had no choices")?;

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let msg = &choice["message"];

    if finish_reason == "tool_calls" || msg.get("tool_calls").is_some() {
        let calls_json = msg["tool_calls"]
            .as_array()
            .context("Expected tool_calls array")?;

        let calls: Vec<ToolCall> = calls_json
            .iter()
            .filter_map(|c| {
                let id = c.get("id")?.as_str()?.to_string();
                let func = c.get("function")?;
                let name = func.get("name")?.as_str()?.to_string();
                let arguments = func.get("arguments")?.as_str().unwrap_or("{}").to_string();
                Some(ToolCall {
                    id,
                    name,
                    arguments,
                })
            })
            .collect();

        if calls.is_empty() {
            anyhow::bail!("tool_calls finish_reason but no parseable tool calls");
        }
        Ok(AgentStepResult::ToolCalls {
            assistant_msg: msg.clone(),
            calls,
        })
    } else {
        let text = msg
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(AgentStepResult::Text(text))
    }
}

// ── Provider factory ──────────────────────────────────────────────────────────

/// Build the active [`LlmProvider`] from config.
/// Returns an `Arc` so the provider can be cloned cheaply into tokio tasks.
pub fn build_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>> {
    match config.provider.as_str() {
        "openrouter" => Ok(Arc::new(openrouter::OpenRouterProvider::from_config(
            config,
        )?)),
        "ollama" => Ok(Arc::new(openai_compat::OpenAICompatProvider::ollama(
            config,
        ))),
        "lmstudio" => Ok(Arc::new(openai_compat::OpenAICompatProvider::lmstudio(
            config,
        ))),
        "copilot" => Ok(Arc::new(copilot::CopilotProvider::from_config(config)?)),
        other => anyhow::bail!(
            "Unknown LLM provider: '{other}'. Valid options: openrouter, ollama, lmstudio, copilot"
        ),
    }
}
