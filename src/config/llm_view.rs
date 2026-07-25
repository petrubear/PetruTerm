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

pub fn agent_display_name(agent: Option<&AcpAgentConfig>) -> Option<&str> {
    let agent = agent?;
    Some(agent.display_name.as_deref().unwrap_or_else(|| {
        std::path::Path::new(&agent.command)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&agent.command)
    }))
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

    #[test]
    fn llm_runtime_view_preserves_provider_defaults() {
        let config = Config::default();
        let view = llm_runtime_view(&config);

        assert_eq!(view.backend, LlmBackend::Provider);
        assert_eq!(view.provider_cfg.provider, "openrouter");
        assert_eq!(
            view.provider_cfg.model,
            "meta-llama/llama-3.1-8b-instruct:free"
        );
        assert_eq!(view.panel_width_cols, config.llm.ui.width_cols);
    }

    #[test]
    fn llm_runtime_view_agent_path_requires_agent_config() {
        let mut config = Config::default();
        config.llm.enabled = true;
        config.llm.backend = LlmBackend::Agent;
        config.llm.agent = None;

        let view = llm_runtime_view(&config);

        assert!(matches!(view.backend, LlmBackend::Agent));
        assert!(view.agent.is_none());
    }

    #[test]
    fn agent_display_name_prefers_display_name_over_command_basename() {
        let agent = AcpAgentConfig {
            command: "/usr/local/bin/claude-agent-acp".into(),
            args: vec![],
            env: vec![],
            display_name: Some("Claude".into()),
        };
        assert_eq!(agent_display_name(Some(&agent)), Some("Claude"));
    }

    #[test]
    fn agent_display_name_falls_back_to_command_basename() {
        let agent = AcpAgentConfig {
            command: "/usr/local/bin/claude-agent-acp".into(),
            args: vec![],
            env: vec![],
            display_name: None,
        };
        assert_eq!(agent_display_name(Some(&agent)), Some("claude-agent-acp"));
    }

    #[test]
    fn agent_display_name_none_when_no_agent() {
        assert_eq!(agent_display_name(None), None);
    }
}
