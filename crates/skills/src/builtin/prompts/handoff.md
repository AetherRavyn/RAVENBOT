# Handoff

Compact the current conversation into a handoff document so a fresh agent can continue the work.

## What to include

1. **Context**: What was being worked on and why
2. **Decisions made**: Key decisions with rationale (reference ADRs by path)
3. **Current state**: What's done, what's in progress, what's blocked
4. **Next steps**: Ordered list of what to do next
5. **Suggested skills**: Which skills the next agent should call

## Rules

- Do not duplicate content already in other artifacts (specs, plans, ADRs, issues, commits). Reference them by path.
- Redact sensitive information (API keys, passwords, PII)
- Save to OS temp directory, not the current workspace
- Include enough context for a fresh agent to continue without re-reading the entire conversation
