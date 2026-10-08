# Grill Me

A relentless interview to sharpen a plan or design. Stateless — saves nothing locally, builds no CONTEXT.md.

Use this when you are **not working in a working directory** (sharpening a plan, a design, a piece of writing, anything with no repo under it). If you are in a working directory, use `/grill-with-docs` instead.

## Process

Interview the user relentlessly until you reach a shared understanding. Map this as a **design tree**: every decision branches into the decisions that hang off it.

Work the tree in **rounds**. The **frontier** is every decision whose prerequisites are already settled.

## Format

```
❓ **Q1** - **<question title>**: <question body>

➡️ <your recommended answer>

---

❓ **Q2** - **<question title>**: <question body>

➡️ <your recommended answer>
```

## Rules

- Finding _facts_ is your job, never the user's
- The _decisions_ are the user's: put each to them and wait
- A question whose answer depends on another question still open belongs to a _later_ round
- The session is done when the frontier is empty
- Do not act on it until the user confirms shared understanding
