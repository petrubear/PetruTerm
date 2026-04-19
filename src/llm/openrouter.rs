use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use super::tools::{AgentStepResult, ToolCall};
use super::{ChatMessage, LlmProvider, TokenStream};
use crate::config::schema::LlmConfig;

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub struct OpenRouterProvider {
    client: Client,
    api_key: SecretString,
    model: String,
    base_url: String,
}

/// Read `OPENROUTER_API_KEY` from the macOS Keychain via the `security` CLI.
/// The key must have been stored with:
///   security add-generic-password -s PetruTerm -a OPENROUTER_API_KEY -w <key>
fn keychain_api_key() -> Option<SecretString> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "PetruTerm",
                "-a",
                "OPENROUTER_API_KEY",
                "-w",
            ])
            .output()
            .ok()?;
        if out.status.success() {
            let key = String::from_utf8(out.stdout).ok()?.trim().to_string();
            if !key.is_empty() {
                return Some(SecretString::from(key));
            }
        }
    }
    None
}

impl OpenRouterProvider {
    /// Build a provider from [`LlmConfig`].
    ///
    /// API key resolution order:
    ///   1. `config.api_key` (set by Lua via `os.getenv("OPENROUTER_API_KEY")`)
    ///   2. `OPENROUTER_API_KEY` environment variable (direct fallback)
    ///   3. macOS Keychain (service "PetruTerm", account "OPENROUTER_API_KEY")
    pub fn from_config(config: &LlmConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| {
                std::env::var("OPENROUTER_API_KEY")
                    .ok()
                    .map(SecretString::from)
            })
            .or_else(keychain_api_key)
            .context(
                "OpenRouter API key not found.\n\
                 Options:\n\
                 1. Set OPENROUTER_API_KEY in your environment (e.g. ~/.zshrc)\n\
                 2. Set llm.api_key in your llm.lua config\n\
                 3. Store in Keychain: security add-generic-password \
                    -s PetruTerm -a OPENROUTER_API_KEY -w <your-key>",
            )?;

        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
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
        .filter(|m| !matches!(m.role, super::ChatRole::Tool(_)))
        .map(|m| ApiMessage {
            role: m.role.as_str(),
            content: &m.content,
        })
        .collect()
}

/// Parse one or more SSE `data:` lines from a raw chunk.
/// Returns `Ok(Some(tokens))` if content was found, `Ok(None)` if the chunk
/// was empty/metadata-only, or `Err` if the API returned an error payload.
fn parse_sse_chunk(chunk: &str) -> anyhow::Result<Option<String>> {
    let mut tokens = String::new();
    for line in chunk.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }

        // Attempt to parse as a generic JSON value first so we can detect
        // API-level errors that OpenRouter embeds in the SSE stream.
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
impl LlmProvider for OpenRouterProvider {
    async fn stream(&self, messages: Vec<ChatMessage>) -> Result<TokenStream> {
        let url = format!("{}/chat/completions", self.base_url);

        let byte_stream = self
            .client
            .post(&url)
            .bearer_auth(self.api_key.expose_secret())
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

        let body = AgentRequest {
            model: &self.model,
            messages: api_messages,
            tools: tool_specs.to_vec(),
            tool_choice: "auto",
            stream: false,
        };

        let resp_json: Value = self
            .client
            .post(&url)
            .bearer_auth(self.api_key.expose_secret())
            .header("HTTP-Referer", "https://github.com/edisontim/petruterm")
            .header("X-Title", "PetruTerm")
            .json(&body)
            .send()
            .await
            .context("OpenRouter agent_step request failed")?
            .error_for_status()
            .context("OpenRouter returned an error status")?
            .json()
            .await
            .context("Failed to parse OpenRouter agent_step response")?;

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
