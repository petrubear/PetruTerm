use super::schema::{AcpAgentConfig, Config, LlmBackend, LlmConfig};

#[derive(Debug, Clone)]
pub struct LlmRuntimeView {
    pub enabled: bool,
    pub backend: LlmBackend,
    pub panel_width_cols: u16,
    pub agent: Option<AcpAgentConfig>,
    pub provider_cfg: LlmConfig,
}

pub fn llm_runtime_view(config: &Config) -> LlmRuntimeView {
    LlmRuntimeView {
        enabled: config.llm.enabled,
        backend: config.llm.backend.clone(),
        panel_width_cols: config.llm.ui.width_cols,
        agent: config.llm.agent.clone(),
        provider_cfg: config.llm.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_runtime_view_preserves_backend_agent_and_ui_width() {
        let mut config = Config::default();
        config.llm.enabled = true;
        config.llm.backend = LlmBackend::Agent;
        config.llm.ui.width_cols = 72;
        config.llm.agent = Some(AcpAgentConfig {
            command: "npx".into(),
            args: vec!["-y".into(), "@agentclientprotocol/claude-agent-acp".into()],
            env: vec![("FOO".into(), "bar".into())],
            display_name: Some("Claude".into()),
        });

        let view = llm_runtime_view(&config);
        assert!(view.enabled);
        assert_eq!(view.backend, LlmBackend::Agent);
        assert_eq!(view.panel_width_cols, 72);
        assert_eq!(view.agent.as_ref().unwrap().command, "npx");
    }
}
