# RAVENBOT competitive and code audit — 2026

> **Status:** current as of 8 November 2026. This document supersedes
> [`COMPARISON.md`](COMPARISON.md) for competitive positioning; `COMPARISON.md`
> remains the historical analysis of the first OpenBot rebuild.
>
> **Scope:** live product research on OpenBot (openbot.run, Synthetify Labs),
> Grok Bot (xAI/Cursor) and Muse (Meta), plus a code-level audit of RAVENBOT at
> `ee948fc` with the trust and runtime fixes landed in the working tree.
>
> **Rule:** a capability counts as shipped only when this repository contains
> the code path and a test. Marketing copy is not evidence.

---

## 0. Executive summary

RAVENBOT has the strongest **local execution substrate** of the four products
and the weakest **product wrapper** around it.

We lead on:

- local-first operation with no required server or account;
- model/provider breadth (20 providers, custom endpoints, Ollama default);
- multi-agent offices with a real planner, DAG, per-node threads, shared
  workspace, board, queue and autonomy;
- connector breadth (135 MCP connectors, OpenAPI catalog, 1,497 community
  skills, own MCP server);
- artifacts and exports (charts, Mermaid, KaTeX, DOCX/PDF/PPTX/HTML,
  1920×1080 designed decks);
- price and licensing (MIT, bring your own keys).

We are behind on the things that decide adoption:

1. **Activation** — the API-key wall. OpenBot wins by using the ChatGPT,
   Claude, Gemini, Grok, Cursor or Muse subscription the user already pays for.
2. **Reach** — the daemon and remote read-only client now survive the window,
   but there is no mobile write client and no hosted 24/7 computer. OpenBot has
   mobile/web remote access; Grok Bot and Muse are 24/7 cloud computers.
3. **Trust** — the vault and oversight ladder shipped; unsigned artifacts, real
   Windows execution and an external security review remain.
4. **Retention** — the job library, paused installs and personal goals shipped;
   demonstration learning remains, and memory is lexical by default unless a
   semantic backend is configured.
5. **Team** — one human at the keyboard. OpenBot has multiplayer and roles;
   Grok Bot and Muse define the consumer assistant bar.

**The strategic choice:** do not out-cloud Grok or Muse, and do not try to beat
OpenBot at being a generic chat shell. Win as the **sovereign, auditable agent
fleet for people whose work cannot leave their machine** — then earn the right
to be a daily assistant by fixing trust, activation and always-on operation.

The single sentence to protect:

> **RAVENBOT is the agent workspace you can audit, run offline, and hand real
> work to — with a team of agents, not one chat.**

---

## 1. Method and sources

### Products studied

| Product | Owner | Architecture | Positioning |
|---|---|---|---|
| **OpenBot** | Synthetify Labs | Electron/desktop + optional hosted server + iOS/Android/web | Local-first team of agents using existing AI subscriptions |
| **Grok Bot** | xAI / Cursor | Persistent cloud computer + desktop/iOS | Always-on AI teammates that sign into real tools |
| **Muse** | Meta | Dedicated cloud VM (`Muse Secure VM`) + iOS/Android/Mac/web/WhatsApp | One proactive personal agent for everyday life |

### Sources

- <https://openbot.run/> and its `/compare/grok-bot`, `/compare/muse`,
  `/compare/best-ai-agent-apps`, `/pricing`, `/teams`, `/download` pages
  (checked November 2026).
- <https://x.ai/bot>, <https://x.ai/news/introducing-grok-bot>, and the
  Grok Bot docs linked from those pages (checked November 2026).
- <https://introducing.muse.ai/> and the Meta Muse launch material linked from
  it (checked November 2026).
- RAVENBOT source at `ee948fc` plus the working-tree trust/runtime fixes.
- Three independent code audits performed for this document: backend/runtime,
  frontend/UX, and security/privacy. Findings are cited by `file:line`.

### Caveat

Competitor pages are marketing plus documentation, not independently verified
implementation. Where a claim matters (e.g. “all data stays local”), the
document says what the vendor states and what RAVENBOT can prove about itself.

---

## 2. What each competitor actually sells

### 2.1 OpenBot — the closest competitor

OpenBot is the product RAVENBOT will lose to if the product layer is not fixed.
It is free for personal use, open source under PolyForm Noncommercial, and runs
on macOS, Windows, Linux, iOS and Android, with a web app and optional hosted
servers in the EU.

Its core ideas:

- **Use the plans people already pay for.** Codex (ChatGPT), Claude Code,
  Gemini, Grok, Cursor, Cline, Muse Code, Pi, Inception Mercury, any ACP agent,
  Ollama/LM Studio, or an OpenAI-compatible endpoint. Each agent can use a
  different provider and model, and can move to a backup model when a plan
  hits its limit.
- **One persistent workspace per agent.** Each agent has a name, instructions,
  its own workspace and conversation. Moving an agent to another provider
  keeps the role, workspace and conversation.
- **Agents work as a team.** A lead hands work to other agents in shared
  channels; agents message each other, hand off tasks, share files and review
  results.
- **Workflows.** Combine agents, models and approval steps into reusable
  multi-step processes.
- **Multiplayer.** Coworkers join one host; roles decide which agents a member
  can view, chat with, or manage.
- **Remote access.** iPhone, Android and web connect directly to the host over
  an encrypted connection; private-network mode uses Tailscale or the same
  Wi-Fi. No VPN or tunnel.
- **The Vault.** OS-encrypted logins, payment cards and contact info on the
  user's computer; agents see only the card name and last four digits, and the
  first card fill on each site needs approval. 1Password and Bitwarden
  integrations.
- **Voice.** Local speech-to-text and text-to-speech in 31 languages.
- **Chat apps.** Slack, Discord and Telegram.
- **15 languages**, macOS/Windows/Linux, iOS/Android, web.
- **Hosted/VPS option** in the EU from €20/$25 a month.
- **Commercial license** required for work use.

OpenBot's weakness is also its shape: it is a wrapper around external agent
CLIs. The agent loop, tool safety, cleanup and provider quirks belong to
someone else's binary. That is why “one agent, many providers” is an
integration achievement, not a runtime moat. RAVENBOT owns the loop; it should
make that ownership visible as **auditability and reliability**, not just as a
feature list.

### 2.2 Grok Bot — the consumer/team assistant bar

Grok Bot (xAI, built with Cursor) is included with Premium/Super/Ultra and
Cursor plans. Its pitch:

- **A persistent cloud computer** shared by all Bots on the account. Bots sign
  into real apps and websites, including tools with no API or MCP, and keep
  working when the laptop is closed.
- **Message a Bot like a teammate** on desktop or iOS. Bots remember
  conversations, learn how the user works, and only come back when something
  needs approval.
- **Many Bots in parallel**, with a chief-of-staff pattern, group chats of two
  to six Bots, and Bot-to-Bot messages/handoffs.
- **“Show it once.”** A Bot watches a workflow, saves it as a routine, takes
  corrections, and runs it next time.
- **A job marketplace**: Sales Outbound, Talent Scout, Paid Media, Expense
  Manager, Product Performance, Bug Reproduction, Account Health, Chief of
  Staff — each with a job description.
