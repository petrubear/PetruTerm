use super::*;

fn row(pid: u32, ppid: u32, pgid: u32, tpgid: i64, args: &str) -> ProcessRow {
    ProcessRow {
        pid,
        ppid,
        pgid,
        tpgid,
        args: args.to_string(),
    }
}

fn probe(terminal_id: usize, shell_pid: u32) -> PaneProbe {
    PaneProbe {
        terminal_id,
        shell_pid,
    }
}

#[test]
fn parses_rows_and_skips_malformed_lines() {
    let text = "  100     1   100   100 -zsh\n  not a row\n  200   100   200   200 node /usr/lib/claude.js --x\n 300 1 300\n";
    let rows = parse_ps_rows(text);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].pid, 100);
    assert_eq!(rows[0].args, "-zsh");
    assert_eq!(rows[1].args, "node /usr/lib/claude.js --x");
}

#[test]
fn matches_direct_and_interpreter_scripts() {
    let agents = default_agents();
    let direct = row(1, 0, 1, 1, "/usr/local/bin/claude --resume");
    assert_eq!(match_agent(&direct, &agents).unwrap().name, "Claude Code");
    let node = row(2, 0, 2, 2, "node --no-warnings /opt/bin/codex.js run");
    assert_eq!(match_agent(&node, &agents).unwrap().name, "Codex");
    let kiro = row(3, 0, 3, 3, "kiro-cli chat");
    assert_eq!(match_agent(&kiro, &agents).unwrap().name, "Kiro");
    assert!(match_agent(&row(4, 0, 4, 4, "-zsh"), &agents).is_none());
    assert!(match_agent(&row(5, 0, 5, 5, "node server.js"), &agents).is_none());
}

#[test]
fn detects_foreground_agent_only() {
    // shell 100 with foreground group 200 (claude)
    let rows = vec![
        row(100, 1, 100, 200, "-zsh"),
        row(200, 100, 200, 200, "claude"),
    ];
    let found = detect(&rows, &[probe(7, 100)], &default_agents());
    assert_eq!(
        found,
        vec![AgentDetection {
            terminal_id: 7,
            agent: "Claude Code".into(),
            color: Some([0xf2, 0x8c, 0x28]),
            pid: 200,
            session: None,
        }]
    );
}

#[test]
fn plain_shell_yields_nothing() {
    let rows = vec![
        row(100, 1, 100, 100, "-zsh"),
        row(110, 100, 110, 100, "sleep 5"),
    ];
    assert!(detect(&rows, &[probe(1, 100)], &default_agents()).is_empty());
}

#[test]
fn deeper_descendant_in_background_group_is_ignored() {
    // agent runs under tmux: tmux is a direct child, claude is a grandchild in another group
    let rows = vec![
        row(100, 1, 100, 150, "-zsh"),
        row(150, 100, 150, 150, "tmux"),
        row(300, 150, 300, -1, "claude"),
    ];
    assert!(detect(&rows, &[probe(1, 100)], &default_agents()).is_empty());
}

#[test]
fn backgrounded_direct_child_counts_but_other_group_does_not() {
    let rows = vec![
        row(100, 1, 100, 100, "-zsh"),
        row(120, 100, 120, 100, "codex"),
        row(130, 999, 130, 100, "claude"),
    ];
    let found = detect(&rows, &[probe(1, 100)], &default_agents());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].agent, "Codex");
}

#[test]
fn two_panes_two_detections() {
    let rows = vec![
        row(100, 1, 100, 200, "-zsh"),
        row(200, 100, 200, 200, "claude"),
        row(101, 1, 101, 201, "-zsh"),
        row(201, 101, 201, 201, "kiro-cli"),
    ];
    let found = detect(&rows, &[probe(1, 100), probe(2, 101)], &default_agents());
    assert_eq!(found.len(), 2);
    assert_eq!(found[1].agent, "Kiro");
}

