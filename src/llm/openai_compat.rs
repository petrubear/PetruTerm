/// OpenAI-compatible provider — works with Ollama and LMStudio out of the box.
///
/// Both expose the same `/v1/chat/completions` endpoint with SSE streaming.
/// No API key is required by default; `api_key` is forwarded if present.
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::schema::LlmConfig;
use super::{ChatMessage, LlmProvider, TokenStream};

pub struct OpenAICompatProvider {
    client: Client,
    model: String,
    base_url: String,
    /// Optional Bearer token (LMStudio supports it; Ollama ignores it).
    api_key: Option<String>,
}

impl OpenAICompatProvider {
    pub fn ollama(config: &LlmConfig) -> Self {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434/v1".into());
        Self {
            client: Client::new(),
            model: config.model.clone(),
            base_url,
            api_key: config.api_key.clone().filter(|k| !k.is_empty()),
        }
    }

    pub fn lmstudio(config: &LlmConfig) -> Self {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:1234/v1".into());
        Self {
            client: Client::new(),
            model: config.model.clone(),
            base_url,
            api_key: config.api_key.clone().filter(|k| !k.is_empty()),
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

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Option<MessageOwned>,
    delta: Option<Delta>,
}

#[derive(Deserialize)]
struct MessageOwned {
    content: String,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn build_api_messages<'a>(messages: &'a [ChatMessage]) -> Vec<ApiMessage<'a>> {
    messages
        .iter()
        .map(|m| ApiMessage { role: m.role.as_str(), content: &m.content })
        .collect()
}

fn parse_sse_chunk(chunk: &str) -> Result<Option<String>> {
    let mut tokens = String::new();
    for line in chunk.lines() {
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data == "[DONE]" { break; }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(data) else { continue };
        if let Some(msg) = val.pointer("/error/message").and_then(|v| v.as_str()) {
            anyhow::bail!("{msg}");
        }
        if let Ok(resp) = serde_json::from_value::<ChatResponse>(val) {
            for choice in resp.choices {
                if let Some(delta) = choice.delta {
                    if let Some(content) = delta.content {
                        tokens.push_str(&content);
                    }
                }
            }
        }
    }
    Ok(if tokens.is_empty() { None } else { Some(tokens) })
}

// ── LlmProvider impl ─────────────────────────────────────────────────────────

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
    async fn complete(&self, messages: Vec<ChatMessage>) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self.client.post(&url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .json(&ChatRequest {
                model: &self.model,
                messages: build_api_messages(&messages),
                stream: false,
            })
            .send()
            .await
            .context("Request failed")?
            .error_for_status()
            .context("Server returned an error status")?
            .json::<ChatResponse>()
            .await
            .context("Failed to parse response")?;

        resp.choices
            .into_iter()
            .next()
            .and_then(|c| c.message)
            .map(|m| m.content)
            .context("Response contained no choices")
    }

    async fn stream(&self, messages: Vec<ChatMessage>) -> Result<TokenStream> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self.client.post(&url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
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
                    Ok(None)      => None,
                    Err(e)        => Some(Err(e)),
                }
            });

        Ok(Box::pin(token_stream))
    }
}