- **Routines/schedules**, shared logins/files/browser, approvals/Auto Review.
- **Teams/Enterprise** with DLP, certificates, proxies and network controls.
- **Pricing:** $30/$100/$200 with weekly usage; Teams/Enterprise via sales.

Grok Bot's weakness is structural for RAVENBOT's audience: the work and the
logins live on xAI/Cursor's cloud, in the United States, and model choice is
managed. It is the best “give it to a teammate” experience; it is not
sovereign.

### 2.3 Muse — the proactive personal agent

Muse is Meta's consumer agent, US/Canada, free with a weekly limit or $20/$100
a month. Its pitch:

- **One personal agent** on a dedicated `Muse Secure VM` — a Linux computer
  with a browser and storage that keeps working after the app closes.
- **Proactive and always working.** It can send messages without being
  prompted, monitors schedules/events, and only surfaces something when it is
  meaningfully new or needs input.
- **A long-running main chat + side chats.** Not turn-by-turn; you can
  interrupt or send multiple tasks. Persistent memory across conversations.
- **Goals tab** and plans. **Artifacts**: interactive documents, dashboards,
  trackers, study guides. Chat bubbles mark where one thought ends.
- **Custom name, avatar and personality.**
- **Transparency**: status snippet under the avatar, full activity log,
  permissions UI, editable memory files.
- **Deterministic approvals** for irreversible actions (purchases, emails) with
  clear accept/reject cards; default browsing is allowed, hard-to-undo actions
  stop.
- **Sentinel**: a separate security agent that approves actions through
  connected services and network traffic; one-time virtual cards; no card
  details exposed to the agent or shop.
- **WhatsApp**, iOS/Android/Mac/web; AI glasses planned.
- **Model:** Muse Spark only; no provider choice.

Muse's weakness is the opposite of RAVENBOT's: one agent, one model, one cloud,
one region. It is the best “it noticed and did it for me” experience; it is not
a fleet or a team.

---

## 3. Gap matrix

Legend: **L** = RAVENBOT leads · **P** = parity / both credible · **B** =
competitor materially ahead · **A** = RAVENBOT absent.

### 3.1 Product surface

| Axis | RAVENBOT today | vs OpenBot | vs Grok | vs Muse |
|---|---|---|---|---|
| Onboarding | Provider key or detected local Ollama + The Raven default agent; missing setup reopens after login; no setup checklist | **B** | **B** | **B** |
| Activation | User must connect a key, name an architect, create an office, invent a mission | **B** | **B** | **B** |
| Provider / subscription access | 20 API providers, custom endpoints, Ollama; **no ChatGPT/Claude/Cursor subscription bridge** | **B** | **L** | **L** |
| Always-on / cloud | Routines, offices, schedules, webhooks, and the queue run inside the app process; closing the window hides to the tray, and Settings → Local can launch the app at login. Quit still stops them; no OS service yet | **B** | **B** | **B** |
| Remote / mobile | No HTTP control API, no auth, no mobile/web client | **A** | **A** | **A** |
| Team / multiplayer | Offices and internal channels, isolated locals accounts with Argon2id and expiring sessions, shared workspaces with owner/operator/viewer grants and a server-side IPC gate; per-run actor attribution and concurrent windows remain future | **B** | **P** | **L** |
| Channels | Internal channel timeline + task board; native Telegram (long polling), Slack (Socket Mode), Discord (gateway), WhatsApp (signed Cloud API webhook) and Microsoft Teams (Bot Framework) with pairing and approval routing | **B** | **P** | **B** |
| Workflows / jobs | Reusable office DAG templates with versions, conditional branches, explicit input ports, per-task retries, per-task JSON Schema outputs, bounded per-task loops, multi-task loop groups, explicit trigger/agent/LLM/approval/output node types, four built-in editable templates, one-click, scheduled and loopback webhook-triggered runs, live run history, cron routines, office queue; no GitHub triggers or agent/job marketplace | **B** | **B** | **B** |
| Delegation | Real `delegate`, depth guard, DAG planning, parallel nodes, shared status file | **L** | **P** | **L** |
| Demonstration learning | Nothing; no capture/replay | **P** | **B** | **P** |
| Goals / proactivity | Office goal + heartbeat; no personal goals tab, no proactive suggestions | **P** | **B** | **B** |
| Memory | Per-bot/per-office memory, RAG, self-review, editable/deletable in the UI with `MemoryChanged` audit events; source/thread/run provenance; FTS5 BM25 candidate retrieval reranked by word/bigram/character-gram vectors (migration 75), so a query normally scores only the lexical candidates instead of every stored vector and a memory without an embedding is still findable; opt-in Ollama/OpenAI semantic embeddings with atomic reindexing and `MemoryEmbeddingsUpdated` audit events; fixed 50-fact hybrid Recall@5/MRR benchmark (local backend 1.00/1.00; old byte hash 0.10/0.10) with a vector-only baseline shown in the UI; near-duplicate detection with previewed removal; LLM summarization with an editable preview and atomic replacement; default remains lexical, not neural | **P** | **B** | **P** |
| Artifacts | Charts, Mermaid, KaTeX, slides, visuals, DOCX/PDF/PPTX/HTML, designed image decks | **P** | **L** | **P** |
| Chat apps | None native; MCP connector cards only | **B** | **P** | **B** |
| Marketplace / plugins | 135 MCP connectors, 2,530-app OpenAPI directory, 1,497 skills, versioned local skills, own MCP server | **L** | **L** | **L** |

### 3.2 Platform and trust

| Axis | RAVENBOT today | vs OpenBot | vs Grok | vs Muse |
|---|---|---|---|---|
| Computer use | Real local mouse/keyboard/window control on Linux only; macOS/Windows have no input backend | **B** | **B** | **B** |
| Browser | Native tabs with per-tab webviews/history, tab restore/reorder, automatic per-agent/office profiles plus named shared profiles, DOM actions, take-over, opt-in automatic cookie refusal with per-named-profile overrides; Linux-only DOM, no proxies | **B** | **B** | **B** |
| Credentials / vault | Local Vault with OS-keychain secrets, Bitwarden/1Password/CSV import, agent vault-request cards, browser fill and browser save-to-vault without exposing the secret to the model; provider/MCP/plugin keys and signing seed also keychain-backed | **P** | **B** | **B** |
| Payments | No native payments, spending authorization, or virtual cards | **B** | **B** | **B** |
| Voice | Hands-free STT→send→TTS; STT and TTS prefer the saved OpenAI provider key with an env fallback; no local 31-language stack | **B** | **P** | **P** |
| Safety / oversight | Good primitives (bubblewrap/Seatbelt/Docker, kill switch, budgets, audit, denylist, capability narrowing, per-run Review policy); Review maps to `Auto` so unattended high-risk tools park instead of silently running, agent commands fail closed without a backend, trusted renderers use limits-only fallback; Docker tier now works on macOS too; Windows backend still absent and macOS Seatbelt awaits real-machine validation | **B** | **B** | **B** |
| Permissions / roles | Per-agent capability narrowing; ranks are labels, not authority; no human RBAC | **B** | **B** | **B** |
| Observability | Turn trace, waterfall, run ledger, activity feed, global filtered audit viewer with signed Ed25519 export, in-app verification, explicit retention, per-office cost and budget attribution, configurable budget alerts, opt-in desktop notifications, HTTPS webhook alerts, SMTP email alerts, automatic retention, and scheduled signed backups | **P** | **B** | **B** |
| Reliability | 789 tests, provider governor, connect/idle-read timeouts, terminal-stream detection with partial preservation, deterministic agent-eval suite, fallback, compaction, resumable runs; no auto-update, desktop-bound, Windows shell gap | **B** | **B** | **B** |
| Packaging / distribution | Linux AppImage/deb/rpm, macOS DMG, Windows NSIS/MSI; unsigned, updater empty, **no `LICENSE` file despite MIT** | **B** | **B** | **B** |
| Pricing / licensing | MIT, free, bring-your-own keys, no telemetry; no hosted/support revenue | **L** | **L** | **L** |
| i18n / accessibility | 6 locales with key parity, focus/reduced-motion utilities, and axe checks on core chat/onboarding/run components; hardcoded English remains in several surfaces and there is no Storybook | **B** | **P** | **P** |

