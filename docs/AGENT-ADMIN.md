# Agent and routine administration

Agents can inspect the workspace roster and manage a deliberately narrow slice
of agent and routine configuration. The tools mirror the desktop IPC gate, not
a broader one: anything the owner-only Settings surface owns stays there.

## Role matrix

| Tool | Owner | Operator | Viewer | Paired chat |
|---|---|---|---|---|
| `list_agents` | yes | yes | yes | no |
| `read_agent` | yes | yes | yes | no |
| `create_agent` | yes | no | no | no |
| `update_agent` | yes | no | no | no |
| `interrupt_agent` | yes | yes | no | no |
| `list_routines` | yes | yes | yes | no |
| `create_routine` | yes | no | no | no |
| `update_routine` | yes | no | no | no |
| `delete_routine` | yes | no | no | no |
| `test_routine` | yes | yes | no | no |

This is the same boundary as the IPC commands:

- read → `list_bots`, `get_bot`, `list_routines`;
- operate → `cancel_run`, `run_routine_now`;
- configure → `create_bot`, `update_bot`, `delete_bot`, `create_routine`,
  `update_routine`, `delete_routine`.

A run with **no actor row** is treated as read-only, not as an owner. That is
the fail-closed answer for headless routines, office graph nodes and legacy
runs. The desktop records an owner actor; `delegate`, background tasks and a
routine test copy the caller's actor onto the child run, so a chat/operator
identity cannot launder itself into an owner run. A paired-chat identity
(`role = "chat"`) is not a workspace role and is denied the whole surface.

## Agent tools

- **`list_agents`** returns up to 100 workspace agents with id, name,
  description (clipped to 400 characters), rank, specialty, status, model and
  orchestrator flag; `count` is the total. It does not return custom prompts or
  working-folder paths. Use `read_agent` for one full safe profile.
- **`read_agent`** returns the same profile for one agent, plus timestamps,
  token/temperature limits and sandbox tier. Permission scopes (file roots,
  network domains) are intentionally reduced to names.
- **`create_agent`** is owner-only. It creates a normal agent with the caller's
  provider/model unless one is supplied, and deliberately safe defaults:
  - no permissions, no delegation targets, not an orchestrator;
  - host control off, Ask approvals;
  - only `memory_recall`, `memory_save`, `todo` and `ask_user` skills.
  File, shell, browser, delegation and connector capabilities stay in Settings.
- **`update_agent`** is owner-only and changes identity/model fields only:
  name, description, rank, specialty, provider, model. It cannot change
  permissions, skills, approval mode, host control, working folder or
  delegation targets. Those fields are ignored even if sent.
- **`interrupt_agent`** is owner/operator. It asks every active non-paused run
  for one agent to stop at its next boundary and returns the count and run ids.
  Paused runs stay paused; queued background tasks are not deleted.

## Routine tools

- **`list_routines`** returns a routine's full schedule/instruction metadata for
  one agent, defaulting to the caller.
- **`create_routine`** is owner-only and creates the routine **paused by
  default**, so the scheduler cannot fire unreviewed work. `enabled: true` arms
  it explicitly. The schedule and instruction are validated before storage.
- **`update_routine`** is owner-only and can change name, description, schedule,
  instruction or enabled state. It cannot move the routine to another agent.
- **`delete_routine`** is owner-only and removes the routine and its MCP event
  triggers. Past runs and their threads are not deleted.
- **`test_routine`** is owner/operator and runs the stored instruction once on a
  hidden routine thread. It does **not** update `last_run_at` or the schedule.
  High-risk tools inside the test use the normal Review gates and can park on an
  attention card; the result is a bounded final answer plus run state.

## Visibility and limits

- The tools return routine instructions and agent descriptions. They never
  return provider keys, custom system prompts, working-folder paths or
  permission scopes.
- Every query is scoped to this workspace's database. No tool accepts a
  workspace id, so a caller cannot select another workspace's agent or routine.
- There is no `delete_agent` tool. Deleting an agent can orphan threads, tasks
  and routines; that remains an owner-only Settings action.
- Section/folder organization is not exposed. It has no concrete request yet.
- There is no generic `tool_call` bridge or tool discovery. These ten tools are
  explicit.
- A routine test is a real run: it can take time, use tokens and park on an
  approval. `test_routine` does not cancel the test after the call returns.

## Checks

Automated:

- `cargo test -p ravenbot-runtime agent_admin_tools`
- `cargo test -p ravenbot-skills agent_admin`
- `cargo test -p ravenbot access::tests::agent_admin_tools_match_the_ipc_role_boundary`

Manual: see the agent/routine section in [LIVE-CHECKS.md](LIVE-CHECKS.md).
