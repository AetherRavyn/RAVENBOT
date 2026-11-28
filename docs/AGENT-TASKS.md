# Agent tasks and event waits

A background task is durable work one agent starts for another (or for
itself). It has its own thread and run, explicit resource locks, optional
dependencies, and a result the owner can wait for. A wait is one blocked run's
claim on a task transition, a task message, or a timer.

This is the generic task layer. It does not replace `delegate` (synchronous
handoff), offices (planned rooms), channel tasks, or routines (schedules). It
fills the gap between them: work that outlives the caller's tool call but still
belongs to a conversation.

## Lifecycle

```
queued ──claim──▶ starting ──run row──▶ running ──outcome──▶ completed
   │                  │                    │                 failed
   │                  │                    └──owner close──▶ cancelled
   └──dependency fail ┴──owner close──────────────────────▶ cancelled/failed
```

- **queued** — rowed, with a visible task thread and the first prompt already
  written. It waits for dependencies and its resources.
- **starting** — claimed by the host loop; the run row is being created.
- **running** — the target agent is executing on the task thread.
- **completed / failed / cancelled** — terminal. The result or error is stored
  and a `task` event is fired.

A crash cannot leave a task silently running: startup reconciliation (and the
task loop's own first tick) marks `starting`/`running` tasks failed with
`app quit while the background task was running`, and expires every wait left
in flight.

## Tools

| Tool | What it does | Risk |
|---|---|---|
| `start_task` | Creates a task with title, instruction, optional expected result, resources and dependencies. Returns `task_id` and `thread_id`. | High |
| `message_task` | Appends a message to an owned task. Steers the active run when it is at a tool boundary; otherwise records the message and says so. | High |
| `close_task` | Cancels an owned queued/starting/running task and asks its run to stop. It does not invent a result. | High |
| `await_event` | Blocks on a `task` event filter or a `timer`. Fires once, is consumed once, and returns `expired` at the timeout. | High |
| `list_waits` | Lists this run's waits. Never another run's. | High |
| `close_wait` | Cancels one of this run's waits. | High |

Start a task:

```json
{
  "target": "Backend",
  "title": "Review the migration",
  "instruction": "Read migrations/082 and report whether the rollback is safe.",
  "expected_result": "A verdict with the exact statements that would fail.",
  "resources": ["browser", "workspace:/home/me/work/api"],
  "depends_on": ["<earlier-task-id>"]
}
```

Wait for its outcome:

```json
{
  "event_kind": "task",
  "filter": { "task_id": "<task-id>", "state": "completed" },
  "timeout_ms": 60000
}
```

An empty filter matches any terminal task event or message for the caller. A
filter with only `task_id` deliberately ignores the bookkeeping `running`
transition and waits for a terminal state or a message. Ask for
`state: "running"` explicitly if you want the start event.

## Scheduling rules

- **One owner.** Only the bot that created a task can message or close it.
  The target agent runs it.
- **Dependency order.** All `depends_on` tasks must be completed. A missing,
  foreign, failed or cancelled dependency fails the dependent task with a
  reason instead of leaving it queued forever.
- **Resource locks.** `browser` is one shared desktop lock.
  `workspace:<absolute path>` serializes tasks touching the same path. A
  workspace lock is only a lock; it does not grant filesystem access. The real
  working directory still comes from the task thread and its office.
  `start_task` also adds the thread's resolved workspace as a lock, so tasks in
  one office serialize; use a different workspace for genuinely parallel work.
- **Concurrency cap.** The process runs at most four tasks at once. The explicit
  locks, not the cap, are the correctness boundary.
- **Depth cap.** A task is one handoff deep. Nested tasks and delegations share
  the existing three-hop cap, so an agent cannot create an unbounded chain.
- **Permission.** `start_task` uses the same `delegate_to`/office-roster rule as
  `delegate`. An agent cannot hand work to an agent it could not delegate to.
  A bot may start a task for itself.
- **No silent channel delegation.** The tools are High risk and audited. In
  Review/Auto they park on the normal approval card; in Full the workspace owner
  has deliberately disabled that gate.

## Event waits

`await_event` supports two kinds:

- **task** — a task state change or message. Filter fields are exact matches:
  `task_id`, `state`, `target_bot_id`. No query language, no arbitrary JSON.
- **timer** — a deadline. No filter.

A wait row is created before the run blocks. The host fires matching rows with
a conditional update (`state = 'waiting'`), so a duplicate event or two racing
events can only fire a row once. The blocked tool consumes the payload with a
second conditional update, so it is delivered once. A deadline marks the wait
`expired`; it never hangs.

If the named task is already terminal when the wait is created — for example
after a crash/restart or a very fast task — the wait fires immediately instead
of waiting for a transition that will never happen.

`list_waits` shows `waiting`/`fired` by default; `include_finished: true` also
shows consumed, expired and cancelled rows. `close_wait` cancels a waiting or
fired row and the blocked call returns `cancelled`.

## Actor propagation

A task stores the owner run's `run_actors` row and copies it onto the task run.
The same happens for delegated children and routine tests. This is what keeps an
operator's or paired-chat's task from becoming an owner run for the owner-only
agent/routine configuration tools. A run with no actor row is treated as
read-only by those tools, not as the owner.

## Approval and headless behaviour

- Task runs inherit the creator's approval policy at creation time. An
  interactive task that hits a high-risk tool parks on a normal approval card
  in its thread; the owner can decide it from the desktop or attention surface.
- A headless/auto task is marked auto-allow so it does not park forever. The
  hard safety policy still refuses destructive operations.
- Headless runs may wait for task events and timers. They do not skip
  `await_event`: waiting is the point of an unattended task.
- A `message_task` to a run that is not at a tool boundary is recorded in the
  thread, but the current run may not read it. The result explicitly says
  `delivery: "thread"` rather than pretending it was steered.

## Visibility

The task thread is a real visible conversation for the target agent, titled
`Task: …`. The owner can open it from that agent's history. The task row stores
the result/error, and the host emits `agent-task-changed` with the task id,
thread id, owner, target, state and result.

The **Tasks** rail destination is the cross-agent view of that ledger. It lists
queued, starting, running, failed, completed and cancelled tasks with the
target agent, owner, state, relative time and result/error. Filters split
active, failed and finished work; Open jumps to the task thread, and Cancel
cooperatively stops a running task (or closes a queued one) through
`cancel_agent_task`. The rail shows a badge for failed tasks, and the view
refreshes from `agent-task-changed` without polling.

The agent-facing tools still have no `list_tasks`; the owner agent should use
`await_event` for work it started. A human has the Tasks view instead.

## Crash recovery

- Startup marks `starting`/`running` tasks failed with a cause and expires
  every waiting/fired wait. Replaying a task could duplicate side effects, so it
  is not requeued automatically.
- `park_interrupted_runs` excludes task threads, and `fail_orphaned_runs`
  includes them, so a task run is never resumed as an ordinary 1:1 chat.
- The host loop also performs the same recovery on its first tick, which keeps
  tests and alternate hosts honest.

## Limits

- Only `task` and `timer` events are wired. MCP notifications, webhooks and
  arbitrary external events are not bridged into waits yet.
- No `list_tasks`/`read_task` tool. The owner uses the task id with
  `await_event`; the visible thread and the `agent-task-changed` event carry
  the rest.
- A queued task's thread is created immediately; if creation fails, the orphan
  thread is removed. If a task never starts because the process dies, its
  thread and messages remain as the honest record of the interruption.
- Results and event payloads are bounded (8,000 characters in the task row,
  4,000 per event field, 16 KB per event payload). Long transcripts stay in the
  task thread, not in the wait payload.
- The four-task cap is per process, not a distributed lease. The fleet lock
  still permits one owner process at a time.

## Checks

Automated:

- `cargo test -p ravenbot-core agent_task`
- `cargo test -p ravenbot-db agent_tasks`
- `cargo test -p ravenbot-db list_all_returns_tasks`
- `cargo test -p ravenbot-skills agent_tasks`
- `cargo test -p ravenbot-runtime background_tasks`
- `cargo test -p ravenbot-runtime await_event`
- `cargo test -p ravenbot-runtime start_task`
- `cargo test -p ravenbot agent_tasks::tests`
- `npm test -- agentTasks.test.ts TasksView.dom.test.ts`

Manual: see the background-task section in [LIVE-CHECKS.md](LIVE-CHECKS.md).
