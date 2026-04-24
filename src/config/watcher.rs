use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Watches the config directory for changes and sends the changed path over a channel.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    pub rx: mpsc::Receiver<PathBuf>,
}

impl ConfigWatcher {
    pub fn new(config_dir: &Path) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(1);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    for path in event.paths {
                        if path.extension().is_some_and(|e| e == "lua" || e == "json") {
                            let _ = tx.try_send(path);
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

    /// Non-blocking check for pending change events. Returns the first changed path if any.
    pub fn poll(&self) -> Option<PathBuf> {
        // Drain all events, return the last one (or first unique path seen)
        let mut changed = None;
        while let Ok(path) = self.rx.try_recv() {
            changed = Some(path);
        }
        changed
    }

    /// Blocking wait for a change event, with timeout.
    #[allow(dead_code)]
    pub fn wait_timeout(&self, timeout: Duration) -> Option<PathBuf> {
        self.rx.recv_timeout(timeout).ok()
    }
}
