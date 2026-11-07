//! `petruterm agents setup|remove [--dry-run]`: install (or remove) standing instructions
//! that tell Claude Code (a skill) and Kiro (a steering file) how to answer agent messages.
//! Never touches a file it did not create: files carry a marker line, an existing file without
//! it is left alone, and `remove` deletes only marked files. See
//! `specs/002-agent-messaging/contracts/agents-setup-cli.md`.

use std::path::{Path, PathBuf};

const CLAUDE_TEMPLATE: &str = include_str!("../../config/default/agent-messaging/claude-skill.md");
const KIRO_TEMPLATE: &str = include_str!("../../config/default/agent-messaging/kiro-steering.md");
const MARKER: &str = "petruterm-managed";
const BACKUP_SUFFIX: &str = ".petruterm-bak";

pub struct Opts {
    pub dry_run: bool,
    pub claude_skills_dir: PathBuf,
    pub kiro_steering_dir: PathBuf,
}

struct Target {
    label: &'static str,
    path: PathBuf,
    template: &'static str,
}

fn targets(o: &Opts) -> [Target; 2] {
    [
        Target {
            label: "Claude Code skill",
            path: o.claude_skills_dir.join("petruterm-messaging/SKILL.md"),
            template: CLAUDE_TEMPLATE,
        },
        Target {
            label: "Kiro steering file",
            path: o.kiro_steering_dir.join("petruterm-messaging.md"),
            template: KIRO_TEMPLATE,
        },
    ]
}

/// Result of acting on one target: a report line and whether it is a problem.
type Step = (String, bool);

fn backup_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(BACKUP_SUFFIX);
    PathBuf::from(s)
}

fn is_managed(p: &Path) -> bool {
    std::fs::read_to_string(p).is_ok_and(|c| c.contains(MARKER))
}

fn install(t: &Target, dry: bool) -> Step {
    let shown = t.path.display();
    match std::fs::read_to_string(&t.path) {
        Ok(existing) if existing == t.template => {
            (format!("{}: already installed ({shown})", t.label), false)
        }
        Ok(existing) if existing.contains(MARKER) => {
            if dry {
                return (format!("{}: would update ({shown})", t.label), false);
            }
            let bak = backup_path(&t.path);
            let done = std::fs::copy(&t.path, &bak)
                .and_then(|_| std::fs::write(&t.path, t.template))
                .is_ok();
            if done {
                (
                    format!(
                        "{}: updated ({shown}), previous copy in {}",
                        t.label,
                        bak.display()
                    ),
                    false,
                )
            } else {
                (format!("{}: cannot write {shown}", t.label), true)
            }
        }
        Ok(_) => (
            format!(
                "{}: {shown} already exists and is not managed by petruterm; leaving it alone \
(move it away to install)",
                t.label
            ),
            true,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if dry {
                return (format!("{}: would create {shown}", t.label), false);
            }
            let done = t
                .path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| std::fs::write(&t.path, t.template))
                .is_ok();
            if done {
                (format!("{}: created {shown}", t.label), false)
            } else {
                (format!("{}: cannot write {shown}", t.label), true)
            }
        }
        Err(_) => (format!("{}: cannot read {shown}", t.label), true),
    }
}

fn uninstall(t: &Target, dry: bool) -> Step {
    let shown = t.path.display();
    if !t.path.exists() {
        return (format!("{}: not installed", t.label), false);
    }
    if !is_managed(&t.path) {
        return (
            format!(
                "{}: {shown} is not managed by petruterm; leaving it alone",
                t.label
            ),
            false,
        );
    }
    if dry {
        return (format!("{}: would remove {shown}", t.label), false);
    }
    let bak = backup_path(&t.path);
    if is_managed(&bak) {
        let _ = std::fs::remove_file(&bak);
    }
    if std::fs::remove_file(&t.path).is_err() {
        return (format!("{}: cannot remove {shown}", t.label), true);
    }
    // the skill lives in its own directory; remove it only if it is now empty
    if let Some(dir) = t.path.parent() {
        if dir.file_name().is_some_and(|n| n == "petruterm-messaging") {
            let _ = std::fs::remove_dir(dir);
        }
    }
    (format!("{}: removed {shown}", t.label), false)
}

/// Run `setup` or `remove`; returns the report lines and whether anything went wrong.
pub fn apply(remove: bool, o: &Opts) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut failed = false;
    for t in targets(o) {
        let (line, bad) = if remove {
            uninstall(&t, o.dry_run)
        } else {
            install(&t, o.dry_run)
        };
        lines.push(line);
        failed |= bad;
    }
    (lines, failed)
}

const USAGE: &str = "usage: petruterm agents setup|remove [--dry-run] [--claude-skills-dir PATH] [--kiro-steering-dir PATH]";

