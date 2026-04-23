use alacritty_terminal::event::EventListener;
use alacritty_terminal::event::WindowSize;
use alacritty_terminal::event_loop::{EventLoop as PtyEventLoop, Msg, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};
use alacritty_terminal::Term;
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
#[cfg(target_os = "macos")]
use libc::{self, qos_class_t::QOS_CLASS_UTILITY};
use std::sync::{Arc, OnceLock};
use winit::event_loop::EventLoopProxy;

use crate::config::Config;
use dirs;

impl EventListener for PtyEventProxy {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        use alacritty_terminal::event::Event;

        // Lower the PTY I/O thread to QOS_CLASS_UTILITY (efficiency cores) exactly once.
        // send_event is always called from the alacritty_terminal background I/O thread,
        // so this OnceLock fires on the first event and never again.
        #[cfg(target_os = "macos")]
        self.qos_set.get_or_init(|| {
            // SAFETY: pthread_set_qos_class_self_np is safe to call from any thread.
            // QOS_CLASS_UTILITY steers the thread towards efficiency cores on Apple Silicon.
            unsafe {
                libc::pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0);
            }
        });

        // PtyWrite responses (cursor position, DA, DECRQSS, etc.) must be forwarded
        // to the PTY immediately — on the background thread, without going through the
        // main thread. Crossterm and other TUI apps time out (~2 s) waiting for replies
        // to queries like CSI 6 n. Routing through the main thread adds up to 530 ms
        // of latency (blink timer), which causes those timeouts.
        if let Event::PtyWrite(text) = event {
            if let Some(notifier) = self.direct_notifier.get() {
                let _ = notifier.0.send(Msg::Input(text.into_bytes().into()));
            }
            return;
        }

        let pty_event = match event {
            Event::Wakeup => PtyEvent::DataReady,
            Event::Exit | Event::ChildExit(_) => PtyEvent::Exit,
            Event::Title(t) => PtyEvent::TitleChanged(t),
            Event::Bell => PtyEvent::Bell,
            Event::ClipboardStore(_, text) => PtyEvent::ClipboardStore(text),
            Event::ClipboardLoad(_, fmt) => PtyEvent::ClipboardLoad(fmt),
            _ => return,
        };
        match self.tx.try_send(pty_event) {
            Ok(_) => {}
            Err(_) => log::debug!("pty_backpressure_hit: channel full, event dropped"),
        }
        let _ = self.wakeup.send_event(());
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
    #[allow(dead_code)]
    PtyWrite(String),
}

/// Bridges alacritty_terminal events to our PtyEvent channel.
/// Also holds an `EventLoopProxy` to wake the winit event loop immediately
/// when any PTY event (including Exit) is emitted by the background I/O thread.
///
/// `direct_notifier` is set once after the PTY event loop is created and is used
/// to forward PtyWrite responses (cursor position, DA, etc.) directly to the PTY
/// without a main-thread round-trip.
///
/// `qos_set` ensures the QoS class is lowered exactly once on the PTY I/O thread.
#[derive(Clone)]
pub struct PtyEventProxy {
    pub tx: Sender<PtyEvent>,
    pub wakeup: EventLoopProxy<()>,
    pub direct_notifier: Arc<OnceLock<Notifier>>,
    pub(crate) qos_set: Arc<OnceLock<()>>,
}

use std::any::Any;

pub trait PtyJoinHandle: Send {
    fn join(self: Box<Self>) -> Result<(), Box<dyn Any + Send>>;
}

impl<T: Send> PtyJoinHandle for std::thread::JoinHandle<T> {
    fn join(self: Box<Self>) -> Result<(), Box<dyn Any + Send>> {
        (*self).join().map(|_| ())
    }
}

/// A spawned PTY with a running alacritty_terminal I/O thread.
pub struct Pty {
    /// Send data to the PTY (keyboard input → shell).
    pub notifier: Notifier,
    /// Receive events from the PTY reader thread.
    pub rx: crossbeam_channel::Receiver<PtyEvent>,
    /// Sender side of the PTY event channel — used to inject synthetic events
    /// (e.g. ClipboardLoad response as PtyWrite) from background threads.
    pub tx: crossbeam_channel::Sender<PtyEvent>,
    /// Handle to the PTY reader thread for clean shutdown.
    thread_handle: Option<Box<dyn PtyJoinHandle>>,
    /// PID of the shell child process (captured before the EventLoop takes ownership).
    pub child_pid: u32,
}

impl Pty {
    /// Spawn a new PTY running the configured shell.
    #[allow(clippy::too_many_arguments)]
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
        working_directory: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        let (tx, rx) = crossbeam_channel::bounded::<PtyEvent>(1024);
        let tx_clone = tx.clone();
        let proxy = PtyEventProxy {
            tx,
            wakeup,
            direct_notifier: Arc::clone(&direct_notifier),
            qos_set: Arc::new(OnceLock::new()),
        };

        let mut env = std::collections::HashMap::new();
        env.insert("TERM".into(), "xterm-256color".into());
        env.insert("COLORTERM".into(), "truecolor".into());
        env.insert("TERM_PROGRAM".into(), "PetruTerm".into());

        let pty_options = PtyOptions {
            shell: Some(Shell::new(config.shell.clone(), vec!["-l".into()])),
            working_directory: working_directory.or_else(dirs::home_dir),
            drain_on_exit: false,
            env,
        };

        let window_size = WindowSize {
            num_cols: cols,
            num_lines: rows,
            cell_width,
            cell_height,
        };

        let pty = tty::new(&pty_options, window_size, 0).context("Failed to spawn PTY")?;

        // Capture child PID before the pty is consumed by the EventLoop.
        let child_pid = pty.child().id();

        let pty_event_loop = PtyEventLoop::new(
            Arc::clone(&term),
            proxy,
            pty,
            false, // drain_on_exit
            false, // ref_test
        )
        .context("Failed to create PTY event loop")?;

        let notifier = Notifier(pty_event_loop.channel());
        // Give the proxy a direct path to write responses back to the PTY,
        // bypassing the main thread. Set before spawn so it's ready immediately.
        let _ = direct_notifier.set(Notifier(pty_event_loop.channel()));

        let thread_handle = pty_event_loop.spawn();

        log::info!("PTY spawned: shell={} pid={}", config.shell, child_pid);
        Ok(Self {
            notifier,
            rx,
            tx: tx_clone,
            thread_handle: Some(Box::new(thread_handle)),
            child_pid,
        })
    }

    /// Cleanly shut down the PTY thread.
    pub fn shutdown(&mut self) {
        log::debug!("Shutting down PTY thread...");
        let _ = self.notifier.0.send(Msg::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
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
