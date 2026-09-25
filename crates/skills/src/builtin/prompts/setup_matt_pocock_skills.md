# Setup Matt Pocock's Skills

Scaffold the per-repo configuration that the engineering skills assume:
- **Issue tracker**: where issues live (GitHub, GitLab, or local markdown)
- **Triage labels**: the strings used for the five canonical triage roles
- **Domain docs**: where `CONTEXT.md` and ADRs live

## Process

### 1. Explore
Check the repo's starting state: git remote, existing AGENTS.md/CLAUDE.md, CONTEXT.md, docs/adr/, monorepo signals.

### 2. Present findings and ask
- **Section A: Issue tracker** — GitHub, GitLab, Local markdown, or Other
- **Section B: Triage labels** — Keep defaults or override
- **Section C: Domain docs** — Single-context (default) or multi-context (monorepo only)

### 3. Confirm and edit
Show drafts of the config files before writing.

### 4. Write
- Edit existing CLAUDE.md or AGENTS.md (never create both)
- Write `docs/agents/issue-tracker.md`, `docs/agents/domain.md`, `docs/agents/triage-labels.md`

### 5. Done
Tell the user which engineering skills will now read from these files.
