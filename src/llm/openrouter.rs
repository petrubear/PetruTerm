use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::schema::LlmConfig;
use super::{ChatMessage, LlmProvider, TokenStream};

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub struct OpenRouterProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenRouterProvider {
    /// Build a provider from [`LlmConfig`].
    ///
    /// API key resolution order:
    ///   1. `config.api_key` (set by Lua via `os.getenv("OPENROUTER_API_KEY")`)
    ///   2. `OPENROUTER_API_KEY` environment variable (direct fallback)
    pub fn from_config(config: &LlmConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
            .context(
                "OpenRouter API key not found. \
                 Set the OPENROUTER_API_KEY environment variable \
                 or llm.api_key in your llm.lua config.",
            )?;

        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Ok(Self {
            client: Client::new(),
            api_key,
            model: config.model.clone(),
            base_url,
        })
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
    // Non-streaming response
    message: Option<MessageOwned>,
    // Streaming delta
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

/// Parse one or more SSE `data:` lines from a raw chunk.
/// Returns `Ok(Some(tokens))` if content was found, `Ok(None)` if the chunk
/// was empty/metadata-only, or `Err` if the API returned an error payload.
fn parse_sse_chunk(chunk: &str) -> anyhow::Result<Option<String>> {
    let mut tokens = String::new();
    for line in chunk.lines() {
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data == "[DONE]" { break; }

        // Attempt to parse as a generic JSON value first so we can detect
        // API-level errors that OpenRouter embeds in the SSE stream.
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
impl LlmProvider for OpenRouterProvider {
    async fn complete(&self, messages: Vec<ChatMessage>) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("HTTP-Referer", "https://github.com/edisontim/petruterm")
            .header("X-Title", "PetruTerm")
            .json(&ChatRequest {
                model: &self.model,
                messages: build_api_messages(&messages),
                stream: false,
            })
            .send()
            .await
            .context("OpenRouter request failed")?
            .error_for_status()
            .context("OpenRouter returned an error status")?
            .json::<ChatResponse>()
            .await
            .context("Failed to parse OpenRouter response")?;

        resp.choices
            .into_iter()
            .next()
            .and_then(|c| c.message)
            .map(|m| m.content)
            .context("OpenRouter response contained no choices")
    }

    async fn stream(&self, messages: Vec<ChatMessage>) -> Result<TokenStream> {
        let url = format!("{}/chat/completions", self.base_url);

        let byte_stream = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("HTTP-Referer", "https://github.com/edisontim/petruterm")
            .header("X-Title", "PetruTerm")
            .json(&ChatRequest {
                model: &self.model,
                messages: build_api_messages(&messages),
                stream: true,
            })
            .send()
            .await
            .context("OpenRouter stream request failed")?
            .error_for_status()
            .context("OpenRouter returned an error status")?
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
