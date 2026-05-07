pub mod blocks;
pub mod color;
pub mod flag_db;
pub mod input_shadow;
pub mod osc133;
pub mod pty;
pub mod tokenizer;

pub use alacritty_terminal::vte::ansi::CursorShape;
pub use blocks::BlockManager;
pub use input_shadow::InputShadow;
pub use osc133::Osc133Marker;
pub use pty::{Pty, PtyEvent, PtyEventProxy};

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config as TermConfig, TermMode};
use alacritty_terminal::Term;
use anyhow::Result;
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
    /// PID of the shell child process (for CWD resolution).
    pub child_pid: u32,
    /// OSC 133 command block tracker for this pane.
    pub block_manager: BlockManager,
    /// Shadow of the current input line for decoration (syntax colour, ghost text).
    pub input_shadow: InputShadow,
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
        working_directory: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        let term_config = TermConfig {
            scrolling_history: config.scrollback_lines as usize,
            kitty_keyboard: true,
            ..Default::default()
        };

        let size = TermSize {
            cols: cols as usize,
            rows: rows as usize,
            scrollback: config.scrollback_lines as usize,
        };

        let (pty, term) = Pty::spawn(
            config,
            cols,
            rows,
            cell_width,
            cell_height,
            wakeup,
            working_directory,
            term_config,
            &size,
        )?;

        let child_pid = pty.child_pid;

        Ok(Self {
            term,
            pty,
            cols,
            rows,
            child_pid,
            block_manager: BlockManager::new(),
            input_shadow: InputShadow::new(),
        })
    }

    /// Resize the terminal grid and PTY.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        scrollback: usize,
        cell_width: u16,
        cell_height: u16,
    ) {
        self.cols = cols;
        self.rows = rows;

        let new_size = TermSize {
            cols: cols as usize,
            rows: rows as usize,
            scrollback,
        };
        self.term.lock().resize(new_size);

        self.pty.resize(cols, rows, cell_width, cell_height);
    }

    /// Write keyboard input bytes to the PTY.
    pub fn write_input(&self, data: &[u8]) {
        self.pty.write(data);
    }

    /// Start a new text selection at the given viewport cell coordinate.
    /// Adjusts for display_offset so the selection is anchored in buffer space.
    pub fn start_selection(&self, col: usize, row: usize, ty: SelectionType) {
        let mut term = self.term.lock();
        let display_offset = term.grid().display_offset() as i32;
        let point = Point::new(Line(row as i32 - display_offset), Column(col));
        term.selection = Some(Selection::new(ty, point, Direction::Left));
    }

    /// Extend the active selection to the given viewport cell coordinate.
    /// Adjusts for display_offset so the selection tracks the scrolled position.
    pub fn update_selection(&self, col: usize, row: usize) {
        let mut term = self.term.lock();
        let display_offset = term.grid().display_offset() as i32;
        let point = Point::new(Line(row as i32 - display_offset), Column(col));
        if let Some(sel) = &mut term.selection {
            sel.update(point, Direction::Right);
        }
    }

    /// Return the currently selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    /// Clear any active selection.
    #[allow(dead_code)]
    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    /// Scroll the viewport by `delta` lines (positive = toward bottom, negative = toward history).
    pub fn scroll_display(&self, delta: i32) {
        self.term.lock().scroll_display(Scroll::Delta(delta));
    }

    /// Scroll the viewport to the bottom (display_offset = 0).
    pub fn scroll_to_bottom(&self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// Return (display_offset, history_size) for scroll bar positioning.
    /// display_offset = 0 means at the bottom; display_offset = history_size means at the top.
    pub fn scrollback_info(&self) -> (usize, usize) {
        self.with_term(|term| {
            let offset = term.grid().display_offset();
            let history = term.grid().history_size();
            (offset, history)
        })
    }

    /// Cursor position and shape for the current frame.
    pub fn cursor_info(&self) -> CursorInfo {
        self.with_term(|term| {
            let content = term.renderable_content();
            CursorInfo {
                col: content.cursor.point.column.0,
                row: content.cursor.point.line.0.max(0) as usize,
                shape: content.cursor.shape,
                visible: content.display_offset == 0 && content.cursor.shape != CursorShape::Hidden,
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

// ── CWD resolution ────────────────────────────────────────────────────────────

/// Returns the current working directory of a process by PID.
/// On macOS uses `proc_pidinfo`; on Linux reads `/proc/{pid}/cwd`.
pub fn process_cwd(pid: u32) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        let mut info: libc::proc_vnodepathinfo = unsafe { mem::zeroed() };
        let size = mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
        let ret = unsafe {
            libc::proc_pidinfo(
                pid as libc::pid_t,
                libc::PROC_PIDVNODEPATHINFO,
                0,
                &mut info as *mut _ as *mut libc::c_void,
                size,
            )
        };
        if ret <= 0 {
            return None;
        }
        // vip_path is [[c_char; 32]; 32] = 1024 bytes; reinterpret in-place, no allocation.
        // SAFETY: c_char is i8 on macOS; reinterpreting as u8 is always valid for path bytes.
        let path_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(info.pvi_cdir.vip_path.as_ptr() as *const u8, 1024)
        };
        let end = path_bytes.iter().position(|&b| b == 0).unwrap_or(1024);
        let s = std::str::from_utf8(&path_bytes[..end]).ok()?;
        if s.is_empty() {
            return None;
        }
        Some(std::path::PathBuf::from(s))
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_link(format!("/proc/{}/cwd", pid)).ok()
    }
}
