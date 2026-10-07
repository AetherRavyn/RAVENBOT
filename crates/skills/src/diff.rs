//! Line-level change counts for a file write.
//!
//! The question these answer is "what did this agent actually change", which is
//! the one an office cannot answer at all once the browser has reloaded. Tool
//! calls were transient, so five agents could rewrite a project and leave no
//! record of who did which part.
//!
//! Counts rather than the diff itself, because the diff is not this module's
//! business — the journal is a ledger, and a ledger that stored every hunk would
//! grow faster than the files it describes. The full before/after is one
//! `file_read` away for anyone who wants to see it.

/// Above this many lines, exact LCS is quadratic in a way that would stall the
/// tool that is supposed to be making the change. Swapped for the multiset
/// method, which is linear and loses only position: a block that moved reads as
/// unchanged. That is the right trade for a *statistic* — the alternative is a
/// tool call that takes longer than the write it is measuring.
const MAX_LCS_LINES: usize = 4_000;

/// `(lines_added, lines_deleted)` between two versions of a file.
///
/// Both directions matter. Reporting only additions — which is what a naive
/// "count the lines in the new file" does — makes a full rewrite of a 500-line
/// file look like +500/-0, and a reader concludes the agent invented 500 lines
/// rather than replacing the file.
pub fn line_changes(before: &str, after: &str) -> (usize, usize) {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();

    if old.len() > MAX_LCS_LINES || new.len() > MAX_LCS_LINES {
        return multiset_changes(&old, &new);
    }

    let common = lcs_len(&old, &new);
    (new.len().saturating_sub(common), old.len().saturating_sub(common))
}

