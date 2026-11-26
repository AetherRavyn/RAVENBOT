# Market comparison — November 2026

This is a product and capability comparison, not a benchmark. Competitor
statements come from their public documentation and marketing as of
**26 November 2026**; where a vendor claim could not be checked in code or a
public security document, it is labelled as a claim. RAVENBOT statements are
checked against this repository.

Products in scope:

- **RAVENBOT** — this repository.
- **OpenClaw** — MIT personal/team agent, OpenClaw Foundation.
- **Grok Bot** — xAI/Cursor cloud computer agent.
- **Hermes Agent** — Nous Research open-source agent.
- **Muse** — Meta's consumer personal agent (the product sometimes called
  “MuseBot”; not the small community Telegram/Discord bot of the same name).
- **OpenBot / MausBot** — fast followers; mentioned only where they change the
  bar.

The short version: RAVENBOT is not behind on *agent capability*. It is behind
on **always-on delivery, mobile surface, ecosystem and proof**. Its best market
position is not “the broadest assistant”; it is **the trustworthy agent
operating system for a person or a small team**, with structured offices,
signed audit, local accounts and a real approval ladder. That position is
winnable, but only after the table stakes below are closed.

## 1. Snapshot

Legend: **●** strong, **◐** partial, **○** absent/weak.

| Dimension | RAVENBOT | OpenClaw | Grok Bot | Hermes Agent | Muse |
|---|---|---|---|---|---|
| **Where it runs** | Desktop app; headless daemon; self-hosted | Desktop, server, VPS; no managed tier | Cursor cloud only; no self-host | Desktop, server, VPS, Hermes Cloud | Meta cloud only |
| **Always-on without a laptop** | ◐ daemon exists, but the host is the user's machine | ● on a VPS/server | ● persistent cloud computer | ● Hermes Cloud (paid), or own VPS | ● persistent Secure VM |
| **Desktop platforms** | Linux, macOS, Windows | macOS, Windows, Linux | macOS (desktop app) | macOS, Windows, Linux | macOS (app), web |
| **Mobile** | ◐ responsive remote web client | ● native iOS + Android | ● iOS; Android planned | ◐ chat apps + Termux, no native iOS/Android app | ● iOS + Android |
| **Chat channels** | 5: Slack, Discord, Telegram, WhatsApp, Teams | 20+ (29 listed), DMs and groups | Slack, Teams, X, email; no Telegram/Discord/Signal | 20+, DMs and groups | WhatsApp + own apps |
| **Model choice** | ● BYO OpenAI-compatible, Anthropic, Ollama, custom | ● hosted, subscription-backed, local | ○ managed mix, no model picker | ● hundreds + local | ○ Meta models |
| **Persistent memory** | ● memory, provenance, FTS + optional embeddings | ● memory, provenance, forgetting | ● memory across chats | ● memory, user model, session search | ● memory, editable |
| **Learning from use** | ◐ reviewed skills from completed run traces; no human demonstration or autonomous self-modification | ● learned skills | ● “show it once” routines/skills | ● autonomous skill creation and self-improvement | ● learns, builds its own tools |
| **Multi-agent / teams** | ● offices with roles, DAG, review gates, task ledger and claims | ● shared multiplayer gateway, roles, shared sessions, subagents | ● team of bots, group chats, handoffs | ● subagents, Bot Mode/group chats, profiles | ○ one agent per user |
| **Computer use** | ◐ real mouse/keyboard/window control, drag, app launch and clipboard on Linux; app launch and clipboard on macOS/Windows, no synthetic input there | ● host, browser, nodes | ● cloud computer + optional local macOS/Windows execution | ◐ terminal/browser; Linux desktop-control MCP | ● cloud VM + macOS app control |
| **Approval / oversight** | ● Review→Auto, per-tool approvals, kill switch, budgets, signed audit, tenant isolation | ● policy-as-code, approvals, sandbox optional | ● Auto Review model + approvals, network policy, inference hooks | ● smart/manual/off approvals, hardline blocklist, deny rules, fail-closed | ◐ approvals + read/write permissions; privacy incidents |
| **Enterprise admin** | ◐ local accounts/roles/audit; no SSO/SCIM/OTel/data-residency controls | ● roles, proxy SSO, OTel/Prometheus, signed releases; self-managed | ● SSO/SCIM, network/egress, inference hooks, action recording, OTel, retention, ISO 27001/42001 | ◐ business/enterprise portal, approvals | ◐ business offering, consumer trust issues |
| **Ecosystem** | 135 MCP connectors, 2,530-app OpenAPI directory, 1,497 skills, 30 jobs | ClawHub skills/plugins with scans; MCP, A2A, ACP | Bot marketplace, app connectors, MCP/API | 1,000+ plugins, Skills Hub, MCP | App connectors, can build tools |
| **Public task-success evidence** | ○ no public benchmark | ● ShellBench | ◐ vendor examples, no public benchmark | ◐ trajectory research, no public success suite | ◐ vendor examples |
| **Voice** | ◐ STT→send→TTS via provider key | ● voice nodes, Talk mode, realtime | ● voice notes and calls | ◐ voice memo transcription | ● voice mode |
| **Meetings / email / commerce** | ○ no meeting join, agent inbox or payments | ◐ channels, no native meeting/commerce product | ● meetings, agent email, cards/wallets, purchases | ○ no meeting/commerce product | ● meetings-adjacent, email identity, commerce partners |
| **Localization / a11y** | ● 12 locales, axe gates, keyboard audit | ◐ English-first | ◐ English-first | ◐ English-first | ◐ English-first |
| **Release trust** | ◐ checksums + SBOM; unsigned installers; no updater | ● signed releases, update channels | ● managed app distribution | ● desktop/MSIX installers, update ownership documented | ● managed app |
| **Pricing** | MIT, free, BYO keys | MIT, free, no paid tier | Subscription (Grok/Cursor plans) | MIT; Cloud $20–$200/mo | Free / $20 / $100/mo |

