# To Tickets

Break a plan, spec, or conversation into a set of **tracer-bullet tickets**, each declaring its blocking edges.

## Process

### 1. Gather context
Work from the conversation context. If the user passes a spec path or issue number, fetch it.

### 2. Explore the codebase (optional)
Look for opportunities to prefactor. "Make the change easy, then make the easy change."

### 3. Draft vertical slices
Each slice cuts a narrow but COMPLETE path through every layer (schema, API, UI, tests): vertical, NOT horizontal.

Rules:
- A completed slice is demoable or verifiable on its own
- Each slice fits in a single fresh context window
- Any prefactoring is done first
- Wide refactors use expand-contract pattern instead

### 4. Quiz the user
Present the breakdown as a numbered list with title, blocked-by, and what it delivers. Iterate until approved.

### 5. Publish to the configured tracker
- **Local files**: one file per ticket under `.scratch/<feature-slug>/issues/<NN>-<slug>.md`
- **Real tracker**: one issue per ticket in dependency order with native blocking links

## Rules

- Avoid specific file paths or code snippets (they go stale)
- Exception: prototype snippets that encode decisions precisely
- Use domain glossary vocabulary
- Apply `ready-for-agent` triage label
