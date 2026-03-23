use anyhow::{Context, Result};
use alacritty_terminal::event::WindowSize;
use alacritty_terminal::event_loop::{EventLoop as PtyEventLoop, Msg, Notifier};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};
use alacritty_terminal::Term;
use alacritty_terminal::sync::FairMutex;
use crossbeam_channel::Sender;
use std::sync::Arc;

use crate::config::Config;

impl EventListener for PtyEventProxy {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        use alacritty_terminal::event::Event;
        let pty_event = match event {
            Event::Wakeup            => PtyEvent::DataReady,
            Event::Exit              => PtyEvent::Exit,
            Event::Title(t)          => PtyEvent::TitleChanged(t),
            Event::Bell              => PtyEvent::Bell,
            _                        => return,
        };
        let _ = self.tx.send(pty_event);
    }
}

/// Events emitted by the PTY reader thread to the main thread.
#[derive(Debug)]
pub enum PtyEvent {
    /// New data arrived; terminal grid has been updated.
    DataReady,
    /// The shell process exited.
    Exit,
    /// Terminal title changed (OSC 0/2).
    TitleChanged(String),
    /// Bell character received.
    Bell,
}

/// Bridges alacritty_terminal events to our PtyEvent channel.
#[derive(Clone)]
pub struct PtyEventProxy {
    pub tx: Sender<PtyEvent>,
}

/// A spawned PTY with a running alacritty_terminal I/O thread.
pub struct Pty {
    /// Send data to the PTY (keyboard input → shell).
    pub notifier: Notifier,
    /// Receive events from the PTY reader thread.
    pub rx: crossbeam_channel::Receiver<PtyEvent>,
    /// Kept alive to ensure the PTY thread is not prematurely dropped.
    /// Type-erased because alacritty's EventLoop return type is complex.
    _thread: Box<dyn std::any::Any + Send>,
}

impl Pty {
    /// Spawn a new PTY running the configured shell.
    pub fn spawn(
        config: &Config,
        term: Arc<FairMutex<Term<PtyEventProxy>>>,
    ) -> Result<Self> {
        let (tx, rx) = crossbeam_channel::unbounded::<PtyEvent>();
        let proxy = PtyEventProxy { tx };

        let pty_options = PtyOptions {
            shell: Some(Shell::new(config.shell.clone(), vec!["-l".into()])),
            working_directory: None,
            drain_on_exit: false,
            env: Default::default(),
        };

        let window_size = WindowSize {
            num_cols:    80,
            num_lines:   24,
            cell_width:  8,
            cell_height: 16,
        };

        let pty = tty::new(&pty_options, window_size, 0)
            .context("Failed to spawn PTY")?;

        let pty_event_loop = PtyEventLoop::new(
            Arc::clone(&term),
            proxy,
            pty,
            false, // drain_on_exit
            false, // ref_test
        ).context("Failed to create PTY event loop")?;

        let notifier = Notifier(pty_event_loop.channel());
        let thread = Box::new(pty_event_loop.spawn());

        log::info!("PTY spawned: shell={}", config.shell);
        Ok(Self { notifier, rx, _thread: thread })
    }

    /// Write raw bytes to the PTY (keyboard input → shell).
    pub fn write(&self, data: &[u8]) {
        let bytes = data.to_vec();
        let _ = self.notifier.0.send(Msg::Input(bytes.into()));
    }

    /// Resize the PTY to new terminal dimensions.
    pub fn resize(&self, cols: u16, rows: u16) {
        let window_size = WindowSize {
            num_cols:    cols,
            num_lines:   rows,
            cell_width:  8,
            cell_height: 16,
        };
        let _ = self.notifier.0.send(Msg::Resize(window_size));
    }
}