### What the table hides

- OpenClaw's **sandboxing and exec approvals are off by default**. Its own
  “Why OpenClaw” page says one gateway is one trust domain and recommends
  separate gateways for mutually untrusted tenants. Its strength is
  configurability, not safe defaults.
- Grok Bot's **per-user isolation is real** (Firecracker microVM), but all of a
  user's Bots share one computer and one credential set. Grok's own docs say
  not to treat Bots as a security boundary. It is cloud-only and US-hosted.
- Hermes's **self-improving skills** are the strongest learning loop in this
  set. Its security model is explicitly “the OS is the only boundary against
  an adversarial LLM”; it adds approvals, denylists and containers, but the
  defaults are a personal-agent posture.
- Muse's **consumer integrations and commerce** are ahead, but it has had
  public privacy failures: shared chats exposed, notification-derived data,
  training on by default. Trust is its weak point, which is an opportunity for
  RAVENBOT.
- RAVENBOT's **offices** are more structured than “a team of bots”: roles,
  DAG dependencies, review gates, durable work items and claims. That is a
  real product difference, not a UI difference.

### Adjacent products that set expectations

- **OpenBot** (Electron, skills-first agent marketplace, agent templates) sets
  the product-surface bar for templates and polished distribution. It has less
  runtime ownership and security depth.
- **MausBot** (open-source Grok Bot clone on macOS/Windows/Linux, chat-app bots,
  bring-your-own AI account) shows how quickly the “team of bots in a chat”
  surface is commoditizing. The moat is not the surface.
- **Manus, Claude Cowork and ChatGPT dots** are cloud agents with strong
  integrations and managed reliability. They set the expectation that an agent
  works while the laptop is closed, even if they do not offer local ownership.
- **OpenCode / Codex / Claude Code** are coding harnesses, not personal-agent
  operating systems; they matter as model/runtime partners, not as the same
  category.

