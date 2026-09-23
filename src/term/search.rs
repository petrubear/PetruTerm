// Terminal grid text search shared by both binaries (Mux wraps these;
// gpui_shell calls them directly).

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};

use crate::term::Terminal;
use crate::ui::search_bar::SearchMatch;

pub(crate) const MAX_SEARCH_MATCHES: usize = 10_000;

pub(crate) fn push_search_match(
    matches: &mut Vec<SearchMatch>,
    grid_line: i32,
    col: usize,
    len: usize,
) -> bool {
    if matches.len() >= MAX_SEARCH_MATCHES {
        return true;
    }
    matches.push(SearchMatch {
        grid_line,
        col,
        len,
    });
    false
}

/// Search all visible rows and scrollback history for `query` (case-insensitive).
/// Returns matches sorted from oldest history to current screen.
///
/// Implementation uses a collect-then-parallel strategy:
///   Phase 1 (serial, lock held): read the terminal grid into a flat Vec<char>.
///   Phase 2 (parallel, lock released): scan the flat buffer with rayon par_chunks.
/// This keeps the term lock held for only ~O(rows*cols) char copies instead of the
/// full search duration, and parallelizes the CPU-bound scan across all cores.
/// For short scrollback (< PAR_THRESHOLD rows) the serial path is used instead
/// because rayon fork-join overhead (~50 µs) exceeds the serial scan time.
pub fn search_terminal(terminal: &Terminal, query: &str) -> (Vec<SearchMatch>, bool) {
    use rayon::prelude::*;
    const PAR_THRESHOLD: usize = 400;

    if query.is_empty() {
        return (Vec::new(), false);
    }
    let query_lower = query.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let query_len = query_chars.len();

    // Phase 1: collect the entire grid into a flat char buffer while holding the lock.
    // A single flat allocation (rows * cols chars) avoids per-row Vec overhead and
    // produces a contiguous layout suitable for par_chunks in phase 2.
    let (history_i32, cols, flat) = terminal.with_term(|term| {
        let screen_rows = term.screen_lines() as i32;
        let history = term.grid().history_size() as i32;
        let cols = term.columns();
        let total = (history + screen_rows) as usize;
        let mut flat = Vec::with_capacity(total * cols);
        for grid_row in (-history)..screen_rows {
            let line = Line(grid_row);
            for col in 0..cols {
                let c = term.grid()[line][Column(col)].c;
                let c = if c == '\0' { ' ' } else { c };
                flat.push(c.to_lowercase().next().unwrap_or(c));
            }
        }
        (history, cols, flat)
    }); // Term lock released here.

    if cols == 0 || query_chars.is_empty() {
        return (Vec::new(), false);
    }

    let total_rows = flat.len() / cols;

    if total_rows < PAR_THRESHOLD {
        // Serial path: short scrollback or small terminal.
        let mut matches = Vec::new();
        for (chunk_idx, row_chars) in flat.chunks(cols).enumerate() {
            let grid_row = chunk_idx as i32 - history_i32;
            let scan_end = row_chars.len().saturating_sub(query_len.saturating_sub(1));
            for col in 0..scan_end {
                if row_chars[col..col + query_len] == query_chars[..]
                    && push_search_match(&mut matches, grid_row, col, query_len)
                {
                    return (matches, true);
                }
            }
        }
        (matches, false)
    } else {
        // Parallel path: rayon par_chunks — each task scans one row.
        // collect() preserves row order (rayon guarantees output order matches input).
        // No early exit: collect all matches then truncate to MAX_SEARCH_MATCHES.
        // `qc` is a &[char] slice (Copy) so it can be moved into per-row closures
        // without consuming `query_chars`.
        let qc: &[char] = &query_chars;
        let ql = query_len;
        let hi = history_i32;
        let mut matches: Vec<SearchMatch> = flat
            .par_chunks(cols)
            .enumerate()
            .flat_map_iter(|(chunk_idx, row_chars)| {
                let grid_row = chunk_idx as i32 - hi;
                let scan_end = row_chars.len().saturating_sub(ql.saturating_sub(1));
                (0..scan_end).filter_map(move |col| {
                    if row_chars[col..col + ql] == qc[..] {
                        Some(SearchMatch {
                            grid_line: grid_row,
                            col,
                            len: ql,
                        })
                    } else {
                        None
                    }
                })
            })
            .collect();

        let truncated = matches.len() > MAX_SEARCH_MATCHES;
        matches.truncate(MAX_SEARCH_MATCHES);
        (matches, truncated)
    }
}

/// Incremental filter: given matches from a previous (shorter) query, verify each one
/// against the new (longer) query. O(prev_matches × query_len) instead of O(rows × cols).
/// Only valid when `new_query.starts_with(prev_query)` — caller is responsible for this check.
pub fn filter_matches(
    terminal: &Terminal,
    prev: &[SearchMatch],
    new_query: &str,
) -> (Vec<SearchMatch>, bool) {
    if new_query.is_empty() || prev.is_empty() {
        return (Vec::new(), false);
    }
    let q_lower = new_query.to_lowercase();
    let q_chars: Vec<char> = q_lower.chars().collect();
    let q_len = q_chars.len();
    terminal.with_term(|term| {
        let cols = term.columns();
        let mut matches = Vec::with_capacity(prev.len().min(MAX_SEARCH_MATCHES));
        for m in prev {
            if m.col + q_len > cols {
                continue;
            }
            let line = Line(m.grid_line);
            let mut matched = true;
            for (i, &qc) in q_chars.iter().enumerate() {
                let c = term.grid()[line][Column(m.col + i)].c;
                let c = if c == '\0' { ' ' } else { c };
                if c.to_lowercase().next().unwrap_or(c) != qc {
                    matched = false;
                    break;
                }
            }
            if matched && push_search_match(&mut matches, m.grid_line, m.col, q_len) {
                return (matches, true);
            }
        }
        (matches, false)
    })
}

#[cfg(test)]
mod tests {
    use super::{push_search_match, MAX_SEARCH_MATCHES};
    use crate::ui::search_bar::SearchMatch;

    #[test]
    fn push_search_match_truncates_only_after_limit_is_exceeded() {
        let mut matches: Vec<SearchMatch> = Vec::new();
        for col in 0..MAX_SEARCH_MATCHES {
            assert!(!push_search_match(&mut matches, 0, col, 1));
        }
        assert_eq!(matches.len(), MAX_SEARCH_MATCHES);
        assert!(push_search_match(&mut matches, 0, MAX_SEARCH_MATCHES, 1));
        assert_eq!(matches.len(), MAX_SEARCH_MATCHES);
    }
}
