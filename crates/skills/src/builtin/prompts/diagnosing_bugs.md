# Diagnosing Bugs

A discipline for hard bugs. Skip phases only when explicitly justified.

## Redact
**Redact every secret first**: write `<REDACTED>` in its place. Build loops against env vars, so the credential stays in the environment rather than in what you show.

## Phase 1: Build a feedback loop
**This is the skill.** Everything else is mechanical. If you have a **tight** pass/fail signal for the bug, you will find the cause.

### Ways to construct one
1. **Failing test** at whatever seam reaches the bug
2. **Curl / HTTP script** against a running dev server
3. **CLI invocation** with a fixture input, diffing stdout against a known-good snapshot
4. **Headless browser script** (Playwright / Puppeteer)
5. **Replay a captured trace**
6. **Throwaway harness** — minimal subset of the system
7. **Property / fuzz loop** — 1000 random inputs
8. **Bisection harness** — `git bisect run`
9. **Differential loop** — old-version vs new-version
10. **HITL bash script** — last resort

### Tighten the loop
- Can I make it faster?
- Can I make the signal sharper?
- Can I make it more deterministic?

### Completion criterion
One command that is: **Red-capable**, **Deterministic**, **Fast** (seconds), **Agent-runnable**.

## Phase 2: Reproduce + minimise
Run the loop. Watch it go red. Shrink the repro to the **smallest scenario that still goes red**. Cut inputs, callers, config, data one at a time.

## Phase 3: Hypothesise
Generate **3–5 ranked hypotheses** before testing any. Each must be **falsifiable**: "If <X> is the cause, then <changing Y> will make the bug disappear."

## Phase 4: Instrument
Each probe must map to a specific prediction. **Change one variable at a time.**
- Debugger / REPL inspection preferred
- Targeted logs at boundaries
- Tag every debug log with `[DEBUG-xxxx]`

## Phase 5: Fix + regression test
Write the regression test **before the fix**, but only if there is a **correct seam** for it.

## Phase 6: Cleanup
- Original repro no longer reproduces
- Regression test passes
- All `[DEBUG-...]` instrumentation removed
- Throwaway prototypes deleted
- Correct hypothesis stated in commit message
