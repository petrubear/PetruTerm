// Baseline bench for the grid-search CPU path (proxy of `Mux::search_active_terminal`).
//
// The real function lives on `Mux`, which is coupled to `winit::EventLoopProxy` and
// therefore cannot be instantiated in a bench without a full windowing stack. This
// bench replicates the algorithmic shape — case-insensitive sliding-window match over
// a char-indexed grid — against a synthetic corpus of the same dimensions as a typical
// terminal session (80 cols x (40 screen + 10 000 scrollback) rows).
//
// KPIs this bench protects:
// - `search_cold_common_word`: full-grid scan for a word with many hits
// - `search_cold_rare_word`: full-grid scan for a word with ~0 hits
// - `search_incremental_extend`: re-filter an existing match set (proxy of
//   `Mux::filter_matches`, i.e. TD-PERF-11 — the incremental path).
// - `search_cold_par_*`: parallel equivalents using the rayon path from
//   `search_active_terminal` Phase 2 (flat_chars + par_chunks).
//
// When the real `Mux::search_active_terminal` becomes benchable (after extracting
// the grid-access from winit coupling), swap this synthetic harness for the real
// one. The measurements should remain comparable within the same order of magnitude.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rayon::prelude::*;

const COLS: usize = 80;
const SCREEN_ROWS: usize = 40;
const SCROLLBACK: usize = 10_000;
const TOTAL_ROWS: usize = SCREEN_ROWS + SCROLLBACK;
const MAX_SEARCH_MATCHES: usize = 10_000;

#[derive(Clone, Copy, Debug)]
struct SearchMatch {
    grid_line: i32,
    col: usize,
    #[allow(dead_code)]
    len: usize,
}

