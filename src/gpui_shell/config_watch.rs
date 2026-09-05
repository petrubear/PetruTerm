// gpui chrome migration (M2 Task 5b): config-reload watcher thread + the
// cross-thread hand-off statics `GpuiShellRoot`'s poll loop (`poll.rs`)
// drains each tick. Split out of `mod.rs` for the 400-line convention.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::config::Config;

/// Shared between the config-watcher thread and `GpuiShellRoot`'s poll/wake
/// loop: `Some(config)` once a reload has happened and hasn't been applied
/// yet. `Mutex` because construction and consumption happen on different
/// threads; contention is negligible (checked at most ~30Hz, written only on
/// an actual file change).
pub(super) static PENDING_CONFIG_RELOAD: Mutex<Option<Config>> = Mutex::new(None);
pub(super) static CONFIG_CHANGED: AtomicBool = AtomicBool::new(false);

/// Spawn a dedicated thread running `ConfigWatcher`'s blocking watch loop
/// (the same notify-based watcher the wgpu `petruterm` binary uses), and
/// hand reloaded configs to `GpuiShellRoot`'s poll loop via
/// `PENDING_CONFIG_RELOAD`/`CONFIG_CHANGED` — gpui has no cross-thread wake
/// bridge in this version (see `spawn_terminal`'s doc comment), so pushing
/// data directly into gpui from this thread isn't an option.
///
/// Call exactly once, at startup (`main()`, alongside `terminal_element::
/// set_font_config`) — not from `GpuiShellRoot::new`. `PENDING_CONFIG_RELOAD`/
/// `CONFIG_CHANGED` are process-global statics; a second call (e.g. one per
/// window, if this app ever opens more than one) would spawn a second
/// watcher thread racing the first over the same slot, with no guarantee
/// either window's poll loop sees every update.
pub fn spawn_config_watcher() {
    std::thread::spawn(|| {
        let watcher = match crate::config::watcher::ConfigWatcher::new(&crate::config::config_dir())
        {
            Ok(w) => w,
            Err(e) => {
                log::error!("gpui-shell: failed to start config watcher: {e:#}");
                return;
            }
        };
        loop {
            if watcher
                .wait_timeout(std::time::Duration::from_secs(3600))
                .is_some()
            {
                match crate::config::reload() {
                    Ok((config, _lua)) => {
                        *PENDING_CONFIG_RELOAD.lock().unwrap() = Some(config);
                        CONFIG_CHANGED.store(true, Ordering::Release);
                    }
                    Err(e) => log::error!("gpui-shell: config reload failed: {e:#}"),
                }
            }
        }
    });
}
