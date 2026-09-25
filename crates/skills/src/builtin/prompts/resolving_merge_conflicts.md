# Resolving Merge Conflicts

Work through an in-progress git merge or rebase conflict hunk by hunk, resolving by intent traced to each side's primary source, then finish the operation (never `--abort`).

## Process

1. **Identify the operation**: `git status` to see if merge or rebase
2. **List conflicted files**: `git diff --name-only --diff-filter=U`
3. **For each file**:
   - Read the full file with conflict markers
   - Understand the intent of each side (trace to commits/branches)
   - Resolve by intent, not by blindly picking one side
   - Remove all conflict markers
4. **Stage resolved files**: `git add <file>`
5. **Complete the operation**: `git commit` (merge) or `git rebase --continue`

## Rules

- Never `git merge --abort` or `git rebase --abort` unless explicitly asked
- Never pick "ours" or "theirs" without understanding intent
- If intent is genuinely unclear, ask the user before resolving
- After resolving, verify the file compiles/passes tests
