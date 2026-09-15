// gpui chrome migration (M5a Task 3): the ACP session lifecycle -- connect
// (spawned, never blocking), poll-drain the result, and backend-aware
// rewiring. `AcpSession::connect` itself (src/llm/acp/mod.rs) is already
// fully engine-agnostic and reused unmodified; this file is the gpui-side
// bridge around it, mirroring `UiManager::rewire_backend`/`spawn_acp_
// connect`/`poll_acp_connect` (src/app/ui/{mod,providers}.rs) minus the
// winit `EventLoopProxy<()>` wakeup every one of them takes -- the 33ms
// poll tick IS the wake mechanism here, the same "drop the wakeup
// parameter" adaptation every prior async-scan milestone this session has
// made (M5c's branch/workspace scans, M5b's file-picker scan).

use std::path::PathBuf;

use crate::config::schema::LlmBackend;
use crate::config::Config;
use crate::llm::acp::AcpSession;

use super::ChatPanelView;

impl ChatPanelView {
    /// Re-wire the active backend from a fresh config. Call at
    /// construction and on every config reload (hot-reload or `/agent`).
    /// Never blocks: the ACP connect (subprocess spawn + protocol
    /// handshake) runs in the background, picked up later by `poll_acp_
    /// connect`.
    pub fn rewire_backend(&mut self, config: &Config, tokio_rt: &tokio::runtime::Runtime) {
        self.acp_pending_connect = None;
        let view = crate::config::llm_view::llm_runtime_view(config);
        match view.backend {
            LlmBackend::Provider => {
                self.acp_session = None;
                self.rewire_provider(&config.llm);
            }
            LlmBackend::Agent => {
                self.llm_provider = None;
                self.llm_init_error = None;
                self.acp_session = None;
                if let Some(agent_cfg) = config.llm.agent.clone() {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    self.acp_pending_connect = Some(spawn_acp_connect(tokio_rt, agent_cfg, cwd));
                } else {
                    self.llm_init_error =
                        Some("llm.agent config is required when backend = \"agent\"".into());
                }
            }
        }
    }

    /// Drain a completed connect attempt. Returns `true` if it updated
    /// anything (caller should `cx.notify()`). Called from `poll.rs`'s
    /// existing 33ms tick.
    pub(in crate::gpui_shell) fn poll_acp_connect(&mut self) -> bool {
        let Some(rx) = &mut self.acp_pending_connect else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok(session)) => {
                self.acp_session = Some(session);
                self.llm_init_error = None;
                self.acp_pending_connect = None;
                true
            }
            Ok(Err(e)) => {
                log::error!("ACP connect: {e}");
                self.llm_init_error = Some(e);
                self.acp_pending_connect = None;
                true
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => false,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.llm_init_error = Some("ACP connect task ended unexpectedly".to_string());
                self.acp_pending_connect = None;
                true
            }
        }
    }
}

fn spawn_acp_connect(
    rt: &tokio::runtime::Runtime,
    agent_cfg: crate::config::schema::AcpAgentConfig,
    cwd: PathBuf,
) -> tokio::sync::oneshot::Receiver<Result<AcpSession, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let result = AcpSession::connect(&agent_cfg, &cwd)
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
    });
    rx
}
