use alacritty_terminal::event::EventListener;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::vte::ansi::{Processor as VteProcessor, StdSyncHandler};
use alacritty_terminal::Term;
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use std::io;
use std::os::fd::RawFd;
use std::os::unix::process::CommandExt;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use winit::event_loop::EventLoopProxy;

use crate::config::Config;
use crate::term::osc133::{EraseScanner, Osc133Marker, Osc133Scanner};

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
    /// OSC 133 semantic prompt marker.
    Osc133(Osc133Marker),
    /// CSI 2 J (erase display) or CSI 3 J (erase scrollback) detected.
    /// Block decorations must be cleared.
    ScreenCleared,
}

/// Bridges alacritty_terminal events from Term's internal event listener to our
/// PtyEvent channel.  PtyWrite responses (cursor position, DA, etc.) are written
/// directly to the PTY master fd — no main-thread round-trip required.
#[derive(Clone)]
pub struct PtyEventProxy {
    pub tx: Sender<PtyEvent>,
    pub wakeup: EventLoopProxy<()>,
    /// Raw PTY master fd used for direct PtyWrite responses (cursor position, etc.).
    /// Written from whatever thread calls send_event — safe on Unix for PTY fds.
    pub master_fd: RawFd,
    pub(crate) qos_set: Arc<OnceLock<()>>,
}

impl EventListener for PtyEventProxy {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        use alacritty_terminal::event::Event;

