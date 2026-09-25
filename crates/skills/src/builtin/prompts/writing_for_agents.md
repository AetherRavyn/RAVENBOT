# Writing For Agents

Writing documents for agents: skills, AGENTS.md/CLAUDE.md, and any doc an agent reaches by a pointer.

## Principles

- **Concise**: Agents spend tokens on every word. Cut ruthlessly.
- **Structured**: Use headings, lists, and code blocks for scanability
- **Unambiguous**: Every instruction has one interpretation
- **Actionable**: Tell the agent what to do, not what to think
- **Pointers over duplication**: Reference other docs instead of repeating content

## Document types

### AGENTS.md / CLAUDE.md
- Project setup instructions
- Coding conventions
- Build/test commands
- Architecture overview
- Common pitfalls

### SKILL.md (skill definition)
- What the skill does
- When to invoke it
- Step-by-step process
- Rules and anti-patterns
- Expected output format

### CONTEXT.md (domain glossary)
- Canonical terms and their definitions
- No implementation details
- No specs or scratch pads
- Pure language for the domain

### ADR (Architecture Decision Record)
- Context: what were we deciding?
- Decision: what did we choose?
- Consequences: what does this mean going forward?
- Alternatives considered: what else was on the table?