#[test]
fn missing_shell_row_is_skipped() {
    assert!(detect(&[], &[probe(1, 100)], &default_agents()).is_empty());
}

#[test]
fn merge_appends_and_replaces_by_name() {
    let user = vec![
        RecognizedAgent::new("Codex", &["codex2"]),
        RecognizedAgent::new("Gemini", &["gemini"]),
    ];
    let merged = merged_agents(default_agents(), &user);
    assert_eq!(merged.len(), default_agents().len() + 1);
    assert_eq!(
        merged.iter().find(|a| a.name == "Codex").unwrap().process,
        vec!["codex2"]
    );
    assert!(merged.iter().any(|a| a.name == "Gemini"));
    // a replacement without a color keeps the built-in color
    assert_eq!(
        merged.iter().find(|a| a.name == "Codex").unwrap().color,
        Some([0x4a, 0xde, 0x80])
    );
}

#[test]
fn parses_hex_colors() {
    assert_eq!(parse_hex_color("#ff8000"), Some([255, 128, 0]));
    assert_eq!(parse_hex_color("00ff7f"), Some([0, 255, 127]));
    assert_eq!(parse_hex_color("#fff"), None);
    assert_eq!(parse_hex_color("#gg0000"), None);
}

fn agent_row(id: usize, state: Option<AgentState>, at: Option<Instant>) -> AgentRow {
    AgentRow {
        terminal_id: id,
        agent: "x".into(),
        color: None,
        agent_pid: 0,
        accepting: false,
        msg_recent: false,
        custom_name: None,
        session: None,
        workspace_name: String::new(),
        tab_title: String::new(),
        pane_number: None,
        state,
        unread: false,
        message: None,
        activity: None,
        updated_at: at,
    }
}

#[test]
fn sorts_by_urgency_then_recency() {
    let now = Instant::now();
    let old = now - Duration::from_secs(30);
    let mut rows = vec![
        agent_row(1, None, None),
        agent_row(2, Some(AgentState::Done), Some(now)),
        agent_row(3, Some(AgentState::Working), Some(old)),
        agent_row(4, Some(AgentState::Working), Some(now)),
        agent_row(5, Some(AgentState::NeedsInput), Some(old)),
        agent_row(6, Some(AgentState::Idle), Some(now)),
    ];
    sort_rows(&mut rows);
    let ids: Vec<usize> = rows.iter().map(|r| r.terminal_id).collect();
    assert_eq!(ids, vec![5, 4, 3, 2, 6, 1]);
}

#[test]
fn formats_ages() {
    assert_eq!(format_age(Duration::from_secs(12)), "12s");
    assert_eq!(format_age(Duration::from_secs(180)), "3m");
    assert_eq!(format_age(Duration::from_secs(3600)), "1h");
    assert_eq!(format_age(Duration::from_secs(172800)), "2d");
}

#[test]
fn real_ps_output_parses_and_contains_this_process() {
    let out = std::process::Command::new("ps")
        .args(PS_ARGS)
        .output()
        .expect("ps runs");
    let rows = parse_ps_rows(&String::from_utf8_lossy(&out.stdout));
    let me = std::process::id();
    assert!(
        rows.iter().any(|r| r.pid == me),
        "own pid missing from ps rows"
    );
    assert!(rows.len() > 10);
}

#[test]
fn detection_carries_pid_and_session_from_args_only() {
    let id = "3f6c0a52-91d4-4b1e-8c3a-0d2f7e5b9a11";
    let rows = vec![
        row(100, 1, 100, 200, "-zsh"),
        row(200, 100, 200, 200, &format!("claude --resume {id}")),
    ];
    let found = detect(&rows, &[probe(1, 100)], &default_agents());
    assert_eq!(found[0].pid, 200);
    assert_eq!(found[0].session.as_deref(), Some(id));
    assert!(!format!("{:?}", found[0]).contains("--resume"));
}