        // Lower the PTY I/O thread to QOS_CLASS_UTILITY (efficiency cores) exactly once.
        #[cfg(target_os = "macos")]
        self.qos_set.get_or_init(|| {
            use libc::qos_class_t::QOS_CLASS_UTILITY;
            unsafe { libc::pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0) };
        });

        // PtyWrite responses (cursor position, DA, DECRQSS, etc.) must be forwarded
        // to the PTY immediately — without going through the main thread channel.
        if let Event::PtyWrite(text) = event {
            let bytes = text.as_bytes();
            if self.master_fd >= 0 && !bytes.is_empty() {
                unsafe {
                    libc::write(
                        self.master_fd,
                        bytes.as_ptr() as *const libc::c_void,
                        bytes.len(),
                    );
                }
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

/// A spawned PTY with a running reader thread and child monitor.
pub struct Pty {
    /// Receive events from the PTY reader thread.
    pub rx: crossbeam_channel::Receiver<PtyEvent>,
    /// Sender side — used to inject synthetic events (e.g. ClipboardLoad response).
    pub tx: crossbeam_channel::Sender<PtyEvent>,
    /// PID of the shell child process (for CWD resolution).
    pub child_pid: u32,

    /// PTY master fd — used for write() and resize().
    /// -1 after shutdown.
    master_fd: RawFd,
    /// Child PID as libc type for SIGHUP on drop.
    child_pid_libc: libc::pid_t,
    /// Reader thread: reads PTY bytes, advances VTE, emits DataReady/Osc133.
    reader_thread: Option<JoinHandle<()>>,
    /// Child monitor thread: waits for child exit, emits PtyEvent::Exit.
    child_thread: Option<JoinHandle<()>>,
}

impl Pty {
    /// Spawn a new PTY running the configured shell, along with the
    /// alacritty_terminal `Term` that processes its output.
    ///
    /// Returns `(Pty, Arc<FairMutex<Term<PtyEventProxy>>>)`.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        config: &Config,
        cols: u16,
        rows: u16,
        cell_width: u16,
        cell_height: u16,
        wakeup: EventLoopProxy<()>,
        working_directory: Option<std::path::PathBuf>,
        term_config: alacritty_terminal::term::Config,
        term_size: &crate::term::TermSize,
    ) -> Result<(Self, Arc<FairMutex<Term<PtyEventProxy>>>)> {
        // ── 1. Open PTY pair ───────────────────────────────────────────────
        let (master_fd, slave_fd) =
            unsafe { open_pty(cols, rows, cell_width, cell_height) }.context("openpty failed")?;

        // ── 2. Set UTF-8 mode on master ───────────────────────────────────
        #[cfg(target_os = "macos")]
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(master_fd, &mut t) == 0 {
                t.c_iflag |= libc::IUTF8;
                libc::tcsetattr(master_fd, libc::TCSANOW, &t);
            }
        }

        // ── 3. Create event channel ───────────────────────────────────────
        let (tx, rx) = crossbeam_channel::bounded::<PtyEvent>(1024);

        // ── 4. Build PtyEventProxy ────────────────────────────────────────
        let proxy = PtyEventProxy {
            tx: tx.clone(),
            wakeup: wakeup.clone(),
            master_fd,
            qos_set: Arc::new(OnceLock::new()),
        };

        // ── 5. Create alacritty Term ──────────────────────────────────────
        let term = Arc::new(FairMutex::new(Term::new(term_config, term_size, proxy)));

        // ── 6. Spawn shell process ────────────────────────────────────────
        let child = unsafe { spawn_shell(config, slave_fd, master_fd, working_directory)? };
        let child_pid = child.id();
        let child_pid_libc = child_pid as libc::pid_t;

        // Close slave in parent — child has its own copies via dup2.
        // Without closing, read(master_fd) never gets EIO after shell exits.
        unsafe { libc::close(slave_fd) };

        // ── 7. Start reader thread ────────────────────────────────────────
        let term_clone = Arc::clone(&term);
        let tx_clone = tx.clone();
        let wakeup_clone = wakeup.clone();
        let reader_thread = std::thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || reader_loop(master_fd, term_clone, tx_clone, wakeup_clone))
            .context("failed to spawn pty reader thread")?;

        // ── 8. Start child monitor thread ─────────────────────────────────
        let tx_clone2 = tx.clone();
        let wakeup_clone2 = wakeup.clone();
        let child_thread = std::thread::Builder::new()
            .name("pty-child".into())
            .spawn(move || {
                // Blocking wait for child exit.
                let code = {
                    // We only have a &Child here (via id), so use waitpid.
                    let mut status: libc::c_int = 0;
                    unsafe {
                        libc::waitpid(child_pid_libc, &mut status, 0);
                    }
                    if libc::WIFEXITED(status) {
                        libc::WEXITSTATUS(status)
                    } else {
                        -1
                    }
                };
                log::info!("PTY child exited with code {code}");
                drop(child); // explicit: ensures child is cleaned up
                let _ = tx_clone2.try_send(PtyEvent::Exit);
                let _ = wakeup_clone2.send_event(());
            })
            .context("failed to spawn pty child monitor")?;

        log::info!("PTY spawned: shell={} pid={child_pid}", config.shell);
        Ok((
            Self {
                rx,
                tx,
                child_pid,
                master_fd,
                child_pid_libc,
                reader_thread: Some(reader_thread),
                child_thread: Some(child_thread),
            },
            term,
        ))
    }

    /// Write raw bytes to the PTY (keyboard input → shell).
    pub fn write(&self, data: &[u8]) {
        if self.master_fd >= 0 && !data.is_empty() {
            unsafe {
                libc::write(
                    self.master_fd,
                    data.as_ptr() as *const libc::c_void,
                    data.len(),
                );
            }
        }
    }

    /// Resize the PTY to new terminal dimensions.
    pub fn resize(&self, cols: u16, rows: u16, cell_width: u16, cell_height: u16) {
        if self.master_fd < 0 {
            return;
        }
        let ws = libc::winsize {
            ws_col: cols,
            ws_row: rows,
            ws_xpixel: cell_width.saturating_mul(cols),
            ws_ypixel: cell_height.saturating_mul(rows),
        };
        unsafe {
            libc::ioctl(self.master_fd, libc::TIOCSWINSZ as libc::c_ulong, &ws);
        }
    }

    /// Cleanly shut down the PTY: close master fd, signal child, join threads.
    pub fn shutdown(&mut self) {
        self.close_master();
        // SIGHUP triggers PTY hangup, causing shell to exit.
        unsafe {
            libc::kill(self.child_pid_libc, libc::SIGHUP);
        }
        if let Some(h) = self.reader_thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self.child_thread.take() {
            let _ = h.join();
        }
    }

    fn close_master(&mut self) {
        if self.master_fd >= 0 {
            unsafe { libc::close(self.master_fd) };
            self.master_fd = -1;
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.close_master();
    }
}

// ── PTY creation ─────────────────────────────────────────────────────────────

/// Open a PTY pair and set the initial window size.
/// Returns (master_fd, slave_fd).
unsafe fn open_pty(
    cols: u16,
    rows: u16,
    cell_width: u16,
    cell_height: u16,
) -> io::Result<(RawFd, RawFd)> {
    let ws = libc::winsize {
        ws_col: cols,
        ws_row: rows,
        ws_xpixel: cell_width.saturating_mul(cols),
        ws_ypixel: cell_height.saturating_mul(rows),
    };
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let ret = libc::openpty(
        &mut master,
        &mut slave,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        &ws as *const libc::winsize as *mut libc::winsize,
    );
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((master, slave))
}

