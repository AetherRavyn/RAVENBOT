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

/// How many lines on one side a real diff is computed over.
///
/// Below this the backtrack table is a few megabytes and the call is instant.
/// Above it, the counts still come from the multiset path and no diff is
/// produced — a unified diff of a generated 40k-line file is not something a
/// reader wants, and a backtrack table big enough to hold it is tens of
/// megabytes allocated inside a tool call.
const MAX_DIFF_LINES: usize = 1_500;

/// Context lines kept either side of each change in a hunk.
///
/// Two is the git default and it is right here too: one is not enough to
/// recognise where you are in the file, five surrounds every change in text
/// nobody is reading.
const HUNK_CONTEXT: usize = 2;

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Same,
    Del,
    Add,
}

/// A unified diff between two versions of a file.
///
/// The counts alone answer "how much changed"; this answers the question the
/// counts provoke — *which* lines. A ledger reporting `+31 −12` on `auth.rs` is
/// a number with no way to check it, and every total in the world is only as
/// interesting as the ability to expand it.
///
/// Returns `None` rather than an approximation when the file is too large, for
/// the same reason a half-reasoned chain is worse than a missing one: a diff
/// that silently omits most of the file is a diff that disagrees with the counts
/// stored beside it.
pub fn unified(before: &str, after: &str, path: &str) -> Option<String> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    if a.len() > MAX_DIFF_LINES || b.len() > MAX_DIFF_LINES {
        return None;
    }

    // Backtrack to rows in one pass, then group them into hunks.
    let mut dp = vec![vec![0u16; b.len() + 1]; a.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    let mut rows: Vec<(Row, &str)> = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (a.len(), b.len());
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            rows.push((Row::Same, a[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            rows.push((Row::Add, b[j - 1]));
            j -= 1;
        } else {
            rows.push((Row::Del, a[i - 1]));
            i -= 1;
        }
    }
    rows.reverse();

    // Only the changed rows matter for splitting; the rest is context.
    let changed: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, (r, _))| *r != Row::Same)
        .map(|(k, _)| k)
        .collect();
    if changed.is_empty() {
        return None;
    }

    // Widen each change to the context around it first, then merge anything
    // that touches. Doing it in that order matters: merging raw positions and
    // widening afterwards would leave two headers whose context overlaps, which
    // git never emits and which reads as the same lines twice.
    let mut groups: Vec<(usize, usize)> = changed
        .into_iter()
        .map(|k| (k.saturating_sub(HUNK_CONTEXT), (k + HUNK_CONTEXT).min(rows.len() - 1)))
        .collect();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(groups.len());
    for (start, end) in groups.drain(..) {
        match merged.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let groups = merged;

    let mut out = String::with_capacity(1024);
    out.push_str(&format!("--- a/{path}\n+++ b/{path}\n"));

    for &(start, end) in groups.iter() {
        // Line numbers for the hunk header: 1-based counts of what each side
        // contributes *up to this point*.
        let old_before = rows[..start].iter().filter(|(r, _)| *r != Row::Add).count();
        let new_before = rows[..start].iter().filter(|(r, _)| *r != Row::Del).count();
        let old_span = rows[start..=end].iter().filter(|(r, _)| *r != Row::Add).count();
        let new_span = rows[start..=end].iter().filter(|(r, _)| *r != Row::Del).count();

        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            if old_span == 0 { old_before } else { old_before + 1 },
            old_span,
            if new_span == 0 { new_before } else { new_before + 1 },
            new_span,
        ));

        // Git writes the trailing hunk header's context as a brace section
        // header when it can find one. We cannot cheaply, so nothing is faked.
        // Git would write a `\ No newline at end of file` marker here when a
        // side lacks a trailing newline. We do not, because that marker means
        // something specific and emitting it as a hunk separator would be a
        // lie told in a syntax the reader already knows.
        for (r, line) in &rows[start..=end] {
            let marker = match r {
                Row::Same => ' ',
                Row::Del => '-',
                Row::Add => '+',
            };
            out.push(marker);
            out.push_str(line);
            out.push('\n');
        }
    }
    Some(out)
}

/// One file's worth of patch: its counts, and the lines themselves.
///
/// Counts alone answer "how much"; `text` answers *which*, and a total that
/// cannot be expanded into the lines it claims is a total nobody can check.
#[derive(Debug, Clone, PartialEq)]
pub struct FilePatch {
    pub path: String,
    pub added: usize,
    pub deleted: usize,
    /// The patch as it applies to this file — headers plus hunks, so it can be
    /// shown as a git diff or fed straight back to `patch`.
    pub text: String,
}

