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

    /// Non-blocking check for pending change events. The channel has capacity 1
    /// and the watcher uses `try_send`, so this returns the first path changed
    /// since the last poll; later events before the next poll are dropped.
    pub fn poll(&self) -> Option<PathBuf> {
        let mut changed = None;
        while let Ok(path) = self.rx.try_recv() {
            changed = Some(path);
        }
        changed
    }

    /// Blocking wait for a change event, with timeout.
    // gpui-petruterm only (gpui_shell/config_watch.rs); main.rs's mod tree never calls it.
    #[allow(dead_code)]
    pub fn wait_timeout(&self, timeout: Duration) -> Option<PathBuf> {
        self.rx.recv_timeout(timeout).ok()
    }
}
