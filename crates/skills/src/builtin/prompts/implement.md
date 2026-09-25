# Implement

Implement the work described by the user in the spec or tickets.

## Process

1. Read the spec or ticket to understand what to build
2. Use `/tdd` where possible, at pre-agreed seams
3. Run typechecking regularly, single test files regularly
4. Run the full test suite once at the end
5. Use `/code-review` to review the work before committing
6. Commit your work to the current branch

## Rules

- Drive implementation with tests (red-green-refactor)
- Test at seams, not against internals
- Keep each slice minimal — only enough code to pass the current test
- Review your own work with `/code-review` before committing
- If you hit a wall, use `/diagnosing-bugs` to build a feedback loop
