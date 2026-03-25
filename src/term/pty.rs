use anyhow::{Context, Result};
use alacritty_terminal::event::WindowSize;
use alacritty_terminal::event_loop::{EventLoop as PtyEventLoop, Msg, Notifier};
use alacritty_terminal::event::EventListener;
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};
use alacritty_terminal::Term;
use alacritty_terminal::sync::FairMutex;
use crossbeam_channel::Sender;
use std::sync::{Arc, OnceLock};
use winit::event_loop::EventLoopProxy;

use crate::config::Config;
use dirs;

impl EventListener for PtyEventProxy {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        use alacritty_terminal::event::Event;

        // PtyWrite responses (cursor position, DA, DECRQSS, etc.) must be forwarded
        // to the PTY immediately — on the background thread, without going through the
        // main thread. Crossterm and other TUI apps time out (~2 s) waiting for replies
        // to queries like CSI 6 n. Routing through the main thread adds up to 530 ms
        // of latency (blink timer), which causes those timeouts.
        if let Event::PtyWrite(text) = event {
            // Write to /tmp/petruterm.log so we can diagnose from .app bundle
            // (stdout/stderr are /dev/null when launched from Finder).
            {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true).append(true).open("/tmp/petruterm.log")
                {
                    let _ = writeln!(f, "PtyWrite: {:?}", text);
                }
            }
            match self.direct_notifier.get() {
                Some(notifier) => {
                    if let Err(e) = notifier.0.send(Msg::Input(text.into_bytes().into())) {
                        use std::io::Write;
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true).append(true).open("/tmp/petruterm.log")
                        {
                            let _ = writeln!(f, "PtyWrite send FAILED: {e}");
                        }
                    }
                }
                None => {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true).append(true).open("/tmp/petruterm.log")
                    {
                        let _ = writeln!(f, "PtyWrite: direct_notifier not ready — DROPPED");
                    }
                }
            }
            return;
        }

        let pty_event = match event {
            Event::Wakeup               => PtyEvent::DataReady,
            Event::Exit | Event::ChildExit(_) => PtyEvent::Exit,
            Event::Title(t)             => PtyEvent::TitleChanged(t),
            Event::Bell                 => PtyEvent::Bell,
            Event::ClipboardStore(_, text) => PtyEvent::ClipboardStore(text),
            Event::ClipboardLoad(_, fmt)   => PtyEvent::ClipboardLoad(fmt),
            _                           => return,
        };
        if self.tx.send(pty_event).is_ok() {
            let _ = self.wakeup.send_event(());
        }
    }
}

/// Events emitted by the PTY reader thread to the main thread.
pub enum PtyEvent {
    /// New data arrived; terminal grid has been updated.
    DataReady,
    /// The shell process exited.
    Exit,
    /// Terminal title changed (OSC 0/2).
    TitleChanged(String),
    /// Bell character received.
    Bell,
    /// OSC 52 write — store this text in the system clipboard.
    ClipboardStore(String),
    /// OSC 52 read — read clipboard, apply formatter, write result to PTY.
    ClipboardLoad(std::sync::Arc<dyn Fn(&str) -> String + Send + Sync + 'static>),
    /// Terminal parser response that must be forwarded to the shell process.
    PtyWrite(String),
}

/// Bridges alacritty_terminal events to our PtyEvent channel.
/// Also holds an `EventLoopProxy` to wake the winit event loop immediately
/// when any PTY event (including Exit) is emitted by the background I/O thread.
///
/// `direct_notifier` is set once after the PTY event loop is created and is used
/// to forward PtyWrite responses (cursor position, DA, etc.) directly to the PTY
/// without a main-thread round-trip.
#[derive(Clone)]
pub struct PtyEventProxy {
    pub tx: Sender<PtyEvent>,
    pub wakeup: EventLoopProxy<()>,
    pub direct_notifier: Arc<OnceLock<Notifier>>,
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
        cols: u16,
        rows: u16,
        cell_width: u16,
        cell_height: u16,
        wakeup: EventLoopProxy<()>,
        // Shared with the Term's placeholder proxy so that PtyWrite events from
        // term.process() — which use Term's internal proxy — are forwarded immediately.
        direct_notifier: Arc<OnceLock<Notifier>>,
    ) -> Result<Self> {
        let (tx, rx) = crossbeam_channel::unbounded::<PtyEvent>();
        let proxy = PtyEventProxy { tx, wakeup, direct_notifier: Arc::clone(&direct_notifier) };

        let mut env = std::collections::HashMap::new();
        env.insert("TERM".into(),          "xterm-256color".into());
        env.insert("COLORTERM".into(),     "truecolor".into());
        env.insert("TERM_PROGRAM".into(),  "PetruTerm".into());

        let pty_options = PtyOptions {
            shell: Some(Shell::new(config.shell.clone(), vec!["-l".into()])),
            working_directory: dirs::home_dir(),
            drain_on_exit: false,
            env,
        };

        let window_size = WindowSize {
            num_cols:    cols,
            num_lines:   rows,
            cell_width,
            cell_height,
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
        // Give the proxy a direct path to write responses back to the PTY,
        // bypassing the main thread. Set before spawn so it's ready immediately.
        let _ = direct_notifier.set(Notifier(pty_event_loop.channel()));
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
    pub fn resize(&self, cols: u16, rows: u16, cell_width: u16, cell_height: u16) {
        let window_size = WindowSize {
            num_cols: cols,
            num_lines: rows,
            cell_width,
            cell_height,
        };
        let _ = self.notifier.0.send(Msg::Resize(window_size));
    }
}