## 2. Where RAVENBOT already leads

1. **Oversight you can audit.** The approval ladder, per-decision audit rows,
   signed Ed25519 export, per-run actor attribution, kill switch and budgets
   are enforced in code. Competitors have approvals; few have a local,
   verifiable decision ledger.
2. **Tenant isolation with executable evidence.** Local accounts, workspace
   grants, per-user runs and the cross-tenant audit are stronger than the
   “one shared computer” model of Grok/Muse. OpenClaw is explicitly one trust
   domain per gateway; RAVENBOT has a tenant boundary test suite.
3. **Structured offices.** A DAG with specialist roles, review/approval nodes,
   durable work items, leases and human claims is a different category from a
   chat group of agents.
4. **Local-first without a subscription.** BYO models, Ollama, no telemetry,
   OS keychain vault, signed audit and a local database. OpenClaw and Hermes
   share the philosophy; Grok/Muse do not.
5. **Localization and accessibility.** 12 complete locales, a literal
   `aria-label` gate, focus restoration, keyboard audit and axe checks on 20+
   surfaces. None of the compared products ship this breadth.
6. **Breadth of local artifacts.** 1,497 skills, 135 MCP connectors and a
   2,530-app OpenAPI directory, with a skill versioning/install pipeline.

## 3. Where RAVENBOT is behind — and why it matters

### 3.1 Table stakes it does not meet yet

| Gap | Who sets the bar | Why it blocks adoption |
|---|---|---|
| **Always-on hosted/VPS deployment** | Grok Bot, Muse, Hermes Cloud, OpenClaw on a VPS | “24/7” is not credible if the laptop must stay open. The daemon helps, but there is no one-command server deploy, backup/restore or remote upgrade path. |
| **Native mobile approvals** | Grok Bot (iOS), OpenClaw (iOS/Android), Muse (iOS/Android) | The decision that matters is an approval. A responsive web page is not a push notification. |
| **Signed installers and auto-update** | OpenClaw, Grok Bot, Muse, Hermes (installer ownership) | An unsigned desktop agent that cannot update itself reads as a prototype. This is a trust gate, not polish. |
| **Measured task success** | OpenClaw ShellBench; Hermes trajectory research | 1,035 tests prove plumbing. Buyers ask “does it finish the job?” RAVENBOT has no published task-success benchmark. |
| **Channel breadth** | OpenClaw/Hermes 20+, Grok Slack/Teams/X/email | Five channels covers teams, not a personal assistant. iMessage, Signal, Matrix, Google Chat and email are missing. |
| **Shared-host multiplayer** | OpenClaw multiplayer gateway | RAVENBOT has accounts and a remote listener per workspace, but no one host/relay that a team joins with per-user identity and shared sessions. |
| **Windows/macOS execution parity** | Grok Bot (cloud + local), OpenClaw, Hermes | Windows shell fails closed and computer use is Linux-only. “Three platforms” is still partly a build matrix. |

### 3.2 Differentiation gaps

| Gap | Who sets the bar | What “good” looks like |
|---|---|---|
| **Learning loop** | Hermes (self-created skills), Grok Bot (“show it once”) | A successful run becomes a reviewed, versioned skill; a recorded browser/desktop task becomes a replayable routine; failures repair rather than silently repeat. |
| **Independent action reviewer** | Grok Auto Review | A second, independent model evaluates risky actions against the user's request, with deterministic policy underneath it. RAVENBOT's Review→Auto is deterministic and auditable, but not an independent reviewer. |
| **Marketplace trust** | ClawHub scans, Grok marketplace, Hermes 1,000+ plugins | Signed manifests, per-operation scopes, sandboxed MCP, verified badges, rollback, and a live-tested “recommended” set. RAVENBOT has catalog breadth without the trust layer. |
| **Enterprise controls** | Grok Bot | SSO/SCIM, egress allowlists, inference hooks, action recording/OTel, retention, data residency, admin API. RAVENBOT has local roles/audit but no enterprise identity or export plane. |
| **Meetings, agent email, voice calls** | Grok Bot, Muse | Join a meeting and return notes/actions; own an email address; make/receive a call with approvals. RAVENBOT has STT/TTS but not the workflow. |
| **Consumer integrations and commerce** | Muse, Grok Bot | Shopping, cards/wallets, travel, health, relationships. High trust risk; only worth entering with spend controls and insurance. |
| **Release governance** | OpenClaw Foundation, Grok ISO certifications | A neutral steward, signed artifacts, public advisories, independent security review. MIT alone is not a security story. |

