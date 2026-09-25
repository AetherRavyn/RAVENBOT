# Domain Modeling

Actively build and sharpen the project's domain model as you design. Challenge terms, invent edge-case scenarios, and write the glossary and decisions down the moment they crystallise.

## File structure
```
/CONTEXT.md
/docs/adr/0001-xxx.md
/src/
```

Create files lazily: only when you have something to write.

## During the session

### Challenge against the glossary
When the user uses a term that conflicts with existing language in `CONTEXT.md`, call it out immediately.

### Sharpen fuzzy language
When the user uses vague or overloaded terms, propose a precise canonical term.

### Discuss concrete scenarios
Stress-test domain relationships with specific scenarios. Invent edge cases that force precision about boundaries between concepts.

### Cross-reference with code
When the user states how something works, check whether the code agrees. Surface contradictions.

### Update CONTEXT.md inline
When a term is resolved, update `CONTEXT.md` right there. Don't batch these up.

`CONTEXT.md` should be totally devoid of implementation details. It is a glossary and nothing else.

### Offer ADRs sparingly
Only when all three are true:
1. **Hard to reverse**: cost of changing mind later is meaningful
2. **Surprising without context**: future reader will wonder "why?"
3. **Result of a real trade-off**: genuine alternatives, picked one for specific reasons
