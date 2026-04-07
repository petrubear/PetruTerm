/// Kind of a diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffKind {
    Context,
    Added,
    Removed,
}

/// A single line in a computed diff.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// Compute a line-based diff between `old` and `new` using LCS.
/// Input is capped at 300 lines each to avoid O(m*n) blowup on huge files.
/// Returns all diff lines (context + added + removed).
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    const CAP: usize = 300;
    let old_lines: Vec<&str> = old.lines().take(CAP).collect();
    let new_lines: Vec<&str> = new.lines().take(CAP).collect();
    lcs_diff(&old_lines, &new_lines)
}

/// Build the LCS table and backtrack to produce diff lines.
fn lcs_diff(old: &[&str], new: &[&str]) -> Vec<DiffLine> {
    let m = old.len();
    let n = new.len();

    // dp[i][j] = LCS length of old[..i] and new[..j]
    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if old[i - 1] == new[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    // Backtrack
    let mut lines = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            lines.push(DiffLine { kind: DiffKind::Context, text: old[i - 1].to_string() });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            lines.push(DiffLine { kind: DiffKind::Added, text: new[j - 1].to_string() });
            j -= 1;
        } else {
            lines.push(DiffLine { kind: DiffKind::Removed, text: old[i - 1].to_string() });
            i -= 1;
        }
    }
    lines.reverse();
    lines
}

/// Return only the lines near changes (at most `ctx` context lines around each hunk).
/// Useful to trim unchanged regions so the confirmation view shows only what matters.
pub fn compress_diff(lines: &[DiffLine], ctx: usize) -> Vec<DiffLine> {
    if lines.is_empty() {
        return Vec::new();
    }

    // Mark which lines are "near" a change.
    let n = lines.len();
    let mut near = vec![false; n];
    for i in 0..n {
        if lines[i].kind != DiffKind::Context {
            let lo = i.saturating_sub(ctx);
            let hi = (i + ctx + 1).min(n);
            for k in lo..hi {
                near[k] = true;
            }
        }
    }

    let mut result = Vec::new();
    let mut skipping = false;
    for (i, line) in lines.iter().enumerate() {
        if near[i] {
            if skipping {
                result.push(DiffLine { kind: DiffKind::Context, text: "⋯".to_string() });
                skipping = false;
            }
            result.push(line.clone());
        } else {
            skipping = true;
        }
    }
    result
}