## 4. What “best in market” should mean

There are two different markets:

1. **Consumer convenience** — “message a bot, it runs your life.” Grok Bot and
   Muse are ahead on cloud computer, mobile, integrations, voice and commerce.
   RAVENBOT should not try to win this on breadth; it would lose.
2. **Trustworthy autonomous work** — “give an agent real work and real access,
   and be able to prove what it did, stop it, and hand it to a teammate.”
   RAVENBOT can win this, but only if it closes the table stakes and proves
   task success.

The winning sentence should be:

> **RAVENBOT is the local-first agent operating system for work you can audit:
> structured offices, real approvals, signed evidence, and a team that shares
> the work without sharing a single credential.**

That is a narrower claim than “best personal assistant,” and it is defensible.

## 5. Roadmap to be best

Sequenced by adoption risk first, differentiation second. Each item has a
testable acceptance bar.

### P0 — table stakes (next 90 days)

1. **One-command always-on deployment.**
   `ravenbot deploy --target vps` (Docker Compose or systemd), encrypted remote
   access, backup/restore, upgrade/rollback. Acceptance: fresh Ubuntu VPS to a
   working office + mobile approval in under 10 minutes; survives reboot;
   restore verified; no inbound port except the tunnel.
2. **Mobile approvals with push.**
   PWA first, native wrapper later. Approval/deny, run status, question cards,
   push notifications, offline queue. Acceptance: approve from a phone with the
   desktop closed; decision appears in the signed audit; no full chat parity
   required in v1.
3. **Signed releases and auto-update.**
   macOS notarization, Windows Authenticode, Linux signatures, updater
   N-1→N, SBOM and checksums. Acceptance: Gatekeeper/SmartScreen clean;
   update/rollback test; release artifacts verify against the published keys.
4. **Published task-success benchmark.**
   Twenty real office tasks derived from the job library, three seeds, success
   / cost / latency / human interventions. Acceptance: ≥80% success on the
   published set, reproducible in CI, regression gate on every release.
5. **Shared-host relay.**
   One host, multiple members, per-user identity, shared sessions, roles,
   immediate revocation. Acceptance: Alice/Bob/Carol on one host; Carol loses
   access immediately; tenant-isolation suite still passes; no shared
   credential between members.

### P1 — differentiation (90–180 days)

6. **Learning loop and demonstration capture.**
   Turn successful runs into reviewed skills; record a browser/desktop task and
   draft a routine; replay, repair on UI change. Acceptance: record a five-step
   browser task, replay 3/3, review before enable, zero secrets recorded.
7. **Independent action reviewer + policy as code.**
   A second model checks risky actions against the request; deterministic
   denylist, path/network policy and fail-closed timeouts underneath.
   Acceptance: risky actions judged, bypass attempts denied, every decision
   audited, no model-only boundary.
8. **Channel breadth.**
   Add iMessage/Signal/Matrix/Google Chat/email, with DM pairing and group
   allowlists. Acceptance: 10+ channels, unknown senders denied by default,
   one memory across surfaces.
9. **Marketplace trust.**
   Signed skill/connector manifests, per-operation scopes, sandboxed MCP,
   verified badges, rollback. Acceptance: top 50 recommended connectors
   live-tested; zero unverified in “recommended”; install/rollback tests.
