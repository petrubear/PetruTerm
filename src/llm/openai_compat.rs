use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde_json::Value;
/// OpenAI-compatible provider — works with Ollama and LMStudio out of the box.
///
/// Both expose the same `/v1/chat/completions` endpoint with SSE streaming.
/// No API key is required by default; `api_key` is forwarded if present.
use std::time::Duration;

use super::tools::AgentStepResult;
use super::{
    infer_context_window, parse_agent_response, parse_sse_chunk, parse_usage, ChatMessage,
    LlmProvider, TokenStream, UsageStats,
};
use crate::config::schema::LlmConfig;

pub struct OpenAICompatProvider {
    client: Client,
    model: String,
    base_url: String,
    /// Optional Bearer token (LMStudio supports it; Ollama ignores it).
    api_key: Option<SecretString>,
}

impl OpenAICompatProvider {
    fn build_client() -> Client {
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client")
    }

    pub fn ollama(config: &LlmConfig) -> Self {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434/v1".into());
        Self {
            client: Self::build_client(),
            model: config.model.clone(),
            base_url,
            api_key: config.api_key.clone(),
        }
    }

    pub fn lmstudio(config: &LlmConfig) -> Self {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:1234/v1".into());
        Self {
            client: Self::build_client(),
            model: config.model.clone(),
            base_url,
            api_key: config.api_key.clone(),
        }
    }
}

// ── Wire types ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ApiMessage<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct ApiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct AgentRequest<'a> {
    model: &'a str,
    messages: &'a [Value],
    tools: &'a [Value],
    tool_choice: &'a str,
    stream: bool,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn build_api_messages<'a>(messages: &'a [ChatMessage]) -> Vec<ApiMessage<'a>> {
    messages
        .iter()
        .map(|m| ApiMessage {
            role: m.role.as_str(),
            content: &m.content,
        })
        .collect()
}

// ── LlmProvider impl ─────────────────────────────────────────────────────────

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
    async fn stream(&self, messages: Vec<ChatMessage>) -> Result<TokenStream> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self.client.post(&url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose_secret());
        }
        let byte_stream = req
            .json(&ChatRequest {
                model: &self.model,
                messages: build_api_messages(&messages),
                stream: true,
            })
            .send()
            .await
            .context("Stream request failed")?
            .error_for_status()
            .context("Server returned an error status")?
            .bytes_stream();

        let token_stream = byte_stream
            .map(|chunk| -> Result<Option<String>> {
                let bytes = chunk.context("Error reading SSE chunk")?;
                let text = std::str::from_utf8(&bytes).context("Non-UTF8 SSE chunk")?;
                parse_sse_chunk(text)
            })
            .filter_map(|result| async move {
                match result {
                    Ok(Some(tok)) => Some(Ok(tok)),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                }
            });

        Ok(Box::pin(token_stream))
    }

    async fn agent_step(
        &self,
        api_messages: &[Value],
        tool_specs: &[Value],
    ) -> Result<(AgentStepResult, Option<UsageStats>)> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self.client.post(&url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose_secret());
        }

        let resp_json: Value = req
            .json(&AgentRequest {
                model: &self.model,
                messages: api_messages,
                tools: tool_specs,
                tool_choice: "auto",
                stream: false,
            })
            .send()
            .await
            .context("Agent step request failed")?
            .error_for_status()
            .context("Server returned an error status")?
            .json()
            .await
            .context("Failed to parse agent step response")?;

        let usage = parse_usage(&resp_json);
        let result = parse_agent_response(resp_json)?;
        Ok((result, usage))
    }

    fn context_window(&self) -> Option<u32> {
        infer_context_window(&self.model)
    }
}
