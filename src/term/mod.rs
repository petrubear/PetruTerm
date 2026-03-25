pub mod pty;
pub mod color;

pub use pty::{Pty, PtyEvent, PtyEventProxy};
pub use alacritty_terminal::vte::ansi::CursorShape;

use anyhow::Result;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{Config as TermConfig, TermMode};
use alacritty_terminal::Term;
use alacritty_terminal::sync::FairMutex;
use std::sync::Arc;

use crate::config::Config;
use winit::event_loop::EventLoopProxy;

/// Cursor rendering info extracted from the terminal for one frame.
#[derive(Debug, Clone, Copy)]
pub struct CursorInfo {
    pub col: usize,
    pub row: usize,
    pub shape: CursorShape,
    pub visible: bool,
}

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
    /// `cell_width` / `cell_height` are physical pixel dimensions of one cell
    /// (used in the TIOCSWINSZ ioctl for image-protocol-aware programs).
    pub fn new(
        config: &Config,
        cols: u16,
        rows: u16,
        cell_width: u16,
        cell_height: u16,
        wakeup: EventLoopProxy<()>,
    ) -> Result<Self> {
        // TD-002 (partial fix): Term requires an EventListener at construction,
        // but the Notifier is only available after PtyEventLoop is created in
        // Pty::spawn. We create the shared Arc<OnceLock<Notifier>> here and
        // pass it to BOTH the placeholder proxy (stored inside Term) and to
        // Pty::spawn, which sets the OnceLock once the Notifier is ready.
        // This ensures that PtyWrite events from term.process() — which go
        // through Term's internal proxy — can forward responses immediately.
        let direct_notifier: Arc<std::sync::OnceLock<alacritty_terminal::event_loop::Notifier>> =
            Arc::new(std::sync::OnceLock::new());

        let (tx_placeholder, _rx) = crossbeam_channel::unbounded();
        let proxy = PtyEventProxy {
            tx: tx_placeholder,
            wakeup: wakeup.clone(),
            direct_notifier: Arc::clone(&direct_notifier),
        };

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
        let pty = Pty::spawn(config, Arc::clone(&term), cols, rows, cell_width, cell_height, wakeup, direct_notifier)?;

        Ok(Self { term, pty, cols, rows })
    }

    /// Resize the terminal grid and PTY.
    pub fn resize(&mut self, cols: u16, rows: u16, scrollback: usize, cell_width: u16, cell_height: u16) {
        self.cols = cols;
        self.rows = rows;

        let new_size = TermSize { cols: cols as usize, rows: rows as usize, scrollback };
        self.term.lock().resize(new_size);

        self.pty.resize(cols, rows, cell_width, cell_height);
    }

    /// Write keyboard input bytes to the PTY.
    pub fn write_input(&self, data: &[u8]) {
        self.pty.write(data);
    }

    /// Start a new text selection at the given cell coordinate.
    pub fn start_selection(&self, col: usize, row: usize, ty: SelectionType) {
        let point = Point::new(Line(row as i32), Column(col));
        self.term.lock().selection = Some(Selection::new(ty, point, Direction::Left));
    }

    /// Extend the active selection to the given cell coordinate.
    pub fn update_selection(&self, col: usize, row: usize) {
        let point = Point::new(Line(row as i32), Column(col));
        if let Some(sel) = &mut self.term.lock().selection {
            sel.update(point, Direction::Right);
        }
    }

    /// Return the currently selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    /// Clear any active selection.
    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    /// Scroll the viewport by `delta` lines (positive = toward bottom, negative = toward history).
    pub fn scroll_display(&self, delta: i32) {
        self.term.lock().scroll_display(Scroll::Delta(delta));
    }

    /// Cursor position and shape for the current frame.
    pub fn cursor_info(&self) -> CursorInfo {
        self.with_term(|term| {
            let content = term.renderable_content();
            CursorInfo {
                col: content.cursor.point.column.0,
                row: content.cursor.point.line.0.max(0) as usize,
                shape: content.cursor.shape,
                visible: content.display_offset == 0
                    && content.cursor.shape != CursorShape::Hidden,
            }
        })
    }

    /// Return whether bracketed paste mode is active.
    pub fn bracketed_paste_mode(&self) -> bool {
        self.term.lock().mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Return the active mouse mode flags: (any_reporting, sgr, motion).
    pub fn mouse_mode_flags(&self) -> (bool, bool, bool) {
        let mode = *self.term.lock().mode();
        let any = mode.intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG,
        );
        let sgr = mode.contains(TermMode::SGR_MOUSE);
        let motion = mode.intersects(TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG);
        (any, sgr, motion)
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
