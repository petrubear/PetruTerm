pub mod fs;
mod session;
pub mod terminal;

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use agent_client_protocol_tokio::AcpAgent;
use anyhow::Result;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::config::schema::AcpAgentConfig;
use crate::llm::chat_panel::AiEvent;

use self::session::run_session;
use self::terminal::AcpTerminalRequest;

/// Shared context for the current in-flight prompt.
type QueryCtx = Arc<Mutex<Option<mpsc::Sender<AiEvent>>>>;
type TermCtx = Arc<Mutex<Option<mpsc::Sender<AcpTerminalRequest>>>>;

struct PromptMsg {
    content: String,
    ai_tx: mpsc::Sender<AiEvent>,
    terminal_tx: mpsc::Sender<AcpTerminalRequest>,
}

/// Persistent ACP session.  One session lives for the lifetime of a chat panel.
///
/// Both backends (Provider and Agent) emit the same `AiEvent`s so `ChatPanel`
/// and `UiManager` do not need to distinguish them.
pub struct AcpSession {
    #[allow(dead_code)]
    pub agent_name: String,
    #[allow(dead_code)]
    pub display_name: String,
    prompt_tx: mpsc::Sender<PromptMsg>,
    pub last_prompt_at: Instant,
    _task: tokio::task::JoinHandle<()>,
}

impl AcpSession {
    /// Spawn the agent process, initialise the ACP connection and create a
    /// session.  Returns once the agent is ready to accept prompts.
    pub async fn connect(cfg: &AcpAgentConfig, cwd: &Path) -> Result<Self> {
        let agent = build_acp_agent(cfg)?;

        let agent_name = Path::new(&cfg.command)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&cfg.command)
            .to_string();
        let display_name = cfg
            .display_name
            .clone()
            .unwrap_or_else(|| agent_name.clone());

        let (prompt_tx, prompt_rx) = mpsc::channel::<PromptMsg>(4);
        let (ready_tx, ready_rx) = oneshot::channel::<Result<()>>();
        let cwd = cwd.to_path_buf();

        let task = tokio::spawn(run_session(agent, cwd, prompt_rx, ready_tx));

        // Block until initialize + new_session complete (or task dies).
        ready_rx
            .await
            .map_err(|_| anyhow::anyhow!("ACP session task exited before signalling ready"))
            .and_then(|r| r)?;

        Ok(AcpSession {
            agent_name,
            display_name,
            prompt_tx,
            last_prompt_at: Instant::now(),
            _task: task,
        })
    }

    /// Send a prompt to the agent.  Tokens stream back via `ai_tx`.
    /// Terminal/fs callbacks from the agent are forwarded via `terminal_tx`.
    #[allow(dead_code)]
    pub async fn prompt(
        &mut self,
        content: &str,
        ai_tx: mpsc::Sender<AiEvent>,
        terminal_tx: mpsc::Sender<AcpTerminalRequest>,
    ) -> Result<()> {
        self.last_prompt_at = Instant::now();
        self.prompt_tx
            .send(PromptMsg {
                content: content.to_string(),
                ai_tx,
                terminal_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("ACP session task closed"))
    }

    /// Returns `true` if no prompt has been sent in the last 300 seconds.
    #[allow(dead_code)]
    pub fn is_idle(&self) -> bool {
        self.last_prompt_at.elapsed().as_secs() >= 300
    }

    /// Sync variant of `prompt` — uses `try_send` so it can be called from the main thread.
    /// Returns an error if the internal channel is full or the session task has died.
    pub fn try_send_prompt(
        &mut self,
        content: String,
        ai_tx: mpsc::Sender<AiEvent>,
        terminal_tx: mpsc::Sender<AcpTerminalRequest>,
    ) -> Result<()> {
        self.last_prompt_at = Instant::now();
        self.prompt_tx
            .try_send(PromptMsg {
                content,
                ai_tx,
                terminal_tx,
            })
            .map_err(|e| anyhow::anyhow!("ACP prompt: {e}"))
    }
}

fn build_acp_agent(cfg: &AcpAgentConfig) -> Result<AcpAgent> {
    // Pass env vars as leading `KEY=VALUE` args so `AcpAgent::from_args` picks them up.
    let mut argv: Vec<String> = cfg.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    argv.push(cfg.command.clone());
    argv.extend(cfg.args.iter().cloned());
    AcpAgent::from_args(argv).map_err(|e| anyhow::anyhow!("ACP agent init: {e}"))
}