### 3.3 Scorecard

Indicative counts across 27 axes:

| Opponent | Leads | Parity | Behind | Absent |
|---|---:|---:|---:|---:|
| **OpenBot** | 3 | 5 | 18 | 1 |
| **Grok Bot** | 4 | 6 | 16 | 1 |
| **Muse** | 5 | 4 | 17 | 1 |

**The pattern:** RAVENBOT wins where the product is **infrastructure** and loses
where it is a **daily assistant**. That is a strategic choice, not an accident.
Choose it deliberately.

---

## 4. Code-level findings

These are verified in this repository. The first group is fixed in the current
working tree; the second is the remaining P0/P1 backlog.

### 4.1 Fixed in the trust pass

| Finding | What was wrong | Fix |
|---|---|---|
| **Workspace drift** | `office_workspace()` created a folder but never marked it; the next call stepped over its own folder and produced `-2`, `-3`, … up to 50. A normal chat could write to a new empty folder every run. | The path helper now writes `.ravenbot-office`; regression test proves two calls return the same path. |
| **Office nodes lost the office** | Graph node threads were never inserted into `chatroom_office_threads`, so `office_id_for_thread` returned `None`. Nodes lost goal/policy/roster/office memory and delegation permission checks saw an empty roster. | `GraphExecutor::with_office()` links every node thread; regression test asserts the row. |
| **Tool results read at the wrong level** | `journal_file_changes`, `extract_sources`, and `extract_image` read fields from the top-level `SkillResult`, but skills put them under `output`. File-change journal, live citations, and screenshots were silently dead. | `tool_payload()` unwraps `output`; regression test passes a real `SkillResult`. |
| **Anthropic streaming failed** | The SSE parser treated `event:` lines as JSON, so every Anthropic stream died on `message_start`; per-chunk `from_utf8_lossy` corrupted split UTF-8. | Real SSE field handling, byte buffering until a complete line, `[DONE]` sentinel, tests. |
| **Failed runs stayed “planning”** | `execute_run` returned `Err` without finalizing the run, clearing the checkpoint, or emitting `done`; agents stayed “thinking” until restart. | Wrapper finalizes the run on error, persists outcome, emits usage/status, clears flags. Cancelled is a successful incomplete exit. |
| **Terminal state was always `Completed`** | `Run::complete` set `Completed` even for `Failure`/`Cancelled`, so the usage ledger could not count failures. | State follows the outcome; checkpoint is cleared on terminal outcomes; test. |
| **No stop path** | A hung run could not be stopped; the composer stayed locked until the IPC promise settled. | ThreadView Stop button binds `cancel_run`; `cancel_office_turn` cancels every non-terminal run linked to an office and pauses its queue. |
| **Docker/SSH shell injection** | `docker` interpolated model text into `sh -c`; `ssh_remote` interpolated host/paths into `sh -c`. A crafted host/args ran local commands. | Both use argv; Docker validates mounts/privileged flags; SSH validates hosts and confines local copy paths; tests. |
| **Secrets crossed IPC** | `get_mcp_server_env` returned real MCP values; `list_custom_providers` returned `headers_json`. The UI claimed write-only. | MCP returns key names with empty values (save keeps existing when blank); provider headers are no longer in the payload. |
| **Secrets inherited by sandboxed commands** | The sandbox child inherited `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, etc.; `env` inside the command leaked them. | `env_clear` + explicit allowlist; test proves a sentinel secret is absent. |
| **Host roots as workspaces** | `/`, `$HOME`, the data root, and system directories could be configured as project folders, making confinement meaningless. | `safe_workspace_root` rejects them; warning and fallback to the default office folder. |
| **Workspace files and `.git` deletable** | Recursive delete guards covered the root but not `.git`, symlink escapes, shell expansion, or `cd` state. | Denylist tracks `cd`, refuses `.git`, command substitution, unexpanded variables, Docker host mounts, and symlink escapes; tests. |
| **HTTP/screenshot risk misclassified** | `http_request` was `ReadOnly`, so `Ask` never gated GET/POST/PUT/DELETE egress; browser `open`/`snapshot`/`text`/`html` were also `ReadOnly`. | `http_request` is `High`; page reads are `Low`; screenshot is `Low`. |
| **Office autonomy defaulted on** | A missing autonomy setting meant `true`; an office could spend money 30–90 seconds after launch. | Default is now `false`; the header toggle is the opt-in. Test updated. |
| **MCP server exposed everything** | `mcp-serve` ran any skill with no approvals and no workspace. | High-risk tools and the team control plane require `RAVENBOT_MCP_ALLOW_DANGEROUS=1`; file tools are confined to `~/RAVENBOT/projects/mcp`. |
| **DB world-readable** | `ravenbot.db` and its `-wal`/`-shm` siblings were created with default permissions. | Data root `0700` and DB `0600` on Unix. |
| **Local LAN sync bound all interfaces** | The unused `LocalSync` listener bound `0.0.0.0` with no auth. | Bound to loopback. |
| **`code_search` glob broken** | `--include=` and the glob were separate argv words, so grep treated the glob as a file. | One `--include=<glob>` argument. |
| **Unbounded tool output** | `git diff`/`log` could inject megabytes into the prompt; file-tree truncation was silent. | 100 KB cap with a truncation marker; file-tree reports `truncated` and `total`. |
| **Self-review polluted memory** | Every run re-read the whole thread and stored every assistant answer; `is_new` always bypassed dedup. | Only messages from this run, 500-char memory cap, dedup always, usage boost instead of access penalty. |
| **Costs were flat-rate and partly invented** | Every model round was charged at $3/$15 per 1M and every tool call at $0.001, regardless of model or whether the tool cost anything. | Model rounds now use the built-in catalog's input/output prices, local models are free, unknown cloud models keep a conservative estimate, and tool calls no longer add a fake charge. Regression tests cover catalog pricing and a real run's ledger. |
| **Model and tool calls could hang forever** | Provider and network-tool HTTP clients had no connect or read timeout, so a black-holed socket could pin an agent indefinitely. | Every provider and web tool uses a shared client with connect and idle-read timeouts, tunable by environment variable. Tests hold a socket open without replying and assert a timeout. |
| **Startup state was only partly reconciled** | Interrupted runs were closed, but pending HITL rows, queue items left mid-turn, and paused runs were not handled consistently. | Startup now fails orphaned runs/workflow runs, expires pending approvals/questions, marks running queue rows failed with a clear reason, preserves paused checkpoints, clears stale office markers, and resets agent status. A startup test seeds each row and verifies the result; the Runs panel resumes preserved checkpoints. |
| **Budget periods were labels only** | `hourly`/`daily`/`weekly`/`monthly` all accumulated into one lifetime total, so a “monthly” ceiling never reset. | Usage now carries a period key and resets atomically on rollover; lifetime totals are stored separately for telemetry and office baselines, and changing the period starts a fresh window. Tests cover window keys, rollover, resets, and period changes. |
| **Budget edits created duplicate rows** | Every save inserted a new budget row with a new id; reads returned the oldest, so limit and behavior changes were ignored. `Warn` was also treated as a hard stop. | `set_budget` reuses the existing row, migration 49 keeps the newest row per bot and enforces uniqueness, and `Warn` returns `allowed: true` with `should_warn`. Settings now exposes the behavior and a warning notice. |
| **`AskUser` budgets still hard-stopped** | The behavior was accepted but had no flow behind it. | An exhausted `AskUser` budget creates a confirmation card, waits for Continue/Stop, and fails closed on timeout, cancellation, kill switch, or a headless run. Approval continues the run; tests cover the pause-and-resume path. |
| **Sandbox failed open** | A missing bubblewrap backend silently ran commands on the host, and the Docker tier was not wired. | Agent-driven commands now refuse to run when no backend is available. The Docker tier (Linux/macOS) now runs a real read-only container with workspace mounts and blocked egress; the Host tier is the explicit opt-in and `RAVENBOT_ALLOW_UNSANDBOXED=1` is a global override. App-owned renderers still use limits-only fallback. Platform contract tests pin the Windows refusal and the macOS Seatbelt profile, with a live Seatbelt job in CI. |
| **HTTP tools could read cloud metadata** | `http_request` and browser navigation accepted any http(s) URL, including metadata hosts and link-local addresses. Custom MCP URLs and redirects were also unchecked. | A shared guard in `ravenbot-core` refuses non-HTTP schemes, embedded credentials, metadata hostnames, and link-local/unspecified addresses before any request. It is used by HTTP tools, browser navigation, custom MCP saves, and redirect policies in model/tool/MCP clients. Integration and unit tests cover the refusals; public and local-first URLs still work. |
| **Network policy was partly wired** | Narrowed agents had domain grants, but unrestricted agents could reach private hosts and DNS could rebind between validation and request. | Narrowed grants are enforced at the actual URL; arbitrary HTTP tools and MCP HTTP pin the resolved address and revalidate redirects; unrestricted private/LAN access is now an explicit Settings opt-in, with per-agent grants still winning. |
| **Unattended runs bypassed every approval** | Office graph nodes, routines, and channel tasks auto-allowed all gates, and provisioning forced agents to `Full`. | Those runs now use a per-run **Review** policy: high-risk tools gate even for a `Full` bot, delegated children inherit it, and new office agents default to `Auto`. Interactive modes are preserved, and headless CLI keeps an explicit allow-all path. Tests cover the effective-mode and risk decisions. |
| **Closing the window killed every background job** | Offices, routines, schedules, and webhooks lived only as long as the visible window. | A tray icon now hides the window on close and keeps the process running, with Open/Pause/Resume/Quit actions. If the tray cannot start, close still quits. `RAVENBOT_CLOSE_TO_TRAY=0` opts out. |
| **A dropped stream looked like a complete answer** | Providers accepted streams that ended without `[DONE]`, `finish_reason`, `message_stop`, or Ollama `done`. | Every streaming provider now requires its terminal signal. A truncated stream preserves the partial answer, marks the run `PartialSuccess`, inserts an explicit incomplete note, and fails graph nodes rather than publishing them as done. No retry happens after text was streamed. |
| **Re-staffing duplicated every office member** | Role matching excluded existing members, so applying a blueprint twice created a second agent for each role. | Assignments are planned across the whole fleet with the current roster included, one agent cannot fill two roles in one run, and tests cover re-provisioning, duplicate roles, distinct role matches, and stale-member detection. A preview shows the diff and asks before removing obsolete members from the office; agents are never deleted. |
| **Remote access and roles were missing** | The desktop was local-only; there was no authenticated way to read a conversation, answer an approval, or stop a run from a phone, and no role boundary for remote actions. | An opt-in listener now supports one-time pairing, hashed tokens, viewer/operator/admin roles, mutation audit, revocation, and a mobile web client. Device management stays local; the API has no shell/files/settings endpoints. LAN mode is cleartext and documented as tunnel-only. Tests cover pairing replay, role denial, protected reads and the admin-only audit. |
| **Background work died with the window** | The tray kept the process alive, but there was no login service, no crash restart, and no way to run the fleet after logout. | The service now runs a headless `ravenbot daemon` with no window or display. It owns routines, scheduled and webhook workflows, offices, audit maintenance, credential refresh, attention notifications and remote access, restarts on failure, and survives logout on Linux with lingering. An exclusive fleet lock prevents duplicate schedulers; launching the desktop app offers to stop the service and take over. Windows gets a per-user logon task. A clean stop stays stopped. Real logout, macOS LaunchAgent and Windows task checks remain manual. |
| **A second attention surface was missing** | Pending approvals and questions were only visible inside the main window. | A compact always-on-top island polls the same pending rows, shows the newest item and running count, and opens or hides itself. |
| **Tool spend was unpriced and unattributed** | Only a user-configured price was charged, utility calls lived only in the budget counter, and a run could not say whether money went to a model or a tool. | A built-in catalog prices the known paid paths (Tavily search, DALL-E 3 images) with conservative estimates, only while the paid provider is configured; the result's own provider marker keeps the keyless fallback free. User prices win and also cover MCP/plugin tools. Runs carry model and tool buckets, utility calls get their own ledger, `analyze_image` charges its vision tokens, and Usage shows all three amounts plus a paid-tool price audit. |
| **Planning and synthesis were free** | `generate_text` discarded model usage, so office planning, blueprint design, and synthesis never reached budgets or the audit log. | Utility completions return usage, are priced from the model catalog, and are recorded against the bot's budget and audit log. Tests assert planning and synthesis leave a token/cost trace. |
| **Office graph could hang on an early node error** | `execute_node` marked a node running and then used `?` without terminalizing it; a DB/thread error left the office wedged. | The executor wraps node work in a terminal-state guard and fails any node still `Running` after a panicked task, with the normal skip cascade. Regression tests cover a missing bot and a condition-skip distinction. |
| **Office errors were recorded as success** | `office_turn` returned `Ok` with an empty checklist on executor failure; the queue item was marked done. | The turn returns `status: failed` plus the failure list, the queue item is marked failed, and failed/skipped nodes make workflow runs fail. Branch-condition skips stay successful. |

### 4.2 P0 — trust and correctness

> **Update on the secret finding below:** the OS keychain remains the preferred
> store, and a keychain-less host now gets an encrypted, 0600 local vault
> instead of plaintext SQLite. Startup migrates legacy provider, vault, plugin,
> MCP, webhook, signing and app-setting secrets. The remaining boundary is
> explicit: an attacker who can read the user's config directory can read the
> fallback key, which is why the OS keychain remains preferred.
>
> **Update on the memory finding below:** migration 75 adds FTS5 indexes for
> agent and office memory with BM25 ranking. Retrieval now takes a lexical
> candidate set, reranks it with the configured embedding, and keeps a full
> vector pass only for semantic backends. The local backend also moved from a
> byte-position hash to word, bigram and character-gram features. On the fixed
> 50-fact fixture the local path scores 1.00 Recall@5 and 1.00 MRR; the old byte
> hash scored 0.10 and 0.10. The default is still lexical rather than neural —
> Ollama or an OpenAI-compatible endpoint remains the semantic option.

| Finding | Evidence | Impact | Fix |
|---|---|---|---|
| **Resolved: blocking dependency audit** | `deny.toml`, `.github/workflows/ci.yml`, `Cargo.lock` | The security job could not gate anything: cargo-deny's old action could not parse the v4 lockfile, and the advisory backlog was not written down. | cargo-deny v2 is now the blocking gate. Bundled SQLite is 3.54.1 (RUSTSEC-2026-0459), the unmaintained rustls-pemfile is replaced by rustls-pki-types, MIT-0/0BSD are allowed, and the chrono/zip advisories are ignored only with the concrete API non-use documented in `deny.toml`. |
| **Resolved: secrets at rest** | `crates/db/src/secrets.rs`, migration 039 and the startup migration | Provider, vault, plugin, custom-provider header, MCP, webhook, bundle-signing and app-setting secrets now use the OS keychain when available; a keychain-less host gets an encrypted 0600 local vault and the database holds only a marker. Startup migrates legacy plaintext rows. A copied database alone no longer reveals them. | Remaining boundary is explicit: a host that can read the user's config directory can read the fallback key, which is why the OS keychain remains preferred. |
| **Resolved: approval and question waits honor cancel/pause** | `crates/runtime/src/lib.rs` approval, question and budget wait loops | A cancel no longer has to wait for the ten-minute timeout, and a pause is no longer ignored by a parked run. | All three waits poll kill switch, cancellation and pause every 400ms. Pause returns a resumable Paused run with a checkpoint, denies the approval, expires the question, and the Tauri layer emits `paused`; ThreadView reloads the card lists so a stale decision cannot linger. Tests cover approval, question and budget waits, plus prompt cancellation. |
| **Resolved: default memory retrieval** | Byte-position hash, full scan, no FTS | Long-lived agents on the default backend got noisier, not smarter, and retrieval cost grew with the whole memory table. | FTS5 BM25 candidates + vector rerank (migration 75), word/bigram/character-gram local features, a vector-only baseline in the eval, and measured 1.00 hybrid Recall@5 vs 0.10 for the old byte hash. Remaining limit: true paraphrase semantics still need Ollama/OpenAI; no ANN index yet. |

### 4.3 Remaining P1 — frontend and product

> **Update:** every P1 finding in this table now has a fix, and a regression test where one is possible. The remaining product backlog is P2 (splitting the large chat views) and the longer-term epics.

| Finding | Evidence | User-visible effect | Fix |
|---|---|---|---|
| **Resolved: global HITL inbox** | `src/lib/attention.svelte.ts`, `AttentionInbox.svelte`, `Sidebar.svelte`, `WorkspaceRail.svelte` | The inbox existed, but the sidebar's “Waiting on me” filter read the persisted `bot.status`, so after a reload a parked approval could be invisible there while the inbox knew about it. | The sidebar filter and per-agent attention state now read the live attention store; the toggle shows the number of approvals/questions and appears whenever one exists. The offices rail badge uses the same source, so inbox, rail and cards agree after a reload. A failed decision shows a toast and reloads the card, leaving it answerable for retry. |
| **Resolved: conversation history is manageable** | `ThreadView` history drawer and dropdown; `src/lib/chat/threadHistory.ts` | Long-lived agents no longer turn into a pile of indistinguishable `Thread #N` rows. | Rename and two-step delete are available from both the thread dropdown and the history drawer. A blank thread takes its title from the first user message, tool prefixes are stripped, and the last thread opened for each bot is restored after a reload. No archive column was added; delete is the explicit destructive action. |
| **Resolved: load failures look like empty states** | ThreadView, ChatRoomView, ConnectorCenter, WorkspaceBrowser | Users can tell a failed load from an empty workspace and retry without switching away. | Each pane now keeps a distinct loading state and an in-place error with the actual message and a Retry button. ThreadView and ChatRoom suppress the empty state while an error is showing; Connector only shows its empty state after a successful load. |
| **Resolved: draft leak across selection** | `$lib/chat/drafts`, ThreadView, ChatRoomView | Wrong draft or attachment can no longer be sent to the wrong agent. | Drafts are keyed by bot + thread for a 1:1 chat and by room for an office. Attachments and parked decisions reset on switch, and a failed send restores the draft instead of consuming it. |
| **Resolved: office sources missing** | `SourceChips.svelte`, ChatRoomView, ThreadView | Research-heavy office answers now carry the same citations as a 1:1 thread, live and after reload. | `sources` and `image` stream events are routed to the owning office lane and rendered as numbered chips / inline images. Persisted assistant messages render their stored `content.sources` through the shared component; tests cover rendering and opening a chip. |
| **Resolved: room mentions and direct recipients** | ChatRoomView `@` picker; `recipient_bot_id` plumbing | A user can ask one specialist directly instead of always addressing the lead. | The composer filters installed members after `@`, inserts the mention, and sends `recipient_bot_id`. The runtime routes the office queue item to that member; a direct recipient must belong to the office. |
| **Resolved: board cannot be opened** | `OfficeBoard.svelte`, `OfficeTaskCard.svelte`, ChatRoomView `WorkItemDrawer` | “Who did what” is inspectable without leaving the office. | Cards are keyboard-operable buttons with an `onSelect` callback; selecting one opens the durable work-item drawer for that node, with its thread, result, attempts and error. Backlog rows open the same record. |
| **Resolved: narrow layout** | `ThreadView` header, `Workspace.svelte`, `src/lib/workspace/layout.ts`, `src/lib/styles/components.css` | Fixed rail + sidebar + crowded header crushed the conversation around 900–1100px, and opening the dock could still squeeze it at wider windows because the overlay check ignored the sidebar. | The conversation column is a container query: below 820px secondary header actions move into a **More** menu, and below 620px the model/engine pills hide. The dock overlay now mirrors `clamp(460px, 46vw, 820px)` and requires 424px of conversation after the rail and sidebar, so it overlays instead of squeezing. |
| **Resolved: i18n gaps at decision points** | ApprovalCard, QuestionCard, structured blocks, Onboarding, ChatRoom status labels | Non-English users keep their language through setup and every gate. | Allow/Deny, permission/question cards, structured choice/form actions, onboarding copy and live office status labels now use the six-locale dictionary, including their aria labels. The i18n test walks every English key and enforces parity in all six locales. |
| **Resolved: `role="application"` and `select-none`** | Workspace root | Text is selectable and screen readers keep normal document mode. | The root carries neither the application role nor a global `select-none`; a Workspace DOM regression test asserts both. |
| **Resolved: no global error boundary** | `src/routes/+error.svelte`, ThreadView reasoning access | A rendering failure now shows a recoverable screen instead of a blank window, and a malformed message cannot fail on a missing property. | Added a SvelteKit error page with the status, message and Reload action; ThreadView guards `reasoning` to object content before reading it. |
| **Resolved: settings section resets / dead links** | section effect; `target="_blank"` in Tauri | Reopening keeps the section; provider links open through the platform. | Section persistence, `openExternal` routing and provider test errors are shipped. |
| **Resolved: routines wrong-bot list** | load only onMount | Switching agents no longer acts on the previous bot’s routine. | The list reloads on `bot.id` and the webhook header is shown. |
| **Resolved: team import URL mode** | preview fetched, import sent empty paste buffer | A successful preview imports the fetched Markdown. | The backend returns the fetched Markdown and the UI dispatches a fleet refresh. |
| **Resolved: onboarding ready without keys** | `existingAgent` check ran before `hasKeys` | Setup no longer claims readiness when no provider can answer. | The key check runs first and the local fallback is used instead of an invented cloud model. |

