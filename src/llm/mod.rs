pub mod ai_block;
pub mod chat_panel;
pub mod openai_compat;
pub mod openrouter;
pub mod shell_context;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::Stream;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::schema::LlmConfig;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatRole::System    => "system",
            ChatRole::User      => "user",
            ChatRole::Assistant => "assistant",
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