/// Entry point for `petruterm agents ...`.
pub fn run(args: &[String]) -> i32 {
    let remove = match args.first().map(String::as_str) {
        Some("setup") => false,
        Some("remove") => true,
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    let home = dirs::home_dir().unwrap_or_default();
    let mut opts = Opts {
        dry_run: false,
        claude_skills_dir: home.join(".claude/skills"),
        kiro_steering_dir: home.join(".kiro/steering"),
    };
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dry-run" => opts.dry_run = true,
            "--claude-skills-dir" => match it.next() {
                Some(p) => opts.claude_skills_dir = PathBuf::from(p),
                None => {
                    eprintln!("{USAGE}");
                    return 2;
                }
            },
            "--kiro-steering-dir" => match it.next() {
                Some(p) => opts.kiro_steering_dir = PathBuf::from(p),
                None => {
                    eprintln!("{USAGE}");
                    return 2;
                }
            },
            _ => {
                eprintln!("{USAGE}");
                return 2;
            }
        }
    }
    let (lines, failed) = apply(remove, &opts);
    for l in lines {
        println!("{l}");
    }
    i32::from(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(dir: &Path, dry: bool) -> Opts {
        Opts {
            dry_run: dry,
            claude_skills_dir: dir.join("claude/skills"),
            kiro_steering_dir: dir.join("kiro/steering"),
        }
    }

    #[test]
    fn templates_carry_the_marker_and_the_reply_command() {
        for t in [CLAUDE_TEMPLATE, KIRO_TEMPLATE] {
            assert!(t.contains(MARKER));
            assert!(t.contains("petruterm msg <name>"));
            assert!(t.contains("not by the user"));
        }
        assert!(CLAUDE_TEMPLATE.starts_with("---\nname: petruterm-messaging\n"));
        assert!(KIRO_TEMPLATE.starts_with("---\ninclusion: always\n"));
    }

    #[test]
    fn fresh_install_creates_both_and_second_run_changes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let o = opts(d.path(), false);
        let (l1, bad1) = apply(false, &o);
        assert!(!bad1, "{l1:?}");
        assert!(l1.iter().all(|l| l.contains("created")));
        assert!(o
            .claude_skills_dir
            .join("petruterm-messaging/SKILL.md")
            .is_file());
        assert!(o.kiro_steering_dir.join("petruterm-messaging.md").is_file());
        let (l2, bad2) = apply(false, &o);
        assert!(!bad2);
        assert!(l2.iter().all(|l| l.contains("already installed")), "{l2:?}");
    }

    #[test]
    fn dry_run_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let o = opts(d.path(), true);
        let (l, bad) = apply(false, &o);
        assert!(!bad);
        assert!(l.iter().all(|x| x.contains("would create")));
        assert!(!d.path().join("claude").exists());
    }

    #[test]
    fn a_users_own_file_is_never_touched() {
        let d = tempfile::tempdir().unwrap();
        let o = opts(d.path(), false);
        let theirs = o.kiro_steering_dir.join("petruterm-messaging.md");
        std::fs::create_dir_all(&o.kiro_steering_dir).unwrap();
        std::fs::write(&theirs, "my own notes").unwrap();
        let (l, bad) = apply(false, &o);
        assert!(bad, "an unmanaged file at the target is reported");
        assert!(l.iter().any(|x| x.contains("not managed by petruterm")));
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "my own notes");
        let (_, bad) = apply(true, &o);
        assert!(!bad);
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "my own notes");
    }

    #[test]
    fn an_older_managed_file_is_updated_with_a_backup() {
        let d = tempfile::tempdir().unwrap();
        let o = opts(d.path(), false);
        let p = o.claude_skills_dir.join("petruterm-messaging/SKILL.md");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, format!("old <!-- {MARKER} -->")).unwrap();
        let (l, bad) = apply(false, &o);
        assert!(!bad, "{l:?}");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), CLAUDE_TEMPLATE);
        assert!(backup_path(&p).is_file());
    }

    #[test]
    fn remove_deletes_only_what_was_installed() {
        let d = tempfile::tempdir().unwrap();
        let o = opts(d.path(), false);
        apply(false, &o);
        let other = o.claude_skills_dir.join("someone-elses/SKILL.md");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "x").unwrap();
        let (l, bad) = apply(true, &o);
        assert!(!bad && l.iter().all(|x| x.contains("removed")), "{l:?}");
        assert!(!o.claude_skills_dir.join("petruterm-messaging").exists());
        assert!(!o.kiro_steering_dir.join("petruterm-messaging.md").exists());
        assert!(other.is_file());
        let (l, _) = apply(true, &o);
        assert!(l.iter().all(|x| x.contains("not installed")));
    }

    #[test]
    fn usage_errors_exit_two() {
        assert_eq!(run(&[]), 2);
        assert_eq!(run(&["frobnicate".to_string()]), 2);
        assert_eq!(run(&["setup".to_string(), "--bogus".to_string()]), 2);
        assert_eq!(
            run(&["setup".to_string(), "--claude-skills-dir".to_string()]),
            2
        );
    }
}