### 4.4 P2 — quality and scale

> **Update:** the P2 backlog below has been worked through. The only deliberate
> boundary left is the local embedding model: RAVENBOT ships a lexical feature
> model and uses Ollama/OpenAI for true semantics, because bundling a neural
> model would add tens of megabytes to every build.

| Area | Resolution |
|---|---|
| **Split the chat monoliths** | `ThreadView` (4,696 → 3,853 lines) delegates its header, HITL cards, in-thread search and floating composer to `ThreadHeader`, `ThreadHITL`, `ThreadSearch` and `ThreadComposer`; `ChatRoomView` (4,442 → 4,038) delegates office HITL, its composer/queue capsule and the office header to `OfficeHITL`, `OfficeComposer` and `OfficeHeader`. `ChatMessageRow`, `TurnTrace`, `SourceChips`, `OfficeBoard` and `PlanDag` were already independent. |
| **Transcript loading** | Cursor pages, reply-target hydration across pages, per-row placeholders for malformed rows and `content-visibility` are covered by `messagePaging`, `replyResolution`, `decode` and virtual-row tests. The 50/30 page window is intentional; the measured virtualizer is not needed at that bound. |
| **Internal thread pollution** | Migration 76 adds `threads.kind` and `threads.visible`; node/delegation/routine/channel-task/office-group threads stay queryable by id but are hidden from `list_threads`, remote history and self-review. |
| **IPC types** | `types.ts` covers threads, messages, attention, runs, boards, routines and connectors. `ipc/decode.ts` safely normalizes malformed list rows and is wired into attention, thread history, office messages and status. Remaining `any` is local optimistic UI state, not the IPC boundary. |
| **Paused checkpoint images** | Image bytes are externalized to hashed 0600 files under `cache/checkpoint-images`; the checkpoint stores `checkpoint:<hash>` and resume rehydrates it. `Run::complete` still drops the checkpoint. |
| **Compaction** | The runtime already asks the model for a structured handoff summary; image inputs now contribute 1,024 tokens each to the compaction estimate instead of being invisible. |
| **Mutating-tool races** | Mutating tools serialize per target path; shell/git/Docker/database/package/computer tools share one global lock. Reads remain concurrent. |
| **Retry classification** | `Retry-After` headers are included in provider errors and parsed; 408/425 join 429/5xx, with permanent auth/payment/protocol failures excluded. |
| **Live status** | `get_status` reports distinct active bots, running tasks and today's token/cost totals from the run ledger instead of hardcoded zeroes. |
| **Local embedding** | Accepted boundary: the default is a fast offline lexical feature model; semantic recall is opt-in through Ollama or an OpenAI-compatible embedding endpoint. The 50-fact hybrid benchmark remains 1.00 Recall@5. |
| **Agent-task evals** | `crates/runtime/tests/agent_evals.rs` contains five deterministic end-to-end tasks (tool loop, partial stream, workflow loop, destructive denial, denied approval), backed by the unit suite. |


