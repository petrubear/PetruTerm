pub mod pty;
pub mod color;

pub use pty::{Pty, PtyEvent, PtyEventProxy};

use anyhow::Result;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::Term;
use alacritty_terminal::sync::FairMutex;
use std::sync::Arc;

use crate::config::Config;

/// Minimal struct implementing `alacritty_terminal::grid::Dimensions`
/// so we can construct a `Term` without needing a full SizeInfo.
pub struct TermSize {
    pub cols: usize,
    pub rows: usize,
    pub scrollback: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows + self.scrollback
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// Wraps an alacritty_terminal::Term instance + its PTY for a single pane.
pub struct Terminal {
    /// The alacritty terminal state (grid, parser, scrollback).
    /// Shared with the PTY I/O thread via FairMutex.
    pub term: Arc<FairMutex<Term<PtyEventProxy>>>,
    /// The PTY (process I/O, input notifier).
    pub pty: Pty,
    /// Current terminal dimensions.
    pub cols: u16,
    pub rows: u16,
}

impl Terminal {
    /// Create a new terminal pane: initialize the grid and spawn the shell.
    pub fn new(config: &Config, cols: u16, rows: u16) -> Result<Self> {
        let (tx_placeholder, _rx) = crossbeam_channel::unbounded();
        let proxy = PtyEventProxy { tx: tx_placeholder };

        let term_config = TermConfig {
            scrolling_history: config.scrollback_lines as usize,
            ..Default::default()
        };

        let size = TermSize {
            cols: cols as usize,
            rows: rows as usize,
            scrollback: config.scrollback_lines as usize,
        };

        let term = Arc::new(FairMutex::new(Term::new(term_config, &size, proxy)));
        let pty = Pty::spawn(config, Arc::clone(&term))?;

        Ok(Self { term, pty, cols, rows })
    }

    /// Resize the terminal grid and PTY.
    pub fn resize(&mut self, cols: u16, rows: u16, scrollback: usize) {
        use alacritty_terminal::event::WindowSize;
        self.cols = cols;
        self.rows = rows;

        let new_size = TermSize { cols: cols as usize, rows: rows as usize, scrollback };
        self.term.lock().resize(new_size);

        self.pty.resize(cols, rows);
    }

    /// Write keyboard input bytes to the PTY.
    pub fn write_input(&self, data: &[u8]) {
        self.pty.write(data);
    }

    /// Lock and inspect the terminal grid. The closure receives a reference
    /// to the locked Term for read-only access during rendering.
    pub fn with_term<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Term<PtyEventProxy>) -> R,
    {
        f(&self.term.lock())
    }
}