/// Spawn the shell process with slave_fd as its controlling terminal.
///
/// SAFETY: caller must ensure slave_fd and master_fd are valid open fds.
unsafe fn spawn_shell(
    config: &Config,
    slave_fd: RawFd,
    master_fd: RawFd,
    working_directory: Option<std::path::PathBuf>,
) -> Result<std::process::Child> {
    use std::os::fd::FromRawFd;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(&config.shell);
    cmd.arg("-l");

    // Connect stdin/stdout/stderr to the slave PTY.
    // We dup slave_fd for stdout and stderr to avoid double-close.
    let stdin_fd = libc::dup(slave_fd);
    let stdout_fd = libc::dup(slave_fd);
    if stdin_fd < 0 || stdout_fd < 0 {
        return Err(anyhow::anyhow!(
            "dup failed: {}",
            io::Error::last_os_error()
        ));
    }
    cmd.stdin(Stdio::from_raw_fd(stdin_fd));
    cmd.stdout(Stdio::from_raw_fd(stdout_fd));
    cmd.stderr(Stdio::from_raw_fd(slave_fd)); // slave_fd consumed here

    // Environment variables.
    cmd.env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("TERM_PROGRAM", "PetruTerm");

    // Working directory.
    if let Some(dir) = working_directory.or_else(dirs::home_dir) {
        cmd.current_dir(dir);
    }

    // pre_exec runs in the child process after fork, before exec.
    cmd.pre_exec(move || {
        // New session so the child becomes a session leader.
        if libc::setsid() < 0 {
            return Err(io::Error::last_os_error());
        }
        // Set the slave as the controlling terminal.
        libc::ioctl(slave_fd, libc::TIOCSCTTY as libc::c_ulong, 0 as libc::c_int);
        // Close extra copies of PTY fds in the child — stdin/stdout/stderr
        // were already dup2'd by the Command implementation.
        libc::close(slave_fd);
        libc::close(master_fd);
        // Restore default signal handlers.
        for sig in [
            libc::SIGCHLD,
            libc::SIGHUP,
            libc::SIGINT,
            libc::SIGQUIT,
            libc::SIGTERM,
            libc::SIGALRM,
        ] {
            libc::signal(sig, libc::SIG_DFL);
        }
        Ok(())
    });

    cmd.spawn().context("failed to spawn shell")
}

// ── Reader thread ─────────────────────────────────────────────────────────────

/// Blocking PTY reader loop.  Runs on its own thread.
/// Reads raw bytes from master_fd, scans for OSC 133 markers, advances the VTE
/// processor (which updates the terminal grid), and sends DataReady events.
fn reader_loop(
    master_fd: RawFd,
    term: Arc<FairMutex<Term<PtyEventProxy>>>,
    tx: Sender<PtyEvent>,
    wakeup: EventLoopProxy<()>,
) {
    let mut processor: VteProcessor<StdSyncHandler> = VteProcessor::new();
    let mut scanner = Osc133Scanner::new();
    let mut erase_scanner = EraseScanner::new();
    let mut buf = vec![0u8; 0x10_0000]; // 1 MiB read buffer

    loop {
        // Blocking read from PTY master.
        let n = unsafe { libc::read(master_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };

        if n <= 0 {
            // 0 = EOF, -1 = EIO (shell exited) or other error.
            break;
        }
        let n = n as usize;
        let bytes = &buf[..n];

        // OSC 133 + erase scan — must run before VTE advance.
        for &b in bytes {
            if let Some(marker) = scanner.scan(b) {
                let _ = tx.try_send(PtyEvent::Osc133(marker));
            }
            if erase_scanner.scan(b) {
                let _ = tx.try_send(PtyEvent::ScreenCleared);
            }
        }

        // Advance VTE parser → updates Term grid.
        {
            let mut term_lock = term.lock_unfair();
            processor.advance(&mut *term_lock, bytes);
        }

        // Notify main thread that new data is available.
        let _ = tx.try_send(PtyEvent::DataReady);
        let _ = wakeup.send_event(());
    }
    log::debug!("PTY reader thread exiting");
}
