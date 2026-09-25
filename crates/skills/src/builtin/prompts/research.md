# Research

Investigate a question against **primary sources** (official docs, source code, specs, first-party APIs), not a secondary write-up of them. Follow every claim back to the source that owns it.

## Process

1. **Investigate** the question against primary sources
2. **Write findings** to a single Markdown file, citing each claim's source
3. **Save** where the repo already keeps such notes; match existing convention

## Rules

- Primary sources only: official docs, source code, specs, first-party APIs
- Every claim must cite its source URL
- No secondary write-ups or blog posts as evidence
- Capture the answer (verdict + question it settled) in the output file

## Output format

```markdown
# Research: <question>

## Finding
<Answer with inline citations>

## Sources
- [Source Name](URL) — what it established
- [Source Name](URL) — what it established

## Open questions
- Things that couldn't be verified from primary sources
```
