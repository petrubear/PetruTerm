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

use crate::config::schema::BatterySaverMode;
use crate::platform::battery::BatteryStatus;

/// Cached battery poll state, living as a plain field on `GpuiShellRoot`
/// (matching `GitBranchState`'s own shape).
#[derive(Default)]
pub struct BatteryState {
    /// `None` until the first poll, or permanently on a desktop Mac /
    /// non-macOS platform with no battery to report. Holds the full
    /// `BatteryStatus` (not just `(percent, on_battery)`) so callers can
    /// also read `low_power_mode` -- e.g. the poll loop's own
    /// `battery_saver_active` computation.
    pub cache: Option<crate::platform::battery::BatteryStatus>,
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
    let fresh = crate::platform::battery::query();
    if fresh == state.cache {
        return false;
    }
    state.cache = fresh;
    true
}

/// Resolve `config.battery_saver` against the current cached battery status
/// -- same `Always`/`Never`/`Auto` semantics the wgpu app's own
/// `poll_low_freq_tasks` (`src/app/mod.rs`) uses. `Auto` triggers on either
/// `on_battery` or macOS Low Power Mode: `on_battery` alone would miss a
/// plugged-in laptop with Low Power Mode turned on by choice. Extracted as
/// a pure function (same shape as `should_poll` above) so it's
/// unit-testable without a real IOKit call.
pub fn resolve_battery_saver_active(mode: BatterySaverMode, cache: Option<BatteryStatus>) -> bool {
    match mode {
        BatterySaverMode::Always => true,
        BatterySaverMode::Never => false,
        BatterySaverMode::Auto => cache.is_some_and(|s| s.on_battery || s.low_power_mode),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_battery_saver_active, should_poll, BatteryState};
    use crate::config::schema::BatterySaverMode;
    use crate::platform::battery::BatteryStatus;
    use std::time::{Duration, Instant};

    fn status(percent: u8, on_battery: bool) -> BatteryStatus {
        BatteryStatus {
            on_battery,
            percent,
            low_power_mode: false,
        }
    }

    #[test]
    fn polls_on_first_call() {
        let state = BatteryState::default();
        assert!(should_poll(&state, Instant::now(), Duration::from_secs(30)));
    }

    #[test]
    fn does_not_poll_before_ttl_expires() {
        let now = Instant::now();
        let state = BatteryState {
            cache: Some(status(80, true)),
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
            cache: Some(status(80, true)),
            last_poll: Some(now),
        };
        assert!(should_poll(
            &state,
            now + Duration::from_secs(31),
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn always_is_active_with_no_battery_info_at_all() {
        assert!(resolve_battery_saver_active(BatterySaverMode::Always, None));
    }

    #[test]
    fn never_is_inactive_even_on_battery() {
        assert!(!resolve_battery_saver_active(
            BatterySaverMode::Never,
            Some(status(10, true))
        ));
    }

    #[test]
    fn auto_is_inactive_on_ac_with_low_power_mode_off() {
        assert!(!resolve_battery_saver_active(
            BatterySaverMode::Auto,
            Some(status(100, false))
        ));
    }

    #[test]
    fn auto_is_active_on_battery() {
        assert!(resolve_battery_saver_active(
            BatterySaverMode::Auto,
            Some(status(50, true))
        ));
    }

    #[test]
    fn auto_is_active_on_ac_with_low_power_mode_on() {
        let plugged_in_low_power = BatteryStatus {
            on_battery: false,
            percent: 100,
            low_power_mode: true,
        };
        assert!(resolve_battery_saver_active(
            BatterySaverMode::Auto,
            Some(plugged_in_low_power)
        ));
    }

    #[test]
    fn auto_is_inactive_with_no_battery_info() {
        assert!(!resolve_battery_saver_active(BatterySaverMode::Auto, None));
    }
}
