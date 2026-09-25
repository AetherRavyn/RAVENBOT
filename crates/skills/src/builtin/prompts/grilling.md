# Grilling

Interview the user relentlessly until you reach a shared understanding. Map this as a **design tree**: every decision branches into the decisions that hang off it.

Work the tree in **rounds**. The **frontier** is every decision whose prerequisites are already settled: the questions you can ask _now_ without guessing at answers you haven't heard yet.

## Format

```
❓ **Q1** - **<question title>**: <question body>

➡️ <your recommended answer>

---

❓ **Q2** - **<question title>**: <question body>

➡️ <your recommended answer>
```

Each round the user answers reshapes the tree: settled decisions push the frontier outward and unblock questions that depended on them. Recompute the frontier and ask the next round.

## Rules

- Finding _facts_ is your job, never the user's. Dispatch sub-agents to look things up.
- The _decisions_ are the user's: put each to them and wait.
- A question whose answer depends on another question still open belongs to a _later_ round.
- The session is done when the frontier is empty: every branch visited, nothing silently assumed.
- Do not act on it until the user confirms shared understanding.