---

## 5. Five strategic wedges

These are places RAVENBOT can be structurally better than all three, not just
feature-parity.

### Wedge 1 — Provable sovereignty

The only agent platform a regulated buyer can audit, run offline, and prove
what it did.

**Proof today:** bubblewrap namespaces, workspace-aware denylist, kill switch,
budgets, audit log, Ed25519 bundle signing, MIT.
**Must be true:** keychain-backed vault, cross-platform sandbox, signed and
exportable audit trail, reproducible sandbox report, no telemetry, external
security review.
**12-month metric:** 100% of high-risk actions policy-gated and auditable; zero
plaintext secrets; three regulated design partners; external pen-test with no
critical findings.

### Wedge 2 — Agent fleet as infrastructure

A 12 MB local daemon/CLI that other agents drive over MCP.

**Proof today:** 12 MB binary, 75 ms headless cold start, 118 tools over
`mcp-serve`, 20 providers, CLI.
**Must be true:** stable local HTTP API, service mode (systemd/launchd),
container image, SDK, concurrency/queue semantics, structured observability.
**12-month metric:** p95 MCP roundtrip <100 ms; 10 external MCP clients; 1,000
CI runs; one supported container image.

### Wedge 3 — Accountable offices

Multi-agent work that can be inspected, replayed, budgeted, and handed off.

