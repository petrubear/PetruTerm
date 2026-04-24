mod app;
mod config;
mod font;
mod i18n;
mod llm;
mod platform;
mod renderer;
mod term;
mod ui;

rust_i18n::i18n!("locales");

use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Result;
use winit::event_loop::{ControlFlow, EventLoop};
#[cfg(target_os = "macos")]
use winit::platform::macos::EventLoopBuilderExtMacOS as _;

use app::App;

/// Inherit environment variables from the user's login shell.
///
/// When launched as a .app bundle from Finder/Dock, macOS does not go through
/// a shell, so ~/.zshrc and ~/.zprofile are never sourced. API keys and other
/// env vars set there are invisible to the process. This function spawns the
/// user's shell in login mode (`-l -c 'env -0'`) and imports any variables that
/// are not already set in the current environment.
///
/// Must be called before any threads are spawned (set_var is not thread-safe).
#[cfg(target_os = "macos")]
fn inherit_login_shell_env() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let Ok(output) = std::process::Command::new(&shell)
        .args(["-l", "-c", "env -0"])
        .output()
    else {
        return;
    };
    for pair in output.stdout.split(|&b| b == 0) {
        if pair.is_empty() {
            continue;
        }
        let s = String::from_utf8_lossy(pair);
        if let Some((key, value)) = s.split_once('=') {
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

fn main() -> Result<()> {
    // Inherit login-shell env vars (API keys, PATH, etc.) when launched as a
    // .app bundle from Finder/Dock, where ~/.zshrc is never sourced.
    // Must run before any threads are spawned.
    #[cfg(target_os = "macos")]
    inherit_login_shell_env();

    // Initialize logging. RUST_LOG env var controls level; default to "info".
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Detect system locale and set rust-i18n accordingly.
    i18n::init();

    // When built with --features profiling, connect tracing spans to Tracy.
    // Run Tracy before launching PetruTerm; spans stream live over localhost.
    #[cfg(feature = "profiling")]
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        tracing_subscriber::registry()
            .with(tracing_tracy::TracyLayer::default())
            .init();
        log::info!("Tracy profiling subscriber active.");
    }

    log::info!("PetruTerm starting up");

    // Load config (copies defaults to ~/.config/petruterm/ on first launch).
    let config = match config::load() {
        Ok(c) => {
            log::info!("Config loaded successfully.");
            c
        }
        Err(e) => {
            log::warn!("Failed to load config ({e}); using defaults.");
            config::Config::default()
        }
    };

    // On macOS, disable the default application menu so muda can install its own.
    #[cfg(target_os = "macos")]
    let event_loop = EventLoop::<()>::with_user_event()
        .with_default_menu(false)
        .build()?;
    #[cfg(not(target_os = "macos"))]
    let event_loop = EventLoop::new()?;

    // about_to_wait sets Wait/WaitUntil each frame; PTY threads wake via wakeup_proxy.
    event_loop.set_control_flow(ControlFlow::Wait);

    // Proxy lets PTY background threads wake the winit event loop immediately
    // (e.g. on shell exit) without waiting for the next WaitUntil blink timer.
    let wakeup_proxy = event_loop.create_proxy();

    let mut app = App::new(config, wakeup_proxy);
    event_loop.run_app(&mut app)?;

    log::info!("PetruTerm exiting.");
    Ok(())
}
