# Grill With Docs

A relentless interview to sharpen a plan or design, which also creates docs (ADRs and glossary) as we go.

This skill combines two primitives:
1. **Grilling** — interview the user relentlessly about a plan until every branch of the design tree is resolved
2. **Domain Modeling** — actively build and sharpen the project's domain model

## Process

1. Begin the grilling interview (rounds, frontier, recommended answers)
2. As decisions crystallize, update `CONTEXT.md` inline with new terms
3. When a decision is hard-to-reverse, surprising, and a real trade-off, offer an ADR
4. Continue until the frontier is empty

## Rules

- Finding _facts_ is your job, never the user's
- The _decisions_ are the user's: put each to them and wait
- Update `CONTEXT.md` inline as terms are resolved
- Only offer ADRs when all three conditions are met: hard to reverse, surprising without context, result of a real trade-off
- The session is done when the frontier is empty
