# Learning from a run

RAVENBOT can turn a completed run's **tool trace** into a reviewed, versioned
skill. This is the first learning loop, and it is deliberately small:

- the synthesis is deterministic, not a model call;
- the result is a draft, never a silently installed skill;
- a person reviews and edits the document before it is written;
- secrets are redacted and raw tool output is never read.

The explicit limit: **v1 learns from agent runs, not from human demonstration.**
Watching a person use the browser or desktop, capturing that sequence, replaying
it, and repairing it when the UI changes remain open work. There is no
autonomous self-modification: saving a skill requires the same explicit action
as writing one by hand.

## What is learned

A draft is derived from the successful tool calls of one completed run:

1. The run must be `Completed` with outcome `Success` or `PartialSuccess`.
2. RAVENBOT reads the thread's messages and chooses the assistant text message
   with the most non-empty tool traces created during the run window (with a
   10-second clock skew).
3. The user message immediately before that answer becomes the goal, which
   seeds the skill name, description and example prompt.
4. Each successful call becomes a step. Consecutive identical calls are
   collapsed, failed calls are excluded and named in warnings, and a browser
   `snapshot`/`click`/`fill`/`press` run or a `computer_control` run becomes one
   readable interaction step.
5. Steps are capped at 50, with a warning when later calls are dropped.
6. A complete `SKILL.md` is generated with YAML frontmatter (`name`,
   `description`, `example-prompt`) and sections for when to use, steps, tools,
   verification, evidence and safety.

If the run has no successful step, the command returns a clear error instead of
an empty skill.

## What is not learned

- **No results or test claims.** Tool results are never copied into the draft.
  The verification section says plainly that RAVENBOT did not re-run the task.
- **No reasoning.** The model's private reasoning is not part of the tool trace
  and is not used.
- **No failed path.** Failed calls appear only as warnings, never as steps.
- **No human demonstration.** There is no screen recording, click capture,
  replay or DOM-repair loop in v1.
- **No autonomy.** The draft is inert until the person saves it.

A generated skill is therefore a **reproducible outline**, not proof that the
workflow is correct or complete.

## Redaction rules

Redaction happens before a value enters the draft, not as a display filter.

Sensitive JSON keys are redacted recursively. Key matching is case-insensitive
and ignores punctuation, so all of these are covered:

`password`, `passwd`, `secret`, `token`, `api_key` / `apikey`,
`authorization`, `cookie`, `private_key`, `session`, `credential`.

Common credential shapes are redacted inside free text:

- `sk-…`, `sk-proj-…` and Stripe-style `sk_live_…` / `pk_test_…` keys;
- GitHub `ghp_…`, `gho_…`, `ghu_…`, `ghs_…`, `ghr_…` and `github_pat_…`;
- Slack `xoxb-…`, `xoxp-…` and related `xox?-` tokens;
- `Bearer …` values (the scheme is kept, the token is replaced);
- AWS access key ids (`AKIA…`, `ASIA…`, and related prefixes);
- long `KEY=value` assignments whose key contains a sensitive word.

Payload fields are removed entirely, never merely truncated: typed text, file
contents, patches, request bodies, page scripts, HTML and document data. A step
keeps the shape of the action ("Fill `#email`", "Write `notes.md`") without the
value. Serialized step arguments are capped at 2 KB, and a larger argument set
is replaced with a note rather than dumped into the skill.

Raw tool results are not read at all. Summary strings are also redacted, so a
credential embedded in a shell command or URL cannot survive under a harmless
key.

The scanner is pattern-based. It is deliberately broad and will sometimes
redact a harmless field. It cannot prove that every secret shape is covered, so
the review step remains mandatory.

## Review before install

The **Learn from this run** action opens a draft dialog with:

- the proposed title, description and id;
- the step list, with tool chips and summaries;
- the distinct tools the run used;
- warnings for failed calls, truncation, redaction, omitted payloads and
  high-risk tools;
- an editable `SKILL.md` textarea.

Nothing is written until **Save & install** or **Save without installing** is
pressed. Installing copies the reviewed version into the active skill folder;
saving without installing leaves it in the versioned library for later.

## Provenance

A learned skill is stored through `LocalSkillLibrary::create_with_source`, not
by writing files directly. Its index record uses the source string
`learned:run:<run-id>`, and the first version note names the source run and
agent. The library list and view expose that provenance. Versions remain
immutable, as they are for every locally authored skill.

v1 refuses a duplicate skill id with a clear message. It does not try to decide
whether a second draft was meant to revise an existing learned skill; edit the
id (or use the existing skill's revision flow) and save again.

## High-risk warnings

The draft warns when it includes tools that can change the machine or the
outside world:

- `shell_exec`;
- `computer_control` (any action);
- `browser_dom` `click`, `fill`, `press`, `reload` or `eval`;
- `file_write`, `file_edit`, `code_edit`;
- write-like MCP tools (names containing write/create/update/delete/send/…);
- non-read `http_request` methods.

A warning is information, not a permission change. Each future run still goes
through the approval policy in force at that time.

## Limits and residual risk

- A synthesized step sequence may include necessary-but-incidental actions, or
  omit a step the model performed outside the selected trace.
- Grouping browser and desktop actions is heuristic; an unusual action appears
  as a plain step.
- Redaction is pattern-based and cannot guarantee that every unlabelled secret
  is caught. Payload omission is the stronger defence.
- Only one run is learned at a time. There is no cross-run induction, no
  success-rate estimate and no replay validation.
- A `PartialSuccess` run is eligible, but its warnings may describe incomplete
  work.
- Human demonstration capture, replay, and repair are not implemented.
