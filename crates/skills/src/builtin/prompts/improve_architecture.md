# Improve Codebase Architecture

Surface architectural friction and propose **deepening opportunities**: refactors that turn shallow modules into deep ones.

## Process

### 1. Explore
**Scope before you scan: YAGNI.** Deepening a module pays off by making future changes easier, so put extra weight on parts that have recently changed.

- Walk back commit history to find hot spots
- Read `CONTEXT.md` and ADRs in the area
- Note where you experience friction:
  - Understanding one concept requires bouncing between many small modules
  - Modules are **shallow** (interface nearly as complex as implementation)
  - Pure functions extracted just for testability, but bugs hide in how they're called
  - Tightly-coupled modules leak across seams
  - Parts that are untested or hard to test

Apply the **deletion test** to anything you suspect is shallow.

### 2. Present candidates
For each candidate, provide:
- **Files**: which files/modules are involved
- **Problem**: why current architecture causes friction
- **Solution**: what would change
- **Benefits**: in terms of locality and leverage
- **Before / After**: side-by-side illustration
- **Recommendation strength**: Strong, Worth exploring, Speculative

### 3. Grilling loop
Once the user picks a candidate, walk the decision tree: constraints, dependencies, shape of the deepened module, what sits behind the seam, what tests survive.

## Vocabulary
Use these terms exactly: **module**, **interface**, **depth**, **seam**, **adapter**, **leverage**, **locality**. Don't substitute "component," "service," "API," or "boundary."
