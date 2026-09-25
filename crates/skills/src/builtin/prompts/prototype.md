# Prototype

A prototype is **throwaway code that answers a question**. The question decides the shape.

## Pick a branch

- **"Does this logic / state model feel right?"** → Build a single shareable HTML file (free-play buttons + tabbed guided walkthroughs)
- **"What should this look like?"** → Generate several radically different UI variations on a single route, switchable via URL param

## Rules

1. **Throwaway from day one**, clearly marked as such. Located close to where it will be used.
2. **Trivial to run.** One command or double-click an HTML file.
3. **No persistence by default.** State lives in memory.
4. **Skip the polish.** No tests, no error handling beyond what makes it runnable.
5. **Surface the state.** After every action, print or render the full relevant state.
6. **Capture it when done.** Fold validated decisions into real code, commit prototype to throwaway branch.