**Proof today:** lead/specialists, LLM planner, DAG with parallel nodes,
blackboard, per-office queue, `Shared/STATUS.md`, per-node threads, persisted
board.
**Must be true:** task-level eval suite, replay, per-office budget/oversight,
job templates, artifact provenance, recovery.
**12-month metric:** 80% success on 20 benchmark office tasks; <5% human
interventions; every deliverable traceable to node/thread/artifact; cost per
office reported.

### Wedge 4 — Offline artifact studio

Consultants, analysts, legal and finance can produce real deliverables without
uploading client data.

**Proof today:** DOCX/PDF/PPTX/HTML, 1920×1080 designed decks, native charts,
Mermaid, KaTeX, sandboxed interactive visuals, export cards.
**Must be true:** artifact library/versioning, brand kits/templates, citations,
reliable Pandoc/XeLaTeX packaging, one-click client-ready export.
**12-month metric:** 10 templates; 95% export success; 5-minute deck from a
brief; three paying pilots.

### Wedge 5 — Local connector fabric

The broadest local tool graph for the long tail.

**Proof today:** 135 MCP connectors, 2,530-app OpenAPI directory, 1,497 skills,
per-bot/batch assignment, local skill versioning, `mcp-serve`.
**Must be true:** verified catalog, signed manifests, per-operation scopes,
vault-backed credentials, sandboxed MCP processes, one-click rollback, honest
install tests.
**12-month metric:** top 50 connectors live-tested; zero unreviewed connectors
in “recommended”; 90% install success; zero credential leaks.

