use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::tools::AgentStepResult;
use super::{parse_agent_response, parse_sse_chunk, ChatMessage, LlmProvider, TokenStream};
use crate::config::schema::LlmConfig;

const CHAT_URL: &str = "https://api.githubcopilot.com/chat/completions";
const TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const OAUTH_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
// The registered client_id for GitHub Copilot editor integrations (neovim/editor plugin).
const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const REFRESH_MARGIN_SECS: u64 = 300;

#[derive(Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: u64,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
}

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

struct CachedJwt {
    token: SecretString,
    expires_at: u64,
}

pub struct CopilotProvider {
    client: Client,
    github_token: SecretString,
    cached_jwt: Arc<Mutex<Option<CachedJwt>>>,
    model: String,
}

// ── Keychain helpers (macOS) ──────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn keychain_load() -> Option<SecretString> {
    let out = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s", "PetruTerm",
            "-a", "GITHUB_COPILOT_OAUTH_TOKEN",
            "-w",
        ])
        .output()
        .ok()?;
    if out.status.success() {
        let tok = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !tok.is_empty() {
            return Some(SecretString::from(tok));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn keychain_save(token: &str) {
    // Delete any existing entry first, then add the new one.
    let _ = std::process::Command::new("security")
        .args([
            "delete-generic-password",
            "-s", "PetruTerm",
            "-a", "GITHUB_COPILOT_OAUTH_TOKEN",
        ])
        .output();
    let _ = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-s", "PetruTerm",
            "-a", "GITHUB_COPILOT_OAUTH_TOKEN",
            "-w", token,
        ])
        .output();
}

#[cfg(not(target_os = "macos"))]
fn keychain_load() -> Option<SecretString> { None }
#[cfg(not(target_os = "macos"))]
fn keychain_save(_token: &str) {}

// ── Device flow ───────────────────────────────────────────────────────────────

/// Run the GitHub device flow and return a Copilot-capable OAuth token.
/// Blocks until the user authorizes (or the code expires).
fn run_device_flow() -> Result<SecretString> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // Step 1: request device + user code
    let dev: DeviceCodeResponse = client
        .post(DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", COPILOT_CLIENT_ID),
            ("scope", "read:user"),
        ])
        .send()
        .context("Failed to reach GitHub device code endpoint")?
        .json()
        .context("Failed to parse device code response")?;

    eprintln!(
        "\n[PetruTerm] GitHub Copilot auth required.\n\
         Open: {}\n\
         Enter code: {}\n\
         Waiting for authorization...",
        dev.verification_uri, dev.user_code
    );

    // Step 2: poll until authorized or expired
    let poll_interval = Duration::from_secs(dev.interval.max(5));
    loop {
        std::thread::sleep(poll_interval);

        let resp: OAuthTokenResponse = client
            .post(OAUTH_TOKEN_URL)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", COPILOT_CLIENT_ID),
                ("device_code", dev.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .context("OAuth token poll failed")?
            .json()
            .context("Failed to parse OAuth token response")?;

        match resp.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
            Some(other) => anyhow::bail!("GitHub OAuth error: {other}"),
            None => {}
        }

        if let Some(token) = resp.access_token {
            eprintln!("[PetruTerm] Copilot authorized. Saving token to Keychain.");
            keychain_save(&token);
            return Ok(SecretString::from(token));
        }
    }
}

// ── Token resolution ──────────────────────────────────────────────────────────

fn resolve_github_token(config: &LlmConfig) -> Result<SecretString> {
    // 1. Lua config api_key
    if let Some(key) = &config.api_key {
        return Ok(key.clone());
    }
    // 2. macOS Keychain (preferred — stored by device flow or manually)
    if let Some(tok) = keychain_load() {
        return Ok(tok);
    }
    // 3. Run device flow — prints instructions to stderr, blocks until done
    run_device_flow()
}

// ── Provider ──────────────────────────────────────────────────────────────────

impl CopilotProvider {
    pub fn from_config(config: &LlmConfig) -> Result<Self> {
        let github_token = resolve_github_token(config)?;

        // Copilot models are plain names (gpt-4o, claude-3.5-sonnet, o3-mini, ...).
        // OpenRouter/Ollama models contain '/' or ':' — fall back to gpt-4o for those.
        let model = if config.model.contains('/') || config.model.contains(':') {
            "gpt-4o".to_string()
        } else {
            config.model.clone()
        };

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            github_token,
            cached_jwt: Arc::new(Mutex::new(None)),
            model,
        })
    }

    async fn jwt(&self) -> Result<SecretString> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        {
            let guard = self.cached_jwt.lock().unwrap();
            if let Some(cached) = guard.as_ref() {
                if cached.expires_at > now + REFRESH_MARGIN_SECS {
                    return Ok(cached.token.clone());
                }
            }
        }

        let token_resp = self
            .client
            .get(TOKEN_URL)
            .header(
                "Authorization",
                format!("token {}", self.github_token.expose_secret()),
            )
            .header("Accept", "application/json")
            .header("User-Agent", "PetruTerm/0.1.0")
            .send()
            .await
            .context("Failed to reach GitHub Copilot token endpoint")?;

        let status = token_resp.status();
        if !status.is_success() {
            let body = token_resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub Copilot token exchange failed (HTTP {status}): {body}");
        }

        let resp: CopilotTokenResponse = token_resp
            .json()
            .await
            .context("Failed to parse Copilot token response")?;

        let jwt = SecretString::from(resp.token);
        *self.cached_jwt.lock().unwrap() = Some(CachedJwt {
            token: jwt.clone(),
            expires_at: resp.expires_at,
        });
        Ok(jwt)
    }

    fn request(&self, jwt: &SecretString) -> reqwest::RequestBuilder {
        self.client
            .post(CHAT_URL)
            .bearer_auth(jwt.expose_secret())
            .header("Editor-Version", "Neovim/0.9.5")
            .header("Editor-Plugin-Version", "copilot.lua/1.16.0")
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("OpenAI-Intent", "conversation-panel")
    }
}

#[async_trait]
impl LlmProvider for CopilotProvider {
    async fn stream(&self, messages: Vec<ChatMessage>) -> Result<TokenStream> {
        let jwt = self.jwt().await?;
        let api_messages: Vec<Value> = messages.iter().map(|m| m.to_api_value()).collect();
        let body = serde_json::json!({
            "model": self.model,
            "messages": api_messages,
            "stream": true,
        });

        let chat_resp = self
            .request(&jwt)
            .json(&body)
            .send()
            .await
            .context("Copilot stream request failed")?;

        let status = chat_resp.status();
        if !status.is_success() {
            let body = chat_resp.text().await.unwrap_or_default();
            anyhow::bail!("Copilot chat failed (HTTP {status}): {body}");
        }

        let byte_stream = chat_resp.bytes_stream();

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
    ) -> Result<AgentStepResult> {
        let jwt = self.jwt().await?;
        let body = serde_json::json!({
            "model": self.model,
            "messages": api_messages,
            "tools": tool_specs,
            "tool_choice": "auto",
            "stream": false,
        });

        let agent_resp = self
            .request(&jwt)
            .json(&body)
            .send()
            .await
            .context("Copilot agent_step request failed")?;

        let status = agent_resp.status();
        if !status.is_success() {
            let body = agent_resp.text().await.unwrap_or_default();
            anyhow::bail!("Copilot agent_step failed (HTTP {status}): {body}");
        }

        let resp_json: Value = agent_resp
            .json()
            .await
            .context("Failed to parse Copilot agent_step response")?;

        parse_agent_response(resp_json)
    }
}