fn push_search_match(
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

/// Same corpus as `build_grid` but returned as a single flat Vec<char> (rows*cols).
/// Mirrors the collect phase in `search_active_terminal` Phase 1.
fn build_flat_grid() -> Vec<char> {
    let corpus = "the quick brown fox jumps over the lazy dog \
                  error warning info debug trace fatal panic retry timeout \
                  fn let const mut struct impl pub use mod trait where match \
                  null void none some ok err true false return yield await async \
                  build test check run install update remove list show help ";
    let corpus_chars: Vec<char> = corpus.chars().collect();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut flat = Vec::with_capacity(TOTAL_ROWS * COLS);
    for _ in 0..(TOTAL_ROWS * COLS) {
        state = state.wrapping_mul(0x2545_F491_4F6C_DD1D).wrapping_add(1);
        let idx = ((state >> 11) as usize) % corpus_chars.len();
        flat.push(corpus_chars[idx]);
    }
    flat
}

/// Parallel search over a flat grid using the same rayon pattern as
/// `search_active_terminal` Phase 2 (par_chunks + flat_map_iter).
fn search_flat_par(flat: &[char], history: i32, query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let q_lower = query.to_lowercase();
    let qc: Vec<char> = q_lower.chars().collect();
    let ql = qc.len();
    let qc_ref: &[char] = &qc;
    let hi = history;

    let mut matches: Vec<SearchMatch> = flat
        .par_chunks(COLS)
        .enumerate()
        .flat_map_iter(|(chunk_idx, row_chars)| {
            let grid_row = chunk_idx as i32 - hi;
            let scan_end = row_chars.len().saturating_sub(ql.saturating_sub(1));
            (0..scan_end).filter_map(move |col| {
                if row_chars[col..col + ql] == qc_ref[..] {
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
    let _ = truncated;
    matches
}

/// Deterministic pseudo-random grid seeded from a fixed string corpus. Uses a
/// splitmix64-style step so the bench is reproducible and not dependent on
/// system entropy.
fn build_grid() -> Vec<Vec<char>> {
    let corpus = "the quick brown fox jumps over the lazy dog \
                  error warning info debug trace fatal panic retry timeout \
                  fn let const mut struct impl pub use mod trait where match \
                  null void none some ok err true false return yield await async \
                  build test check run install update remove list show help ";
    let corpus_chars: Vec<char> = corpus.chars().collect();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut grid = Vec::with_capacity(TOTAL_ROWS);
    for _ in 0..TOTAL_ROWS {
        let mut row = Vec::with_capacity(COLS);
        for _ in 0..COLS {
            state = state.wrapping_mul(0x2545_F491_4F6C_DD1D).wrapping_add(1);
            let idx = ((state >> 11) as usize) % corpus_chars.len();
            row.push(corpus_chars[idx]);
        }
        grid.push(row);
    }
    grid
}

/// Proxy of `Mux::search_active_terminal` — same algorithm, synthetic input.
fn search_grid(grid: &[Vec<char>], history: i32, query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let query_len = query_chars.len();
    let mut matches = Vec::new();

    for (row_idx, row) in grid.iter().enumerate() {
        let grid_row = row_idx as i32 - history;
        let row_lower: Vec<char> = row
            .iter()
            .map(|&c| c.to_lowercase().next().unwrap_or(c))
            .collect();
        if row_lower.len() < query_len {
            continue;
        }
        for col in 0..=row_lower.len() - query_len {
            if row_lower[col..col + query_len] == query_chars[..] {
                if push_search_match(&mut matches, grid_row, col, query_len) {
                    return matches;
                }
            }
        }
    }
    matches
}

/// Proxy of `Mux::filter_matches` — re-verify previous hits against an extended query.
/// The real function reads characters directly from the alacritty grid; here we read
/// from the synthetic grid. Algorithmic cost is identical: O(prev * query_len).
fn filter_matches(
    grid: &[Vec<char>],
    history: i32,
    prev: &[SearchMatch],
    new_query: &str,
) -> Vec<SearchMatch> {
    if new_query.is_empty() || prev.is_empty() {
        return Vec::new();
    }
    let q_lower = new_query.to_lowercase();
    let q_chars: Vec<char> = q_lower.chars().collect();
    let q_len = q_chars.len();
    let cols = grid.first().map(|r| r.len()).unwrap_or(0);

    prev.iter()
        .filter_map(|m| {
            if m.col + q_len > cols {
                return None;
            }
            let row_idx = (m.grid_line + history) as usize;
            let row = grid.get(row_idx)?;
            for (i, &qc) in q_chars.iter().enumerate() {
                let c = row[m.col + i];
                if c.to_lowercase().next().unwrap_or(c) != qc {
                    return None;
                }
            }
            Some(SearchMatch {
                grid_line: m.grid_line,
                col: m.col,
                len: q_len,
            })
        })
        .collect()
}

fn bench_search_cold(c: &mut Criterion) {
    let grid = build_grid();
    let history = SCROLLBACK as i32;
    let mut group = c.benchmark_group("search_cold");

    for (label, query) in &[
        ("common_word_the", "the"),
        ("common_word_error", "error"),
        ("rare_word_zzz", "zzz"),
        ("medium_case_Error", "Error"),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(label), query, |b, &q| {
            b.iter(|| search_grid(&grid, history, q));
        });
    }
    group.finish();
}

fn bench_search_incremental(c: &mut Criterion) {
    let grid = build_grid();
    let history = SCROLLBACK as i32;

    // Prime a match set with a short prefix, then re-filter for an extended query.
    // Mirrors the real typing pattern: user types "e", then "er", then "err".
    let prev = search_grid(&grid, history, "e");

    c.bench_function("search_incremental_extend_e_to_error", |b| {
        b.iter(|| filter_matches(&grid, history, &prev, "error"));
    });
}

/// Parallel cold search using rayon par_chunks over a flat grid.
/// Directly measures Phase 2 of the new `search_active_terminal` implementation.
fn bench_search_cold_par(c: &mut Criterion) {
    let flat = build_flat_grid();
    let history = SCROLLBACK as i32;
    let mut group = c.benchmark_group("search_cold_par");

    for (label, query) in &[
        ("common_word_the", "the"),
        ("common_word_error", "error"),
        ("rare_word_zzz", "zzz"),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(label), query, |b, &q| {
            b.iter(|| search_flat_par(&flat, history, q));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_search_cold,
    bench_search_incremental,
    bench_search_cold_par,
);
criterion_main!(benches);