### What to copy, and what not to

- **Copy OpenBot:** subscription detection, first-run completion, remote/mobile,
  multiplayer/roles, design-system gates.
- **Copy Grok Bot:** the oversight ladder as a product concept, six actionable
  roster states, “show it once” demonstration capture, the job library shape.
- **Copy Muse:** deterministic approval cards for irreversible actions,
  editable memory, visible activity log, proactive-but-restrained surfacing.
- **Do not copy:** cloud-only architecture, closed model lock-in, or a job
  marketplace measured by count. Compete on trust, local artifacts, and
  infrastructure.

---

## 6. Five existential gaps

1. **Reach.** The app dies when the laptop sleeps. Routines and offices are
   spawned inside the desktop process; no HTTP control API, no auth, no mobile
   client. Competitors are 24/7. Fix: background service + stable API + paired
   read-only web client in 0–6 months.
2. **Access and activation.** The API-key wall. OpenBot uses existing
   subscriptions; RAVENBOT removed the CLI engine layer. Fix: detect
   Ollama/LM Studio, offer a keyless path, setup checklist, optional
   subscription/ACP bridge without making it core.
3. **Trust.** The vault, oversight ladder and audit UI shipped; unsigned builds
   and the absent Windows backend are the remaining trust gaps. One breach or
   one autonomous destructive action ends the “sovereign” story. Fix: signed
   releases, a real Windows backend, external review.
4. **Retention.** The job library, paused installs, personal goals and trigger
   paths shipped. Demonstration learning is the remaining gap: a toolkit still
   asks the user to choose the work, while an assistant learns it by watching.
   Fix: E14 demonstration learning, then semantic embeddings.
5. **Team.** A team product with one human. No accounts, roles, shared host, or
   per-user audit. Fix after vault/API: accounts + roles + relay + per-user
   audit.

---

## 7. Roadmap

Sequencing rule: **trust and activation before reach; reach before team; team
before marketplace scale.**

### 0–3 months — “trust release,” not a feature release

| Epic | Closes | Acceptance |
|---|---|---|
| **E0 Truth pass** — claims matrix; remove or label unimplemented features | G3 | Every marketed feature links to code + test; “Local provider” removed or experimental; memory described honestly; no claim survives a 10-minute code audit |
| **E1 Secret vault** — keychain migration, vault request cards and manager imports shipped; remaining SQLCipher/encrypted fallback and keychain-backed search | G3 | `strings ravenbot.db \| grep -E 'sk-\|ghp_\|xoxb-'` returns 0 for every secret class; encrypted DB option; redaction tests |
| **E2 Oversight ladder** — Review now maps to `Auto` for office, routine and channel runs; high-risk tools park on the attention card, low-risk writes proceed, and the hard safety policy still refuses destructive operations; per-run `AllowAll` remains the explicit bypass | G3 | 100% high-risk office/routine actions park or deny; 0 forced-`Full` writes; timeout denies; every decision visible |
| **E3 First-run v2** — detect Ollama/LM Studio; keyless path; setup checklist; 3 starter offices; localize onboarding | G2 | Fresh profile + Ollama → first office answer <5 min, 0 API keys; 90% of 10 test users complete unaided |
| **E4 Background service** — tray and login autostart done; systemd/launchd remain. Scheduler/offices run with the window hidden; startup reconciliation handles crash recovery | G1 | Routine fires with window hidden; restart resumes; 0 duplicate runs; idle CPU <1% |
| **E5 Release trust** — `LICENSE`, checksums and a CycloneDX SBOM ship; the release runbook documents macOS notarization, Windows Authenticode and updater secrets, but artifacts remain unsigned until repository secrets are configured | G3 | 100% signed artifacts; updater N-1→N works; 0 Gatekeeper/SmartScreen blocks; `LICENSE` present |

**Do not do in 0–3:** more connectors, more avatar states, a marketplace UI.
Those are not the bottleneck.

### 3–6 months — reach and retention

