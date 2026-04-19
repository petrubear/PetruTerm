use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
/// OpenAI-compatible provider — works with Ollama and LMStudio out of the box.
///
/// Both expose the same `/v1/chat/completions` endpoint with SSE streaming.
/// No API key is required by default; `api_key` is forwarded if present.
use std::time::Duration;

use super::tools::{AgentStepResult, ToolCall};
use super::{ChatMessage, LlmProvider, TokenStream};
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
    messages: Vec<Value>,
    tools: Vec<Value>,
    tool_choice: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    delta: Option<Delta>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
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

fn parse_sse_chunk(chunk: &str) -> Result<Option<String>> {
    let mut tokens = String::new();
    for line in chunk.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
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
    Ok(if tokens.is_empty() {
        None
    } else {
        Some(tokens)
    })
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
        api_messages: Vec<Value>,
        tool_specs: &[Value],
    ) -> Result<AgentStepResult> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self.client.post(&url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose_secret());
        }

        let resp_json: Value = req
            .json(&AgentRequest {
                model: &self.model,
                messages: api_messages,
                tools: tool_specs.to_vec(),
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

        parse_agent_response(resp_json)
    }
}

fn parse_agent_response(resp: Value) -> Result<AgentStepResult> {
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
