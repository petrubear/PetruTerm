mod app;
mod config;
mod font;
mod renderer;
mod term;
mod ui;

use anyhow::Result;
use winit::event_loop::{ControlFlow, EventLoop};

use app::App;

fn main() -> Result<()> {
    // Initialize logging. RUST_LOG env var controls level; default to "info".
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

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

    let event_loop = EventLoop::new()?;
    // Poll mode: we drive redraws from PTY events and input, not OS events only.
    event_loop.set_control_flow(ControlFlow::Poll);

    // Proxy lets PTY background threads wake the winit event loop immediately
    // (e.g. on shell exit) without waiting for the next WaitUntil blink timer.
    let wakeup_proxy = event_loop.create_proxy();

    let mut app = App::new(config, wakeup_proxy);
    event_loop.run_app(&mut app)?;

    log::info!("PetruTerm exiting.");
    Ok(())
}
