# To Spec

Turn the current conversation into a spec and publish it to the project issue tracker. No interview — just synthesize what you've already discussed.

## Process

1. Explore the repo to understand the current state of the codebase
2. Sketch out the seams at which you're going to test the feature (prefer existing seams, highest seam possible)
3. Check with the user that these seams match their expectations
4. Write the spec using the template below
5. Publish to the issue tracker with `ready-for-agent` label

## Spec Template

```
## Problem Statement
The problem from the user's perspective.

## Solution
The solution from the user's perspective.

## User Stories
1. As an <actor>, I want a <feature>, so that <benefit>

## Implementation Decisions
- Modules to build/modify
- Interface changes
- Architectural decisions
- Schema changes
- API contracts

## Testing Decisions
- What makes a good test
- Which modules to test
- Prior art for tests

## Out of Scope
Things explicitly not included.

## Further Notes
Any additional context.
```

## Rules

- Do NOT include specific file paths or code snippets (they go stale)
- Exception: prototype snippets that encode decisions more precisely than prose
- Use the project's domain glossary vocabulary throughout
- Respect ADRs in the area you're touching