/// Per-file counts out of one patch.
pub fn patch_file_changes(patch: &str) -> Vec<(String, usize, usize)> {
    patch_file_diffs(patch)
        .into_iter()
        .map(|p| (p.path, p.added, p.deleted))
        .collect()
}

/// Split one patch into the diff each file received.
///
/// A single patch can touch several files, and collapsing them into one number
/// would make the ledger say "Priya changed 40 lines" when the honest answer is
/// "38 in `auth.rs`, 2 in `mod.rs`" — which is the only form anyone can act on.
///
/// Each entry keeps the lines as well as the counts, because the counts are a
/// claim and this is what makes the claim checkable.
pub fn patch_file_diffs(patch: &str) -> Vec<FilePatch> {
    let mut out: Vec<FilePatch> = Vec::new();
    let mut cur: Option<usize> = None;
    // A `--- /dev/null` names no file — it introduces one, and the name only
    // arrives on the `+++` line. Held until then, so a new file's own header
    // is not attributed to whatever section came before it.
    let mut pending_minus = false;

    let push = |out: &mut Vec<FilePatch>, cur: &mut Option<usize>, path: &str| {
        match out.iter().position(|p| p.path == path) {
            Some(i) => *cur = Some(i),
            None => {
                out.push(FilePatch { path: path.to_string(), added: 0, deleted: 0, text: String::new() });
                *cur = Some(out.len() - 1);
            }
        }
    };
    let echo = |out: &mut Vec<FilePatch>, cur: Option<usize>, line: &str| {
        if let Some(i) = cur {
            out[i].text.push_str(line);
            out[i].text.push('\n');
        }
    };

    let raw: Vec<&str> = patch.lines().collect();
    for (i, line) in raw.iter().enumerate() {
        let line = *line;

        // Section markers that cannot be content: a changed line carries a
        // `+`/`-` in front of it and a context line a space, so an unprefixed
        // `diff --git`, `index` or `@@` can only be a real boundary.
        if line.starts_with("diff --git ") || line.starts_with("index ") {
            continue;
        }
        if line.starts_with("@@") {
            echo(&mut out, cur, line);
            continue;
        }

        let prev = if i > 0 { raw[i - 1] } else { "" };
        let next = raw.get(i + 1).copied().unwrap_or("");

        // `--- ` and `+++ ` are recognised as a *pair*, by adjacency, because
        // neither prefix identifies a header on its own. Inside a hunk, a
        // removed line whose text starts with `-- ` prints as `--- …`: a
        // markdown rule, a commented-out shell line, a box-drawing comment.
        // Attributing one to a filename invented by the text after it would
        // credit work to a path that does not exist — and every total below
        // would inherit the mistake.
        if line.starts_with("--- ") && next.starts_with("+++ ") {
            let target = line[4..].trim();
            if target.is_empty() {
                continue;
            }
            if target == "/dev/null" {
                pending_minus = true;
            } else {
                push(&mut out, &mut cur, strip_prefix_marker(target));
                echo(&mut out, cur, line);
            }
            continue;
        }
        if line.starts_with("+++ ") && prev.starts_with("--- ") {
            let target = line[4..].trim();
            if !target.is_empty() && target != "/dev/null" {
                push(&mut out, &mut cur, strip_prefix_marker(target));
            }
            if std::mem::take(&mut pending_minus) {
                echo(&mut out, cur, "--- /dev/null");
            }
            echo(&mut out, cur, line);
            continue;
        }

        if let Some(i) = cur {
            if line.starts_with('+') {
                out[i].added += 1;
            } else if line.starts_with('-') {
                out[i].deleted += 1;
            }
            out[i].text.push_str(line);
            out[i].text.push('\n');
        }
    }
    out.retain(|p| p.added > 0 || p.deleted > 0);
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
    fn builds_a_unified_diff_for_a_one_line_change() {
        let out = unified("fn a() {}\nfn b() {}\nfn c() {}\n", "fn a() {}\nfn B() {}\nfn c() {}\n", "x.rs")
            .expect("small file, diff expected");
        assert!(out.contains("--- a/x.rs"), "{out}");
        assert!(out.contains("+++ b/x.rs"), "{out}");
        assert!(out.contains("@@ -1,3 +1,3 @@"), "{out}");
        assert!(out.contains("-fn b() {}"), "{out}");
        assert!(out.contains("+fn B() {}"), "{out}");
        // Context lines keep their leading space, which is what makes a hunk
        // readable as a hunk rather than as two lists.
        assert!(out.contains(" fn a() {}"), "{out}");
    }

    #[test]
    fn an_unchanged_file_has_no_diff() {
        // Counts are (0,0) and there is nothing to expand to. Returning an
        // empty diff would make the row look expandable and reveal nothing.
        assert_eq!(unified("a\nb\n", "a\nb\n", "x.rs"), None);
    }

    #[test]
    fn splits_distant_changes_into_separate_hunks() {
        let mut before: Vec<String> = (0..40).map(|i| format!("line {i}")).collect();
        let mut after = before.clone();
        before[0] = "old head".into();
        after[0] = "new head".into();
        before[35] = "old tail".into();
        after[35] = "new tail".into();

        let out = unified(&before.join("\n"), &after.join("\n"), "big.txt").expect("40 lines");
        let hunks = out.matches("@@ -").count();
        // One change at each end of a 40-line file with 2 lines of context
        // cannot be one hunk without carrying 30 unchanged lines between them.
        assert_eq!(hunks, 2, "{out}");
        assert!(out.contains("-old head") && out.contains("-old tail"), "{out}");
    }

    #[test]
    fn neighbouring_changes_share_a_hunk() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let after = "a\nB\nc\nD\ne\nf\ng\nh\ni\nj\n";
        let out = unified(before, after, "x").expect("diff");
        // b→B and d→D are two lines apart; two hunks would repeat the same
        // context between them and read as the file being touched twice.
        assert_eq!(out.matches("@@ -").count(), 1, "{out}");
    }

    #[test]
    fn a_pure_insertion_reports_an_empty_old_span() {
        let out = unified("keep\n", "keep\nadded\n", "x.rs").expect("diff");
        assert!(out.contains("@@ -1,1 +1,2 @@"), "{out}");
        assert!(out.contains("+added"), "{out}");
    }

    #[test]
    fn refuses_a_generated_file_rather_than_lying_about_it() {
        // A diff that silently omits most of the file would disagree with the
        // counts stored beside it, and a number that cannot be expanded to the
        // lines it claims is a number nobody can check.
        let big = (0..MAX_DIFF_LINES + 1).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
        assert_eq!(unified(&big, &big, "gen.rs"), None);
    }

    #[test]
    fn a_pure_deletion_reports_an_empty_new_span() {
        let out = unified("one\ntwo\nthree\n", "one\nthree\n", "x.rs").expect("diff");
        assert!(out.contains("@@ -1,3 +1,2 @@"), "{out}");
        assert!(out.contains("-two"), "{out}");
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
    fn keeps_the_lines_it_counts() {
        // The whole reason `FilePatch` exists: counts alone are a claim with
        // no way to check it, so the text that produced them travels along.
        let patch = "diff --git a/x.rs b/x.rs
--- a/x.rs
+++ b/x.rs
@@ -1,2 +1,3 @@
 fn a() {}
+fn b() {}
";
        let diffs = patch_file_diffs(patch);
        assert_eq!(diffs.len(), 1);
        let text = &diffs[0].text;
        assert!(text.starts_with("--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,3 @@\n"), "{text}");
        // The git envelope is dropped: each entry is a bare unified diff, not
        // a git diff wrapped around a smaller one.
        assert!(!text.contains("diff --git"), "{text}");
    }

    #[test]
    fn a_created_file_is_attributed_to_the_name_it_gains() {
        // `--- /dev/null` names nothing; the name arrives one line later. Emit
        // the header before the file is known and it lands in whichever entry
        // was open before this one.
        let patch = "--- /dev/null
+++ b/fresh.rs
@@ -0,0 +1,2 @@
+fn a() {}
+fn b() {}
";
        let diffs = patch_file_diffs(patch);
        assert_eq!(diffs.len(), 1, "{diffs:?}");
        assert_eq!(diffs[0].path, "fresh.rs");
        assert_eq!((diffs[0].added, diffs[0].deleted), (2, 0));
        assert!(diffs[0].text.contains("--- /dev/null"), "{}", diffs[0].text);
    }

    #[test]
    fn a_removed_line_that_looks_like_a_header_stays_content() {
        // Deleting a markdown rule or a commented-out shell line prints as
        // `--- …` inside the hunk. The old parser read any `--- ` as a header,
        // so this credited the remaining lines to a file literally named after
        // the removed text — and dropped the real file for having no changes.
        let patch = "--- a/doc.md
+++ b/doc.md
@@ -1,3 +1,3 @@
 keep()
--- rule
-keep2()
";
        let changes = patch_file_changes(patch);
        assert_eq!(changes, vec![("doc.md".to_string(), 0, 2)], "{changes:?}");
    }

    #[test]
    fn a_patch_that_changes_nothing_counts_nothing() {
        assert_eq!(patch_changes(""), (0, 0));
        assert_eq!(patch_changes("no hunks here"), (0, 0));
    }
}
