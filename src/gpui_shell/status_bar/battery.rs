// gpui chrome migration (post-M5 dogfood): the battery widget, reported
// live as missing from gpui_shell's status bar even though `StatusBar::
// build` (this module's own parent) has always accepted a `battery`
// parameter and rendered it -- `render.rs`'s call site just hardcoded
// `None`. `src/platform/battery.rs` (IOKit FFI, `BatteryStatus { on_battery,
// percent }`) is already binary-agnostic; this only wires it into
// gpui_shell's own poll loop and caches the result the same way `git.rs`
// caches a git-branch fetch.
//
// Unlike git branch, no async bridge is needed: `platform::battery::query()`
// is a synchronous, local IOKit call (no subprocess, no blocking I/O), the
// same reason the wgpu build's own `poll_low_freq_tasks`
// (`src/app/mod.rs:1614`) calls it directly rather than spawning it.

use std::time::{Duration, Instant};

/// Cached battery poll state, living as a plain field on `GpuiShellRoot`
/// (matching `GitBranchState`'s own shape).
#[derive(Default)]
pub struct BatteryState {
    /// `(percent, on_battery)`, mirroring `StatusBar::build`'s own
    /// parameter shape. `None` until the first poll, or permanently on a
    /// desktop Mac / non-macOS platform with no battery to report.
    pub cache: Option<(u8, bool)>,
    last_poll: Option<Instant>,
}

/// Whether `poll_battery` should query IOKit again this tick -- true on the
/// very first call (`last_poll` still `None`) or once `ttl` has elapsed.
/// Extracted as a pure function (same shape as `git.rs`'s own
/// `should_spawn_fetch`) so the TTL decision is unit-testable without a
/// real IOKit call.
fn should_poll(state: &BatteryState, now: Instant, ttl: Duration) -> bool {
    state
        .last_poll
        .map(|t| now.duration_since(t) >= ttl)
        .unwrap_or(true)
}

/// Call once per poll-loop tick. Queries `platform::battery::query()` at
/// most once every `ttl` (immediately on the first call), and returns
/// whether `state.cache` changed (caller should redraw). A desktop Mac /
/// non-macOS `None` result still updates `last_poll` -- it's cached as
/// "no battery" rather than retried every tick forever.
pub fn poll_battery(state: &mut BatteryState, now: Instant, ttl: Duration) -> bool {
    if !should_poll(state, now, ttl) {
        return false;
    }
    state.last_poll = Some(now);
    let fresh = crate::platform::battery::query().map(|s| (s.percent, s.on_battery));
    if fresh == state.cache {
        return false;
    }
    state.cache = fresh;
    true
}

#[cfg(test)]
mod tests {
    use super::{should_poll, BatteryState};
    use std::time::{Duration, Instant};

    #[test]
    fn polls_on_first_call() {
        let state = BatteryState::default();
        assert!(should_poll(&state, Instant::now(), Duration::from_secs(30)));
    }

    #[test]
    fn does_not_poll_before_ttl_expires() {
        let now = Instant::now();
        let state = BatteryState {
            cache: Some((80, true)),
            last_poll: Some(now),
        };
        assert!(!should_poll(
            &state,
            now + Duration::from_secs(10),
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn polls_again_once_ttl_expires() {
        let now = Instant::now();
        let state = BatteryState {
            cache: Some((80, true)),
            last_poll: Some(now),
        };
        assert!(should_poll(
            &state,
            now + Duration::from_secs(31),
            Duration::from_secs(30)
        ));
    }
}