| Epic | Closes | Acceptance |
|---|---|---|
| **E6 Stable local API + read-only web client + paired device** — tokens, pairing, TLS/origin checks | G1 | Pair one device; three surfaces readable; zero write endpoints; revoke works; p95 <200 ms on LAN |
| **E7 Memory v2** — shipped: editable/deletable memory, provenance, FTS5 hybrid retrieval, Ollama/OpenAI embeddings, the 50-fact Recall@5/MRR benchmark (hybrid and vector-only), duplicate detection, and summarized replacement | G4 | Recall@5 ≥0.8 on a 50-fact fixture vs hash baseline; local hybrid measured 1.00, old byte hash 0.10; zero whole-answer dumps; edits/deletes persist |
| **E8 Job library + triggers** — shipped: 30 curated jobs across eight groups, each with cadence and oversight; install creates a real agent plus a routine paused **in its first row** and a visible welcome thread; inbound webhook content accepts JSON, form-encoded and plain-text bodies, and manual routines use `@manual`; oversight levels map to the interactive approval modes | G4 | 30 jobs with metadata and unique ids; install creates a paused routine and a welcome thread; email-shaped JSON/form fixtures reach the routine as labelled untrusted content; an oversized body is rejected before execution. The 80%-on-20-tasks office benchmark remains open (separate eval work) |
| **E9 Cross-platform execution** — Linux has bubblewrap and Docker; macOS has Seatbelt and Docker; Windows **fails closed** with an explicit error instead of running an unsandboxed command. CI compiles the Tauri contracts and **runs** the Tauri-free sandbox contract on Windows, and executes the service/account/sandbox contracts plus the live Seatbelt test on macOS. macOS 26 needs the address-space and per-user process limits left alone; the Seatbelt profile is the boundary. A native Windows backend (AppContainer/job object) remains open, as does Windows `shell_exec` | G1, G3 | CI on Linux/macOS/Windows; `shell_exec` works on Linux/macOS; Windows refuses with a clear error and a contract test pins that behaviour; sandbox report on all three; browser DOM on Linux/macOS. Windows shell execution is **not** claimed; the Tauri test harness on Windows is compile-only because it lacks an application manifest (tauri#14580) |
| **E10 Audit and fleet observability** — shipped: the live roster is derived from the event stream (sidebar, rail, office header), and every approval gate now writes an `ApprovalDecision` audit row with the tool, outcome, source (desktop/remote/chat/headless/timeout/cancelled/paused/kill switch) and note; HTTPS webhook and SMTP alerts ship with loopback delivery tests | G3 | High-risk calls leave both a request and a decision row across every decision path; the audit viewer filters and searches the new event; remote alert delivery is covered by loopback webhook/SMTP tests. A live internet delivery test still needs a real endpoint. |
| **E11 i18n/a11y completion** — axe checks cover 20+ surfaces; the keyboard audit ships a skip-to-content link, focus restoration, focus-trap wrap tests and arrow/Enter palette navigation. Every Svelte `aria-label` is now an expression backed by the dictionary, and `scripts/check-aria-labels.mjs` is part of `npm run check`, so a literal English accessible name fails the build. Remaining: six more fully translated locales (six of the 12-locale target ship) | G2 | axe zero serious/critical on 20+ surfaces; all 6 shipped locales have key and placeholder parity; no literal English aria-labels (gated); skip link, focus trap/restore and palette arrows tested. The 12-locale target remains open |

### 6–12 months — expansion

| Epic | Closes | Acceptance |
|---|---|---|
| **E12 Multiplayer/team** — shipped: isolated local accounts with Argon2id passwords and sessions, workspace grants, owner/operator/viewer roles enforced in the Tauri IPC gate, a launcher workspace picker, and a Settings → Team panel that adds/creates/changes/removes members. Membership changes write `WorkspaceMembershipChanged` audit rows, and every run records the acting account (`run_actors`) so the Runs panel shows **by alice (operator)** and the audit log joins the same table, displays the account on every run-backed row, and matches it in search. Remaining: remote users sign in as accounts (remote access pairs devices with roles today), a shared-host relay, and task ownership beyond run attribution | G5 | Two accounts can share one workspace on the local host; roles are enforced server-side; membership changes are audited; runs carry the acting account; the zero-cross-tenant-leak audit is executable with `cargo test -p ravenbot --test tenant_isolation` and documented in `docs/TENANT-ISOLATION.md`. Remote account login, the shared-host relay and conflict-free task ownership remain open |
| **E13 Hosted/VPS option** — one-command deploy, EU region, encrypted relay, backup/restore, managed updates | G1, G5 | Deploy <10 min; EU region; encryption at rest/in transit; restore works; 99.5% SLA |
| **E14 Demonstration learning** — capture browser/desktop actions, synthesize skill/routine, review, replay, repair | G4 | Record a 5-step browser task; replay 3/3; user review; repair on DOM change; zero secrets recorded |
| **E15 Subscription bridge** — optional sandboxed Claude Code/Codex/Gemini/ACP adapters with health checks and fallback, or official OAuth where permitted | G2 | One job runs via a subscription; fallback to API; no core dependency; zero crashes; per-adapter health report |
| **E16 Marketplace trust** — verified connectors, signed skill manifests, per-operation scopes, sandboxed MCP, rollback | G3, G4 | Top 50 connectors live-tested; zero unverified in “recommended”; signed manifests; rollback works |
| **E17 Mobile PWA/native** — approvals, notifications, read/write, offline queue | G1 | Push approvals; read/write; offline queue; one device pair; <2 s load on 4G |

### Sequencing gates

- If **E4 (background service)** is not done by month 3, stop feature work. A
  desktop-only assistant is not credible in this market.
- If **E7 (memory)** is not better than keyword search by month 6, remove
  “learning” from the marketing and position as a workflow tool.
- If **E12 (multiplayer)** has no pilot by month 12, abandon team/enterprise and
  focus on the offline artifact studio and MCP infrastructure wedges.
- Do not start **E14 (demonstration learning)** before **E8 (job
  library/oversight)**. A recorded routine without cadence/oversight is a demo,
  not a product.

---

## 8. What would make us lose

1. **A plaintext-key breach.** One leaked `ravenbot.db` ends the sovereign story.
2. **An autonomous office doing something irreversible.** The denylist is good
   engineering, but it is not an oversight model. Forced `Full` on offices and
   auto-allow on routines is a headline waiting to happen.
3. **Shipping “fully offline local models” that error out.** A technical buyer
   will test it in five minutes; the README becomes evidence against us.
4. **Staying desktop-only while competitors are 24/7.** Users will demo
   RAVENBOT and work in Grok/Muse.
5. **Keeping the API-key wall.** OpenBot's “use your existing subscription” is
   the better activation story for most of the market.
6. **Proactivity that still needs a trigger.** The job library and scheduled,
   webhook and MCP triggers ship, but the agent cannot yet learn a workflow by
   watching the user. Fix: demonstration learning (E14).
7. **Fake learning.** Promoting whole answers as memories makes the agent
   noisier, not smarter.
8. **No human multiplayer/roles.** Offices remain a solo demo; OpenBot sells
   teams.
9. **Breadth without depth.** 135 connectors and 1,497 skills but no ten
   workflows that work end-to-end. Support burden eats the team.
10. **Unsigned installers, no auto-update, missing LICENSE, six locales with
    hardcoded English.** It reads as a prototype, not a product.
11. **Cross-platform breakage.** Windows `shell_exec` through `/bin/sh`, macOS
    without a real sandbox. “Three platforms” is a build matrix, not a product.
12. **No agent evals.** 500 tests prove plumbing, not task success.
13. **No business model.** MIT/free is a wedge, but with no hosted/support
    revenue the trust work cannot be funded; adding pricing later antagonizes
    the community.
14. **Chasing Grok's avatar/job surface instead of beating OpenBot on trust and
    reliability.** Feature parity is not a strategy.

---

## 9. Metrics that should decide the roadmap

| Dimension | Metric | Target |
|---|---|---|
| Activation | fresh install → first successful agent answer | <5 min, no API key if Ollama is present |
| Retention | D7 / D30 active workspaces with a completed run | >40% / >20% |
| Reliability | runs ending without a terminal state | 0 |
| Reliability | p95 time to first token | <2 s |
| Reliability | hung runs older than 15 min | 0 |
| Trust | secrets stored outside keychain | 0 |
| Trust | high-risk actions without a policy decision (office/routine) | 0 |
| Trust | audit events missing for tool calls | 0 |
| Memory | Recall@5 on a fixed 50-fact fixture | ≥0.8 (local hybrid 1.00) |
| Offices | success on 20 benchmark tasks | ≥80% |
| Offices | human interventions per task | <5% |
| Cost | usage ledger vs provider invoice | within 2% |
| Reach | routines completed while window closed | 100% |
| Cross-platform | shell/sandbox/browser on Linux/macOS/Windows | all three |

---

## 10. Immediate next actions

The next commit should be judged by whether it makes RAVENBOT **trustworthy**
and **reachable**, not by how many features it adds.

1. Move provider, MCP, plugin and bundle secrets into the OS keychain; migrate
   existing rows; delete plaintext.
2. Remove forced `Full` from office membership and per-run auto-allow;
   implement Hand over / Review / Approve per run; routines default to Review.
3. Fail closed when the sandbox backend is unavailable; implement or refuse
   shell on macOS/Windows; wire the network policy into every HTTP path.
4. Add a circuit breaker and per-call budgets to model/provider/tool calls; connect and idle-read timeouts are in place.
5. Complete startup reconciliation (queue, HITL, paused runs, board nodes).
6. Add a systemd/launchd service; tray plus login autostart already keep routines and offices alive while the app process runs.
7. Make first-run detect Ollama and prove a keyless first answer; detection and keyless agent creation are in place.
8. Add a global approvals/questions inbox and a real thread history sidebar.
9. Add an agent-task eval suite before the next “memory/learning” claim.
10. Sign the installers, configure the updater, add `LICENSE`, and generate the
    SBOM.

Until these land, RAVENBOT is an impressive engineering project with a product
surface that is not yet safe or reachable enough to be the best. After they
land, the wedges in §5 are credible against all three competitors.