10. **Windows and macOS execution parity.**
    Native Windows shell/sandbox, macOS input backend, computer use on both.
    Acceptance: same task runs on Linux/macOS/Windows or fails with an explicit
    unsupported error; contract tests on all three.

### P2 — expansion (180–365 days)

11. **Enterprise identity and export plane.**
    SSO/OIDC, SCIM, OTel export, retention, egress allowlists, data residency,
    admin API. Acceptance: a 50-person pilot can be provisioned and audited
    without a shared local account.
12. **Voice, meetings and agent email.**
    Realtime voice, meeting join/transcribe, a dedicated agent inbox with
    approval-before-send. Acceptance: join a meeting, return notes and actions;
    email always drafts unless explicitly approved.
13. **Subscription bridge.**
    Official/authorized adapters for Claude/ChatGPT/Grok subscriptions with
    health checks and fallback. Acceptance: one job runs through a subscription
    without an API key; fallback to API works.
14. **Commerce and spend controls.**
    Only after budgets/approvals are proven. Virtual cards, merchant scopes,
    per-transaction approval, revocation. Acceptance: no purchase without an
    explicit policy and an audit row.

### Do not build now

- More connector counts or avatar states before task success is measured.
- A broad consumer shopping/commerce surface before spend controls and
  insurance.
- “One shared computer for all agents.” Grok and Muse do it for convenience;
  their own docs say it is not a security boundary. RAVENBOT's per-agent
  sandbox and tenant isolation are better; do not regress them to match.
- RTL locales without a real bidi/layout audit.
- A hosted cloud that weakens the local-first promise or the audit chain.

## 6. The one thing that would change the trajectory

If only one non-table-stakes item can be funded, fund **measured, provable
task success**. OpenClaw has ShellBench; Hermes publishes trajectory work; Grok
and Muse can point to real internal workflows. RAVENBOT currently points to
test counts. A public, reproducible “20 office tasks, ≥80% success, signed
run logs” benchmark would do more for trust than ten new connectors — and it
feeds the demo, the marketplace ranking and the sales conversation.

## 7. Sources

Checked 26 November 2026.

- **OpenClaw** — <https://openclaw.ai/>, README at
  <https://github.com/openclaw/openclaw>, “Why OpenClaw” at
  <https://docs.openclaw.ai/start/why-openclaw>, security at
  <https://docs.openclaw.ai/gateway/security>, multiplayer at
  <https://docs.openclaw.ai/start/multiplayer>.
- **Grok Bot** — launch post at
  <https://x.ai/news/introducing-grok-bot>, overview at
  <https://docs.x.ai/grok-bot/overview>, security at
  <https://docs.x.ai/grok-bot/security>, teams/enterprise at
  <https://docs.x.ai/grok-bot/teams-and-enterprises>, approvals at
  <https://docs.x.ai/grok-bot/approvals-security-and-privacy>, computer/apps at
  <https://docs.x.ai/grok-bot/computer-and-apps>.
- **Hermes Agent** — <https://hermes-agent.nousresearch.com/>, README at
  <https://github.com/NousResearch/hermes-agent>, security at
  <https://hermes-agent.nousresearch.com/docs/user-guide/security>, comparison
  at <https://hermes-agent.nousresearch.com/compare>.
- **Muse** — <https://muse.ai/>, <https://ai.meta.com/muse/>, and the Wikipedia
  summary with its cited reporting:
  <https://en.wikipedia.org/wiki/Muse_(AI_agent)>.
- **RAVENBOT** — `README.md`, `docs/COMPETITIVE-AUDIT-2026.md`,
  `docs/ACCOUNTS.md`, `docs/REMOTE-ACCESS.md`, `docs/TENANT-ISOLATION.md`,
  `docs/LOCALIZATION.md`, and the code/tests in this repository.

Vendor documentation changes quickly. Treat the table as a decision aid, not a
contract; re-check any claim that would carry a purchasing or security
decision.
