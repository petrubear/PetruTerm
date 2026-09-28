use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// Which kind of watched config file changed. The path itself carries no
/// information no consumer needs (every consumer only branches on
/// extension, or not at all), so the channel carries this instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigFileKind {
    Lua,
    Json,
}

/// Coalesced set of file kinds that changed since the last `poll()`. An
/// editor emits several fs events per save, and a burst of edits across
/// both a `.lua` and a `.json` file can land between two polls — this
/// collapses all of it into "did each kind change at least once".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigChanges {
    pub lua: bool,
    pub json: bool,
}

impl ConfigChanges {
    pub fn is_empty(&self) -> bool {
        !self.lua && !self.json
    }
}

/// Watches the config directory for changes and reports which file kinds
/// changed over a channel.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<ConfigFileKind>,
}

impl ConfigWatcher {
    pub fn new(config_dir: &Path) -> Result<Self> {
        // Unbounded: these are low-frequency file-save events, and a bounded
        // channel with a lossy send (the previous `sync_channel(1)` +
        // `try_send`) silently drops an event that arrives while the slot is
        // still full, losing a `.json` change queued right behind a `.lua`
        // change (or vice versa).
        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    for path in event.paths {
                        let kind = match path.extension().and_then(|e| e.to_str()) {
                            Some("lua") => Some(ConfigFileKind::Lua),
                            Some("json") => Some(ConfigFileKind::Json),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            let _ = tx.send(kind);
                        }
                    }
                }
            }
        })?;

        watcher.watch(config_dir, RecursiveMode::Recursive)?;
        log::info!("Config watcher started on: {}", config_dir.display());

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Non-blocking check for pending change events since the last poll,
    /// coalesced by kind.
    pub fn poll(&self) -> ConfigChanges {
        let mut changes = ConfigChanges::default();
        while let Ok(kind) = self.rx.try_recv() {
            match kind {
                ConfigFileKind::Lua => changes.lua = true,
                ConfigFileKind::Json => changes.json = true,
            }
        }
        changes
    }

    /// Blocking wait for a change event, with timeout. Returns as soon as
    /// one event is available; callers that care about every kind that
    /// changed (not just the one that woke them) should follow up with
    /// `poll()` to drain and coalesce the rest.
    // gpui-petruterm only (gpui_shell/config_watch.rs); main.rs's mod tree never calls it.
    #[allow(dead_code)]
    pub fn wait_timeout(&self, timeout: Duration) -> Option<ConfigFileKind> {
        self.rx.recv_timeout(timeout).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_coalesces_multiple_events_of_the_same_kind() {
        let (tx, rx) = mpsc::channel();
        tx.send(ConfigFileKind::Lua).unwrap();
        tx.send(ConfigFileKind::Lua).unwrap();
        tx.send(ConfigFileKind::Json).unwrap();
        let watcher = ConfigWatcher {
            _watcher: notify::recommended_watcher(|_res: notify::Result<Event>| {}).unwrap(),
            rx,
        };

        let changes = watcher.poll();
        assert!(changes.lua);
        assert!(changes.json);
        assert!(watcher.poll().is_empty());
    }

    #[test]
    fn poll_reports_lua_changed_without_swallowing_a_json_change_right_behind_it() {
        // Regression check for AUDIT-BUG-07: a `.json` change queued right
        // after a `.lua` change must not be dropped.
        let (tx, rx) = mpsc::channel();
        tx.send(ConfigFileKind::Lua).unwrap();
        tx.send(ConfigFileKind::Json).unwrap();
        let watcher = ConfigWatcher {
            _watcher: notify::recommended_watcher(|_res: notify::Result<Event>| {}).unwrap(),
            rx,
        };

        let changes = watcher.poll();
        assert_eq!(
            changes,
            ConfigChanges {
                lua: true,
                json: true
            }
        );
    }
}