/// Exact longest-common-subsequence length over lines, with a rolling row.
///
/// Rolling rather than a full matrix: this runs inside a tool call on a user's
/// machine, and an n×m table for 4k×4k lines is 128MB of `usize` for a number
/// that never exceeds 4k.
fn lcs_len(a: &[&str], b: &[&str]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    // Keep the shorter sequence on the inner axis so the row stays small.
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut prev = vec![0usize; short.len() + 1];
    let mut cur = vec![0usize; short.len() + 1];

    for l in long {
        for (j, s) in short.iter().enumerate() {
            cur[j + 1] = if s == l {
                prev[j] + 1
            } else {
                prev[j + 1].max(cur[j])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
        cur.iter_mut().for_each(|v| *v = 0);
    }
    prev[short.len()]
}

/// Linear approximation for large files: compare line multisets.
///
/// A line that appears three times in the new file and once in the old counts
/// as two additions. This is exact for insertions and deletions and blind only
/// to *reordering*, which for a "how much changed" statistic is a defensible
/// blind spot — a moved block genuinely is neither added nor deleted.
fn multiset_changes(old: &[&str], new: &[&str]) -> (usize, usize) {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for l in old {
        *counts.entry(*l).or_insert(0) += 1;
    }
    let mut added = 0usize;
    for l in new {
        let c = counts.entry(*l).or_insert(0);
        if *c > 0 {
            *c -= 1;
        } else {
            added += 1;
        }
    }
    let removed = counts.values().filter(|v| **v > 0).sum::<i64>() as usize;
    (added, removed)
}

/// Counts for a unified diff, straight out of its `+` and `-` body lines.
///
/// Reading the patch rather than re-diffing the file: `code_edit` already knows
/// exactly what it changed, and inferring it again afterwards would disagree
/// with the patch whenever the patch was a no-op or touched a file twice.
pub fn patch_changes(patch: &str) -> (usize, usize) {
    let mut added = 0usize;
    let mut removed = 0usize;
    for line in patch.lines() {
        // `+++` / `---` are the file headers, not changes. A hunk header starts
        // with `@@` and would otherwise be counted as an addition because it
        // begins with `+`… no, it begins with `@`, but the file header is the
        // real trap: `--- a/x` looks exactly like a deletion.
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    (added, removed)
}

/// Per-file counts out of one patch.
///
/// A single patch can touch several files, and collapsing them into one number
/// would make the ledger say "Priya changed 40 lines" when the honest answer is
/// "38 in `auth.rs`, 2 in `mod.rs`" — which is the only form anyone can act on.
pub fn patch_file_changes(patch: &str) -> Vec<(String, usize, usize)> {
    let mut out: Vec<(String, usize, usize)> = Vec::new();
    let mut current: Option<usize> = None;

    for line in patch.lines() {
        // The `+++` side names the file being changed. `+++ /dev/null` is a pure
        // deletion, which belongs to the `---` path instead.
        if let Some(rest) = line.strip_prefix("+++ ") {
            let target = rest.trim();
            if target == "/dev/null" {
                // Deletion: fall back to whatever `---` named, tracked below by
                // the header ordering (`---` always precedes `+++`).
                current = out.last().map(|_| out.len() - 1);
                continue;
            }
            let path = strip_prefix_marker(target);
            match out.iter().position(|(p, _, _)| p == path) {
                Some(i) => current = Some(i),
                None => {
                    out.push((path.to_string(), 0, 0));
                    current = Some(out.len() - 1);
                }
            }
            continue;
        }
        if let Some(target) = line.strip_prefix("--- ") {
            let target = target.trim();
            if target == "/dev/null" {
                continue;
            }
            let path = strip_prefix_marker(target);
            if !out.iter().any(|(p, _, _)| p == path) {
                out.push((path.to_string(), 0, 0));
                current = Some(out.len() - 1);
            } else {
                current = out.iter().position(|(p, _, _)| p == path);
            }
            continue;
        }

        if line.starts_with("@@") {
            continue;
        }
        if let Some(i) = current {
            if line.starts_with('+') {
                out[i].1 += 1;
            } else if line.starts_with('-') {
                out[i].2 += 1;
            }
        }
    }
    out.retain(|(_, a, d)| *a > 0 || *d > 0);
    out
}

/// git prefixes paths with `a/` and `b/`; the journal wants the path the
/// project actually uses, because that is what a reader will search for.
fn strip_prefix_marker(target: &str) -> &str {
    target.strip_prefix("a/").or_else(|| target.strip_prefix("b/")).unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_a_pure_insertion() {
        assert_eq!(line_changes("a\nb\n", "a\nb\nc\n"), (1, 0));
    }

    #[test]
    fn counts_a_pure_deletion() {
        assert_eq!(line_changes("a\nb\nc\n", "a\nc\n"), (0, 1));
    }

    #[test]
    fn a_replacement_is_one_of_each_not_a_pure_addition() {
        // The bug this guards: counting the new file's lines instead of diffing
        // makes a one-line fix look like the line was invented.
        assert_eq!(line_changes("fn main() {}\n", "fn main() { run() }\n"), (1, 1));
    }

    #[test]
    fn a_full_rewrite_reports_both_sides() {
        let before = (0..500).map(|i| format!("old {i}")).collect::<Vec<_>>().join("\n");
        let after = (0..500).map(|i| format!("new {i}")).collect::<Vec<_>>().join("\n");
        assert_eq!(line_changes(&before, &after), (500, 500));
    }

    #[test]
    fn unchanged_files_report_nothing() {
        assert_eq!(line_changes("x\ny\n", "x\ny\n"), (0, 0));
        assert_eq!(line_changes("", ""), (0, 0));
    }

    #[test]
    fn an_empty_file_to_a_full_file_is_all_additions() {
        assert_eq!(line_changes("", "a\nb\n"), (2, 0));
        assert_eq!(line_changes("a\nb\n", ""), (0, 2));
    }

    #[test]
    fn duplicate_lines_count_by_multiplicity() {
        // Two `x`s where there was one is one addition, not zero.
        assert_eq!(line_changes("x\n", "x\nx\n"), (1, 0));
    }

    /**
     * The large-file path. It must return *something* rather than stall the
     * tool, and it must agree with the exact method on the cases that matter:
     * insertions and deletions.
     */
    #[test]
    fn falls_back_to_the_multiset_method_on_large_files() {
        let old: Vec<String> = (0..MAX_LCS_LINES + 10).map(|i| format!("line {i}")).collect();
        let mut new = old.clone();
        new.push("inserted".to_string());
        let (added, removed) = line_changes(&old.join("\n"), &new.join("\n"));
        assert_eq!((added, removed), (1, 0));
    }

    #[test]
    fn a_moved_block_reads_as_unchanged_on_the_fallback() {
        // Deliberate: for a statistic, a move is neither added nor deleted, and
        // this is the only place the fallback differs from the exact method.
        let mut lines: Vec<String> = (0..MAX_LCS_LINES + 10).map(|i| format!("line {i}")).collect();
        let head: Vec<String> = lines.drain(..3).collect();
        lines.extend(head);
        assert_eq!(line_changes(&lines.join("\n"), &lines.join("\n")), (0, 0));
    }

    #[test]
    fn reads_counts_out_of_a_patch() {
        let patch = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 fn a() {}
-old_line()
+new_line()
+added_line()
";
        let (added, removed) = patch_changes(patch);
        // 2 additions, 1 deletion — and neither file header counted, which is
        // the trap: `--- a/src/lib.rs` looks exactly like a deleted line.
        assert_eq!((added, removed), (2, 1));
    }

    #[test]
    fn splits_a_patch_into_per_file_counts() {
        let patch = "diff --git a/src/auth.rs b/src/auth.rs
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -1,2 +1,3 @@
 fn login() {}
+fn logout() {}
diff --git a/src/mod.rs b/src/mod.rs
--- a/src/mod.rs
+++ b/src/mod.rs
@@ -1 +1 @@
-old()
+new()
";
        let changes = patch_file_changes(patch);
        assert_eq!(
            changes,
            vec![
                ("src/auth.rs".to_string(), 1, 0),
                ("src/mod.rs".to_string(), 1, 1),
            ]
        );
    }

    #[test]
    fn a_deleted_file_is_attributed_to_the_path_that_existed() {
        let patch = "--- a/old.rs
+++ /dev/null
@@ -1 +0,0 @@
-gone()
";
        assert_eq!(patch_file_changes(patch), vec![("old.rs".to_string(), 0, 1)]);
    }

    #[test]
    fn files_a_patch_touched_but_did_not_change_are_dropped() {
        // A context-only hunk names a file and changes nothing in it. Keeping it
        // would make the ledger claim a file was modified when it was only read.
        let patch = "--- a/untouched.rs
+++ b/untouched.rs
@@ -1,2 +1,2 @@
 keep()
 keep()
";
        assert_eq!(patch_file_changes(patch), Vec::<(String, usize, usize)>::new());
    }

    #[test]
    fn a_patch_that_changes_nothing_counts_nothing() {
        assert_eq!(patch_changes(""), (0, 0));
        assert_eq!(patch_changes("no hunks here"), (0, 0));
    }
}
