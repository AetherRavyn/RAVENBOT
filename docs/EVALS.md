# Agent evals

The deterministic agent-task suite lives in
`crates/runtime/tests/agent_evals.rs`. It uses a scripted provider, a temporary
SQLite database, and no network.

Run it with:

```sh
cargo test -p ravenbot-runtime --test agent_evals
```

Scenarios:

1. **Tool loop** — the model calls `memory_save`, the runtime executes it, feeds
   the result back, and persists the final answer with a tool trace.
2. **Truncated stream** — a provider emits half an answer and stops without a
   terminal signal; the run becomes `PartialSuccess` and the transcript carries
   an incomplete marker.
3. **Workflow group loop** — a two-task group runs twice, stops on `APPROVED`,
   and publishes only the final iteration.
4. **Denied approval** — a high-risk shell call is denied; the command does not
   run, and the model receives the denial and finishes the turn.

The suite is intentionally small and deterministic. It is a release gate, not a
benchmark; model-quality benchmarks belong in a separate opt-in harness.
