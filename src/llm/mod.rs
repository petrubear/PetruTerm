pub mod ai_block;
pub mod chat_panel;
pub mod openai_compat;
pub mod openrouter;
pub mod shell_context;
pub mod tools;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::schema::LlmConfig;
use tools::AgentStepResult;

/// A single message in a chat conversation.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: ChatRole::System, content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: ChatRole::User, content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: ChatRole::Assistant, content: content.into() }
    }

    /// Create a tool-result message (response to an LLM tool call).
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: ChatRole::Tool(tool_call_id.into()), content: content.into() }
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
    Tool(String),
}

impl ChatRole {
    pub fn as_str(&self) -> &str {
        match self {
            ChatRole::System    => "system",
            ChatRole::User      => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool(_)   => "tool",
        }
    }
}

/// Streamed token chunks from a provider.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<String>> + Send>>;

/// Core LLM provider interface. Implementors must be Send + Sync.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send messages and return the full assistant response.
    async fn complete(&self, messages: Vec<ChatMessage>) -> Result<String>;

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
        api_messages: Vec<Value>,
        tool_specs: &[Value],
    ) -> Result<AgentStepResult>;
}

/// Build the active [`LlmProvider`] from config.
/// Returns an `Arc` so the provider can be cloned cheaply into tokio tasks.
pub fn build_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>> {
    match config.provider.as_str() {
        "openrouter" => Ok(Arc::new(openrouter::OpenRouterProvider::from_config(config)?)),
        "ollama"     => Ok(Arc::new(openai_compat::OpenAICompatProvider::ollama(config))),
        "lmstudio"   => Ok(Arc::new(openai_compat::OpenAICompatProvider::lmstudio(config))),
        other => anyhow::bail!("Unknown LLM provider: '{other}'. Valid options: openrouter, ollama, lmstudio"),
    }
}
