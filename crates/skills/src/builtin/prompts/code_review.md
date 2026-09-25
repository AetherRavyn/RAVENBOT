# Code Review

Two-axis review of the diff between `HEAD` and a fixed point the user supplies:

- **Standards**: does the code conform to this repo's documented coding standards?
- **Spec**: does the code faithfully implement the originating issue / spec?

Both axes run as **parallel sub-agents** so they don't pollute each other's context.

## Process

### 1. Pin the fixed point
Capture the diff command: `git diff <fixed-point>...HEAD` (three-dot, comparison against merge-base). Note the list of commits via `git log <fixed-point>..HEAD --oneline`.

Confirm the fixed point resolves (`git rev-parse <fixed-point>`) and the diff is non-empty.

### 2. Identify the spec source
Look for the originating spec, in this order:
1. Issue references in commit messages (`#123`, `Closes #45`)
2. A path the user passed as an argument
3. A spec file under `docs/`, `specs/`, or `.scratch/`
4. If nothing found, ask the user where the spec is

### 3. Identify the standards sources
Anything in the repo that documents how code should be written (`CODING_STANDARDS.md`, `CONTRIBUTING.md`).

Plus the **smell baseline**: Mysterious Name, Duplicated Code, Feature Envy, Data Clumps, Primitive Obsession, Repeated Switches, Shotgun Surgery, Divergent Change, Speculative Generality, Message Chains, Middle Man, Refused Bequest.

### 4. Run both axes
- **Standards**: per file/hunk, find violations of documented standards + baseline smells
- **Spec**: find missing/partial requirements, scope creep, wrong implementations

### 5. Aggregate
Present findings under `## Standards` and `## Spec` headings. End with one-line summary: total findings per axis, worst issue within each axis.

## Why two axes
A change can pass one axis and fail the other. Code that follows every standard but implements the wrong thing → Standards pass, Spec fail. Code that does exactly what was asked but breaks conventions → Spec pass, Standards fail.
