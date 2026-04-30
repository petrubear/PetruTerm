use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

use super::tools::AgentStepResult;
use super::{
    infer_context_window, parse_agent_response, parse_sse_chunk, parse_usage, ChatMessage,
    LlmProvider, TokenStream, UsageStats,
};
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
    messages: &'a [Value],
    tools: &'a [Value],
    tool_choice: &'a str,
    stream: bool,
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
        api_messages: &[Value],
        tool_specs: &[Value],
    ) -> Result<(AgentStepResult, Option<UsageStats>)> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = AgentRequest {
            model: &self.model,
            messages: api_messages,
            tools: tool_specs,
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

        let usage = parse_usage(&resp_json);
        let result = parse_agent_response(resp_json)?;
        Ok((result, usage))
    }

    fn context_window(&self) -> Option<u32> {
        infer_context_window(&self.model)
    }
}
