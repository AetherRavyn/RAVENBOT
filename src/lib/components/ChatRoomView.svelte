<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import * as Avatar from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as Card from "$lib/components/ui/card";
  import { Separator } from "$lib/components/ui/separator";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { getDiceBearUrl, isUserMessage, OFFICE_TEMPLATES } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { t } from "$lib/i18n";
  import { recordUtterance, transcribeBlob, voiceErrorMessage } from "$lib/voice";
  import {
    ACCEPTED_IMAGE_MIMES,
    isTextFile,
    readAsDataUrl,
    readAsText,
    type PendingAttachment,
  } from "$lib/attachments";
  import { onMount, onDestroy, tick } from "svelte";
  import { notify } from "$lib/toast";
  import OfficeSettings from "$lib/components/OfficeSettings.svelte";
  import OfficeMemoryPanel from "$lib/components/OfficeMemoryPanel.svelte";
  import ChatMessageRow from "$lib/components/chat/ChatMessageRow.svelte";
  import PlanDag from "$lib/components/chat/PlanDag.svelte";
  import OfficeBoard from "$lib/components/chat/OfficeBoard.svelte";
  import type { BoardNode, BotTodo } from "$lib/components/chat/OfficeBoard.svelte";
  import RunTimelineStrip from "$lib/components/chat/RunTimeline.svelte";
  import { RunTimeline as RunTimelineState } from "$lib/chat/runTimeline.svelte";
  import { showAuthorHeader, authorHue, isGhostContent } from "$lib/chat/grouping";
  import { fleetActivity } from "$lib/fleetActivity.svelte";
  import { handoffs } from "$lib/handoffs.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { StreamReveal } from "$lib/chat/streamReveal.svelte";
  import { prefersReducedMotion } from "$lib/a11y";
  import {
    Building2,
    Users,
    Send,
    Sparkles,
    CheckCircle2,
    XCircle,
    Loader2,
    Circle,
    ArrowRight,
    AlertTriangle,
    Play,
    Pause,
    Radio,
    Shield,
    Workflow,
    Pencil,
    Layers,
    Cpu,
    Wrench,
    Server,
    Paperclip,
    Mic,
    Terminal,
    Settings,
    ArrowUp,
    ArrowDown,
    Copy,
    Check,
    Brain,
  } from "@lucide/svelte";

  interface Props {
    room: any;
    bots: any[];
  }

  let { room, bots }: Props = $props();

  let members = $state<any[]>([]);
  let messages = $state<any[]>([]);
  let newMessage = $state("");
  let threadId: string | null = $state(null);
  /**
   * Handoffs in this conversation, newest first.
   *
   * Derived from the store rather than copied into local state, so a handoff
   * that settles while the view is open cannot leave a stale row behind.
   */
  const threadHandoffs = $derived(threadId ? handoffs.forThread(threadId) : []);

  let sending = $state(false);
  let chatContainer = $state<HTMLDivElement | null>(null);
  let showOfficeSettings = $state(false);
  // Office Brain drawer — memory is a first-class room surface, not a settings tab only
  let showOfficeMemory = $state(false);
  // Plan mode
  let showPlanModal = $state(false);
  let planGoal = $state("");
  let planTasks = $state<Array<{ id: string; botId: string; label: string; dependsOn: number[] }>>([]);
  let planGenerating = $state(false);
  let planError = $state<string | null>(null);
  let draftingPlan = $state(false);
  let planQuestion = $state<string | null>(null);

  // Hiring / org provisioning
  type HireRole = {
    name: string;
    rank: string;
    specialty: string;
    system_prompt: string;
    is_lead: boolean;
    skills: string[];
  };
  let showHireModal = $state(false);
  let hireRoles = $state<HireRole[]>([]);
  let hireBrief = $state("");
  let hireDrafting = $state(false);
  let hireProvisioning = $state(false);
  let hireError = $state<string | null>(null);
  let hireQuestion = $state<string | null>(null);
  let hireSource = $state<"ceo" | "template" | null>(null);
  let openRolePrompt = $state<number | null>(null);
  let hireResult = $state<string | null>(null);

  // Owner agent management (model / skills / MCP & connectors) for any agent.
  let manageBot = $state<any>(null);
  let showBotSettings = $state(false);
  let showSkillManager = $state(false);
  let notice = $state<string | null>(null);
  let noticeTimer: number | undefined;

  function showNotice(text: string) {
    notice = text;
    if (noticeTimer) window.clearTimeout(noticeTimer);
    noticeTimer = window.setTimeout(() => (notice = null), 5000);
  }

  function openManageAgent(m: any) {
    const bot = m?.bot || bots.find((b: any) => b.id === m?.bot_id);
    if (!bot) return;
    manageBot = bot;
  }

  function handleAgentUpdated(updated: any) {
    if (updated) {
      manageBot = updated;
      window.dispatchEvent(new CustomEvent("bots-changed"));
      showNotice(`${updated.name} updated`);
    }
    load();
  }

  function normalizeRole(r: any): HireRole {
    return {
      name: String(r?.name || "Agent"),
      rank: String(r?.rank || "Member"),
      specialty: String(r?.specialty || "Generalist"),
      system_prompt: String(r?.system_prompt || ""),
      is_lead: Boolean(r?.is_lead),
      skills: Array.isArray(r?.skills) ? r.skills.map(String) : [],
    };
  }

  async function openHireModal() {
    hireError = null;
    hireQuestion = null;
    hireSource = null;
    hireResult = null;
    hireBrief = room.goal || room.description || "";
    showHireModal = true;
    // Empty office: seed the built-in blueprint so there is always a team to
    // approve. Existing office: start empty so the CEO can add missing roles.
    if (members.length === 0) {
      try {
        const roles = (await invoke<any[]>("default_office_org", {
          officeTemplate: room.office_template,
        })) || [];
        hireRoles = roles.map(normalizeRole);
        hireSource = "template";
      } catch (e) {
        hireRoles = [];
      }
    } else {
      hireRoles = [];
    }
  }

  function addHireRole() {
    hireRoles = [
      ...hireRoles,
      { name: "New Agent", rank: "Member", specialty: "Generalist", system_prompt: "", is_lead: false, skills: [] },
    ];
  }

  function removeHireRole(idx: number) {
    hireRoles = hireRoles.filter((_, i) => i !== idx);
  }

  function setHireLead(idx: number) {
    hireRoles = hireRoles.map((r, i) => ({ ...r, is_lead: i === idx }));
  }

  async function draftTeamWithCeo() {
    hireDrafting = true;
    hireError = null;
    hireQuestion = null;
    try {
      const res: any = await invoke("draft_office_org", {
        chatroomId: room.id,
        brief: hireBrief || room.goal || room.description || t("room.staffBrief"),
      });
      hireSource = res?.source || "ceo";
      if (res?.question) {
        hireQuestion = String(res.question);
        return;
      }
      const roles = Array.isArray(res?.roles) ? res.roles : [];
      if (roles.length === 0) {
        hireError = "The CEO couldn't propose a team — add roles manually.";
        return;
      }
      hireRoles = roles.map(normalizeRole);
    } catch (e: any) {
      hireError = String(e);
    } finally {
      hireDrafting = false;
    }
  }

  async function provisionHiredTeam() {
    if (hireRoles.length === 0) {
      hireError = "Add at least one role first.";
      return;
    }
    if (hireRoles.filter((r) => r.is_lead).length > 1) {
      hireError = "Only one lead (CEO) is allowed.";
      return;
    }
    hireProvisioning = true;
    hireError = null;
    try {
      const res: any = await invoke("provision_office_org", {
        chatroomId: room.id,
        roles: hireRoles.map((r) => ({
          name: r.name,
          rank: r.rank,
          specialty: r.specialty,
          system_prompt: r.system_prompt || null,
          is_lead: r.is_lead,
          skills: r.skills,
        })),
      });
      const createdCount = Array.isArray(res?.created) ? res.created.length : 0;
      const reusedCount = Array.isArray(res?.reused) ? res.reused.length : 0;
      hireResult = `Hired ${createdCount} new agent${createdCount === 1 ? "" : "s"} · reused ${reusedCount} existing from the fleet.`;
      window.dispatchEvent(new CustomEvent("bots-changed"));
      await load();
    } catch (e: any) {
      hireError = String(e);
    } finally {
      hireProvisioning = false;
    }
  }

  // User avatar from localStorage (chosen via Settings)
  let userAvatar = $state<string | null>(null);
  // Live office telemetry from the runtime stream (per-agent status + usage)
  let agentStatus = $state<Record<string, string>>({});
  // Live token streams during a team run — keyed by graph NODE id (falling
  // back to `bot:<id>` when a delta can't be attributed), so two parallel
  // nodes run by the same bot never mix. `rounds` keeps the finished rounds'
  // text when the runtime emits `clear` — progress notes, not lost text.
  interface Lane {
    botId: string;
    text: string;
    rounds: string[];
  }
  let lanes = $state<Record<string, Lane>>({});
  // Current tool each agent is running (live)
  let agentTool = $state<Record<string, string>>({});
  // Office board (planner → kanban → live DAG) fed by plan_ready/node_* events
  let boardGoal = $state("");
  let boardNodes = $state<BoardNode[]>([]);
  // Threads owned by the current room run: every node thread opened so far.
  // Events from any other thread are ignored (no cross-run clobbering).
  const activeThreads = new Set<string>();
  const threadToNode = new Map<string, string>();
  // Human-in-the-loop cards: an office node's tool call parks server-side
  // until it is decided/answered, so the room must surface the request.
  interface PendingApproval {
    id: string; bot_id: string; thread_id: string; run_id: string;
    tool_name: string; tool_label: string; arguments: any; risk: string;
    status: string; created_at: string;
  }
  interface PendingQuestion {
    id: string; bot_id: string; thread_id: string; run_id: string;
    header: string; question: string; options: string[]; allow_custom: boolean;
    status: string; created_at: string;
  }
  let pendingApprovals = $state<PendingApproval[]>([]);
  let pendingQuestions = $state<PendingQuestion[]>([]);
  // Per-bot self-tracked checklists (the runtime `todo` tool), shown on the board.
  let botTodos = $state<Record<string, BotTodo[]>>({});
  async function refreshBotTodos(botId: string) {
    if (!botId) return;
    try {
      const rows = (await invoke<any[]>("list_bot_todos", { botId })) || [];
      botTodos = {
        ...botTodos,
        [botId]: rows.map((r) => ({ id: String(r.id), task: String(r.task || ""), done: Boolean(r.done) })),
      };
    } catch {
      /* todos are best-effort decoration */
    }
  }
  let decidingApproval = $state<string | null>(null);
  let answeringQuestion = $state<string | null>(null);
  let questionDraft = $state<Record<string, string>>({});

  async function decideApproval(id: string, allowed: boolean) {
    if (decidingApproval) return;
    decidingApproval = id;
    try {
      await invoke("decide_approval", { approvalId: id, allowed, note: null });
      pendingApprovals = pendingApprovals.filter((a) => a.id !== id);
    } catch (e) {
      notify(`Failed to decide approval: ${String(e)}`, "error");
    } finally {
      decidingApproval = null;
    }
  }

  async function answerQuestion(id: string, answer: string) {
    const value = (answer ?? "").trim();
    if (!value || answeringQuestion) return;
    answeringQuestion = id;
    try {
      await invoke("answer_question", { questionId: id, answer: value });
      pendingQuestions = pendingQuestions.filter((q) => q.id !== id);
    } catch (e) {
      notify(`Failed to answer: ${String(e)}`, "error");
    } finally {
      answeringQuestion = null;
    }
  }

  function approvalSummary(args: any): string {
    try {
      if (args == null) return "";
      if (typeof args === "string") return args.slice(0, 280);
      if (typeof args.command === "string") return String(args.command).slice(0, 280);
      if (typeof args.path === "string" && typeof args.content === "string")
        return `${args.path} (+${args.content.length} chars)`;
      if (typeof args.path === "string") return String(args.path).slice(0, 280);
      if (typeof args.instruction === "string") return String(args.instruction).slice(0, 280);
      if (typeof args.query === "string") return String(args.query).slice(0, 280);
      if (typeof args.url === "string") return String(args.url).slice(0, 280);
      return JSON.stringify(args).slice(0, 280);
    } catch {
      return "";
    }
  }
  function agentNameFor(botId: string): string {
    const m = memberForBot(botId);
    return m?.bot?.name || m?.rank || "Agent";
  }
  // Live activity strip: what each agent is doing right now (P5).
  const runTimeline = new RunTimelineState();
  // Word-by-word reveal per live lane (same engine as ThreadView streaming)
  const laneReveals = new Map<string, StreamReveal>();
  // Lanes without a resolvable key share this inert reveal instead of
  // poisoning the map with a ""-keyed live lane.
  const inertReveal = new StreamReveal();
  function laneRevealOf(key: string): StreamReveal {
    if (!key) return inertReveal;
    let r = laneReveals.get(key);
    if (!r) {
      r = new StreamReveal();
      laneReveals.set(key, r);
    }
    return r;
  }
  $effect(() => {
    const live = new Set<string>();
    for (const [key, lane] of Object.entries(lanes)) {
      live.add(key);
      laneRevealOf(key).track(lane.text);
    }
    for (const [key, r] of laneReveals) {
      if (!live.has(key)) r.track("");
    }
  });

  function memberForBot(botId: string) {
    return members.find((m: any) => m.bot?.id === botId);
  }
  function patchNode(nodeId: string, patch: Partial<BoardNode>) {
    boardNodes = boardNodes.map((n) => (n.nodeId === nodeId ? { ...n, ...patch } : n));
  }
  function upsertNode(node: BoardNode) {
    if (boardNodes.some((n) => n.nodeId === node.nodeId)) patchNode(node.nodeId, node);
    else boardNodes = [...boardNodes, node];
  }
  // Wipe everything tied to the current run (room switch, new dispatch).
  function resetRun() {
    if (commitTimer) {
      window.clearTimeout(commitTimer);
      commitTimer = undefined;
    }
    lanes = {};
    agentStatus = {};
    agentTool = {};
    boardGoal = "";
    boardNodes = [];
    botTodos = {};
    pendingApprovals = [];
    pendingQuestions = [];
    activeThreads.clear();
    threadToNode.clear();
    runTimeline.reset();
  }
  // Hold-until-commit: keep the live lanes rendered until the refetched
  // persisted messages land, THEN fade them out (no blank-gap flash).
  let commitTimer: number | undefined;
  function commitLanes() {
    if (commitTimer) window.clearTimeout(commitTimer);
    commitTimer = window.setTimeout(() => {
      commitTimer = undefined;
      lanes = {};
      agentTool = {};
      agentStatus = {};
      activeThreads.clear();
      threadToNode.clear();
    }, 500);
  }
  function checklistState(s: unknown): BoardNode["state"] {
    switch (String(s)) {
      case "InProgress": return "running";
      case "Completed": return "done";
      case "Failed": return "failed";
      case "Skipped": return "skipped";
      default: return "pending";
    }
  }
  // Fallback board when the run finished while this view wasn't listening
  // (no live events): rehydrate columns from the returned checklist. No deps
  // are available, so the DAG section stays hidden.
  function rehydrateBoard(res: any) {
    if (boardNodes.length > 0) return;
    const list = Array.isArray(res?.checklist) ? res.checklist : [];
    if (list.length === 0) return;
    boardGoal = String(res?.goal || "");
    boardNodes = list.map((it: any, i: number) => ({
      nodeId: `post-${i}`,
      botId: String(it.bot_id || ""),
      label: String(it.label || "Task"),
      dependsOn: [] as string[],
      state: checklistState(it.status),
      preview: String(it.result || ""),
    }));
    persistBoard();
  }
  // Persist the board (fire-and-forget) so a room re-open — or an app
  // restart — shows the latest plan snapshot instead of an empty board.
  function persistBoard() {
    if (!threadId || boardNodes.length === 0) return;
    void invoke("save_office_board", { threadId, goal: boardGoal, nodes: boardNodes }).catch(
      () => {
        /* board persistence is best-effort */
      },
    );
  }
  // Rehydrate the persisted board when (re)opening a room. A stored
  // "running" node only stays Working if the server says its node run is
  // still non-terminal (startup reconciliation guarantees that means LIVE);
  // otherwise it is demoted to pending. Parked HITL cards are re-queried
  // for the room thread and every node thread we know about.
  async function rehydratePersistedBoard() {
    if (!threadId || boardNodes.length > 0) return;
    let saved: { goal?: string; nodes?: any[]; liveNodeThreads?: unknown } | null = null;
    try {
      saved = await invoke<{ goal?: string; nodes?: any[]; liveNodeThreads?: unknown } | null>(
        "get_office_board",
        { threadId },
      );
    } catch {
      return;
    }
    if (!saved || boardNodes.length > 0) return;
    const valid = new Set(["pending", "running", "done", "failed", "skipped"]);
    const liveThreads = new Set(
      (Array.isArray(saved.liveNodeThreads) ? saved.liveNodeThreads : []).map(String),
    );
    boardGoal = String(saved.goal || "");
    boardNodes = (Array.isArray(saved.nodes) ? saved.nodes : [])
      .map(
        (n: any): BoardNode => ({
          nodeId: String(n?.nodeId ?? ""),
          botId: String(n?.botId ?? ""),
          label: String(n?.label || "Task"),
          dependsOn: (Array.isArray(n?.dependsOn) ? n.dependsOn : []).map(String),
          state: valid.has(String(n?.state)) ? (String(n.state) as BoardNode["state"]) : "pending",
          preview: n?.preview ? String(n.preview) : undefined,
          nodeThreadId: n?.nodeThreadId ? String(n.nodeThreadId) : undefined,
        }),
      )
      .filter((n: BoardNode) => n.nodeId)
      .map(
        (n: BoardNode) =>
          n.state === "running" && !(n.nodeThreadId && liveThreads.has(n.nodeThreadId))
            ? { ...n, state: "pending" as const }
            : n,
      );
    // Re-opening mid-run: claim the live nodes' threads so subsequent stream
    // events attribute to THIS room instead of dying at the thread gate.
    for (const n of boardNodes) {
      if (n.state === "running" && n.nodeThreadId) {
        activeThreads.add(n.nodeThreadId);
        threadToNode.set(n.nodeThreadId, n.nodeId);
      }
    }
    await refreshPendingHitl();
  }
  async function refreshPendingHitl() {
    if (!threadId) return;
    const threads = [
      String(threadId),
      ...boardNodes.map((n) => n.nodeThreadId).filter(Boolean).map(String),
    ];
    try {
      const chunks = await Promise.all(
        threads.map(async (t) => {
          const [aps, qs] = await Promise.all([
            invoke<any[]>("list_pending_approvals", { threadId: t }).catch(() => [] as any[]),
            invoke<any[]>("list_pending_questions", { threadId: t }).catch(() => [] as any[]),
          ]);
          return { aps: aps || [], qs: qs || [] };
        }),
      );
      const seenA = new Set(pendingApprovals.map((a) => String(a.id)));
      const addA = chunks
        .flatMap((c) => c.aps)
        .filter((a) => a?.id && !seenA.has(String(a.id)));
      if (addA.length) pendingApprovals = [...pendingApprovals, ...addA];
      const seenQ = new Set(pendingQuestions.map((q) => String(q.id)));
      const addQ = chunks
        .flatMap((c) => c.qs)
        .filter((q) => q?.id && !seenQ.has(String(q.id)));
      if (addQ.length) pendingQuestions = [...pendingQuestions, ...addQ];
    } catch {
      /* HITL re-query is best-effort */
    }
  }
  let officeTokens = $state(0);
  let officeCost = $state(0.0);
  let unlisten: UnlistenFn | null = null;
  $effect(() => {
    if (typeof localStorage !== "undefined") {
      userAvatar = localStorage.getItem("ravenbot_user_avatar");
      const handler = () => (userAvatar = localStorage.getItem("ravenbot_user_avatar"));
      window.addEventListener("user-avatar-changed", handler);
      return () => window.removeEventListener("user-avatar-changed", handler);
    }
  });

  const sampleTasks: Record<string, string[]> = {
 "it-office": [
 "Refactor state management and run full test suites",
 "Design database schema migration for multi-agent workflows",
 "Benchmark runtime latency and identify bottlenecks",
    ],
 "rot-archive": [
 "Transcribe arcane marginalia and verify occult sigils",
 "Formulate antidote elixir against necrotic corruption",
 "Catalog forbidden manuscripts and bind protective wards",
    ],
 "design": [
 "Create high-fidelity dark mode design tokens and components",
 "Audit UX navigation flow and eliminate friction points",
 "Generate animated brand asset library and icon set",
    ],
 "marketing": [
 "Draft multi-channel product launch campaign strategy",
 "Analyze competitor positioning and optimize key messaging",
 "Write high-converting technical release notes and copy",
    ],
 "sales": [
 "Build enterprise prospect qualification matrix",
 "Draft targeted outreach sequence for enterprise tier",
 "Prepare value proposition deck and objection handling",
    ],
  };

  let roomTasks = $derived(
    sampleTasks[room?.office_template] || [
 "Audit current workspace state and execute core plan",
 "Coordinate parallel team review across all disciplines",
 "Synthesize execution deliverables and generate summary report",
    ]
  );

  // Agents with live work, and whether the team is currently active at all
  // (covers Plan mode, which does not set `sending`).
  let laneBotIds = $derived(new Set(Object.values(lanes).map((l) => l.botId)));
  let activeMembers = $derived(
    members.filter((m) => {
      const st = agentStatus[m.bot?.id];
      return (
        (st && st !== "idle") ||
        laneBotIds.has(m.bot?.id) ||
        Boolean(agentTool[m.bot?.id])
      );
    })
  );
  let teamActive = $derived(sending || planGenerating || activeMembers.length > 0);

  let templateInfo = $derived(
    OFFICE_TEMPLATES[room?.office_template as keyof typeof OFFICE_TEMPLATES] || OFFICE_TEMPLATES.custom
  );

  // OpenBot scroll behavior: follow the feed only while stuck to the bottom
  // (within 80px); scrolling up during a run unsticks and reveals the jump
  // pill. `staticEntries` flags bulk history loads so row entrances animate
  // only live appends (see $lib/chat/entrance).
  let stickToLatest = $state(true);
  let staticEntries = $state(true);

  function handleFeedScroll() {
    const el = chatContainer;
    if (!el) return;
    stickToLatest = el.scrollHeight - el.scrollTop - el.clientHeight <= 80;
  }

  function jumpToLatest() {
    stickToLatest = true;
    chatContainer?.scrollTo({
      top: chatContainer.scrollHeight,
      behavior: prefersReducedMotion() ? "auto" : "smooth",
    });
  }

  async function scrollToBottom(force = false) {
    if (!force && !stickToLatest) return;
    stickToLatest = true;
    await tick();
    if (chatContainer) {
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }
  }

  async function load() {
    staticEntries = true;
    try {
      const mems = (await invoke("list_chatroom_members", { chatroomId: room.id })) as any[];
      members = mems.map((m) => ({ ...m, bot: bots.find((b: any) => b.id === m.bot_id) }));
      for (const m of members) refreshBotTodos(m.bot?.id ?? "");
      const tid = await invoke("get_chatroom_thread", { chatroomId: room.id });
      if (tid) {
        threadId = tid as string;
        messages = (await invoke("list_messages", { threadId })) as any[];
        await rehydratePersistedBoard();
      } else {
        threadId = null;
        messages = [];
      }
      await tick();
      scrollToBottom(true);
    } catch (e) {
      console.error(e);
    } finally {
      staticEntries = false;
    }
  }

  $effect(() => {
    if (room?.id) {
      threadId = null;
      messages = [];
      resetRun();
      officeTokens = 0;
      officeCost = 0;
      load();
    }
  });

  onMount(() => {
    listen<any>("agent-stream", (event) => {
        const p = event.payload;
        if (!p) return;
        const kind = String(p.kind || "");

        // Board topology events also own thread attribution, so they are
        // handled before the thread gate below.
        if (kind === "plan_ready") {
          if (p.thread_id) activeThreads.add(String(p.thread_id));
          boardGoal = String(p.goal || "");
          boardNodes = (Array.isArray(p.nodes) ? p.nodes : []).map((n: any) => ({
            nodeId: String(n.node_id ?? ""),
            botId: String(n.bot_id ?? ""),
            label: String(n.label || "Task"),
            dependsOn: (Array.isArray(n.depends_on) ? n.depends_on : []).map(String),
            state: "pending" as const,
          }));
          persistBoard();
          return;
        }
        if (kind === "node_open") {
          const nodeId = String(p.node_id ?? "");
          if (!nodeId) return;
          const nodeTid = p.node_thread_id ? String(p.node_thread_id) : undefined;
          if (nodeTid) {
            activeThreads.add(nodeTid);
            threadToNode.set(nodeTid, nodeId);
          }
          if (boardNodes.some((n) => n.nodeId === nodeId)) {
            // Keep the plan_ready label/deps — just flip to working.
            patchNode(
              nodeId,
              nodeTid ? { state: "running", nodeThreadId: nodeTid } : { state: "running" },
            );
          } else {
            upsertNode({
              nodeId,
              botId: String(p.bot_id ?? ""),
              label: String(p.instruction || "Working"),
              dependsOn: [],
              state: "running",
              nodeThreadId: nodeTid,
            });
          }
          persistBoard();
          return;
        }
        if (kind === "node_finished") {
          const nodeId = String(p.node_id ?? "");
          patchNode(nodeId, {
            state: p.state === "failed" ? "failed" : "done",
            preview: String(p.preview || ""),
          });
          if (Array.isArray(p.skipped)) {
            for (const s of p.skipped) patchNode(String(s), { state: "skipped" });
          }
          // The node's thread is done — any card still parked on it is stale.
          const ntid = [...threadToNode.entries()].find(([, id]) => id === nodeId)?.[0];
          if (ntid) {
            pendingApprovals = pendingApprovals.filter((a) => String(a.thread_id) !== ntid);
            pendingQuestions = pendingQuestions.filter((q) => String(q.thread_id) !== ntid);
          }
          refreshBotTodos(String(p.bot_id || ""));
          persistBoard();
          return;
        }
        if (kind === "graph_status") {
          for (const n of Array.isArray(p.nodes) ? p.nodes : []) {
            if (n?.state) patchNode(String(n.node_id ?? ""), { state: String(n.state) as BoardNode["state"] });
          }
          persistBoard();
          return;
        }

        // Everything else must belong to THIS room's current run — the room
        // thread or a node thread opened by it. (Fixes cross-run/cross-room
        // lanes being clobbered by foreign streams.)
        const tid = p.thread_id ? String(p.thread_id) : "";
        if (tid && tid !== String(threadId || "") && !activeThreads.has(tid)) return;
        runTimeline.track(p);

        if (kind === "status") {
          const state = p.state === "done" ? "idle" : p.state;
          agentStatus = { ...agentStatus, [p.bot_id]: state };
        } else if (kind === "usage") {
          officeTokens += p.tokens || 0;
          officeCost += p.cost || 0;
        } else if (kind === "approval_requested") {
          if (p.approval && !pendingApprovals.some((a) => a.id === p.approval.id)) {
            pendingApprovals = [...pendingApprovals, p.approval];
            scrollToBottom(true);
          }
        } else if (kind === "approval_decided") {
          pendingApprovals = pendingApprovals.filter((a) => a.id !== p.approval_id);
        } else if (kind === "question_asked") {
          if (p.question && !pendingQuestions.some((q) => q.id === p.question.id)) {
            pendingQuestions = [...pendingQuestions, p.question];
            scrollToBottom(true);
          }
        } else if (kind === "question_answered") {
          pendingQuestions = pendingQuestions.filter((q) => q.id !== p.question_id);
        } else if (kind === "delta") {
          // Route the token stream to its NODE lane (parallel same-bot nodes
          // stay separate); unattributed deltas fall back to a per-bot lane.
          const botId = String(p.bot_id || "");
          if (!botId || !memberForBot(botId)) return;
          const key = threadToNode.get(tid) || `bot:${botId}`;
          const cur = lanes[key] || { botId, text: "", rounds: [] };
          lanes = { ...lanes, [key]: { ...cur, text: cur.text + (p.content || "") } };
          scrollToBottom();
        } else if (kind === "tool_started") {
          if (p.bot_id) {
            agentTool = { ...agentTool, [p.bot_id]: p.name || "tool" };
            agentStatus = { ...agentStatus, [p.bot_id]: "running_tool" };
          }
        } else if (kind === "tool_finished") {
          if (p.bot_id) {
            const next = { ...agentTool };
            delete next[p.bot_id];
            agentTool = next;
            if (p.name === "todo") refreshBotTodos(String(p.bot_id));
          }
        } else if (kind === "clear") {
          // A new model round begins: KEEP the finished round's text as a
          // progress note instead of wiping it.
          const key = threadToNode.get(tid) || `bot:${p.bot_id}`;
          const cur = lanes[key];
          if (!cur) return;
          const note = cur.text.trim();
          lanes = {
            ...lanes,
            [key]: { ...cur, text: "", rounds: note ? [...cur.rounds, note] : cur.rounds },
          };
        } else if (kind === "done") {
          // Backend emits `done` only AFTER posting the persisted messages.
          // Our own send()/dispatchPlan() refetches and commits (commitLanes)
          // — no blank-gap flash. If we did NOT start this run (room reopened
          // mid-run), refetch here now that the posts are in the DB.
          if (!sending && tid === String(threadId || "")) {
            void load().finally(() => commitLanes());
          }
        }
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.error("Failed to attach office stream listener:", e));
  });

  onDestroy(() => {
    unlisten?.();
    unlisten = null;
    if (commitTimer) window.clearTimeout(commitTimer);
    for (const r of laneReveals.values()) r.stop();
  });

  function openPlanModal() {
    planGoal = "";
    planTasks = [];
    planError = null;
    planQuestion = null;
    planTasks = members.map((m, i) => ({
      id: String(i),
      botId: m.bot_id,
      label: "Address your part of the goal",
      dependsOn: [] as number[],
    }));
    showPlanModal = true;
  }

  function addPlanTask() {
    planTasks = [...planTasks, { id: String(planTasks.length), botId: members[0]?.bot_id ?? "", label: "New task", dependsOn: [] }];
  }

  function removePlanTask(idx: number) {
    planTasks = planTasks.filter((_, i) => i !== idx).map((t, i) => ({ ...t, id: String(i), dependsOn: t.dependsOn.filter((d) => d !== idx).map((d) => d > idx ? d - 1 : d) }));
  }

  function updatePlanTaskLabel(idx: number, label: string) {
    planTasks = planTasks.map((t, i) => (i === idx ? { ...t, label } : t));
  }

  // Let the room's lead draft the plan from the goal, then let the user edit it.
  async function draftPlan() {
    if (!planGoal.trim()) { planError = "Describe the goal first."; return; }
    planError = null;
    planQuestion = null;
    draftingPlan = true;
    try {
      const res: any = await invoke("draft_office_plan", {
        chatroomId: room.id,
        goal: planGoal,
      });
      if (res?.question) {
        // The lead needs clarification — surface it and don't overwrite tasks.
        planQuestion = String(res.question);
        return;
      }
      const drafted = Array.isArray(res?.tasks) ? res.tasks : [];
      if (drafted.length === 0) {
        planError = "The lead couldn't draft a plan — add tasks manually.";
        return;
      }
      planTasks = drafted.map((t: any, i: number) => ({
        id: String(i),
        botId: t.bot_id,
        label: t.instruction,
        dependsOn: Array.isArray(t.depends_on) ? t.depends_on : [],
      }));
    } catch (e: any) {
      planError = String(e);
    } finally {
      draftingPlan = false;
    }
  }

  function updateTaskBot(idx: number, botId: string) {
    planTasks = planTasks.map((t, i) => (i === idx ? { ...t, botId } : t));
  }

  function togglePlanDep(taskIdx: number, depIdx: number) {
    planTasks = planTasks.map((t, i) => {
      if (i !== taskIdx) return t;
      const has = t.dependsOn.includes(depIdx);
      return { ...t, dependsOn: has ? t.dependsOn.filter((d) => d !== depIdx) : [...t.dependsOn, depIdx] };
    });
  }

  async function dispatchPlan() {
    if (!planGoal.trim()) { planError = "Describe the goal first."; return; }
    if (planTasks.length === 0) { planError = "Add at least one task."; return; }
    planError = null;
    planGenerating = true;
    resetRun();
    // Optimistically show the dispatched goal in the room feed.
    const tempUserMsg = {
      id: "temp-plan-" + Date.now(),
      role: "user",
      content: `🎯 **Plan Mode** — goal: ${planGoal.trim()}`,
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempUserMsg];
    // An explicit dispatch always returns the feed to the bottom (OpenBot).
    scrollToBottom(true);
    // Close the modal immediately: the room streams live lanes while the DAG
    // runs, so the user watches progress instead of a frozen button.
    showPlanModal = false;
    showNotice("Plan dispatched — the team is working. Progress streams below.");
    try {
      const tasks = planTasks.map((t) => ({
        bot_id: t.botId,
        instruction: `Goal: ${planGoal}\n\nYour task: ${t.label}`,
        depends_on: t.dependsOn,
      }));
      // Prefer an explicit orchestrator/lead; otherwise let the backend resolve
      // the room's lead (pass a nil UUID).
      const lead =
        members.find((m: any) => m.bot?.is_orchestrator)?.bot_id ??
        members[0]?.bot_id ??
 "00000000-0000-0000-0000-000000000000";
      const res: any = await invoke("execute_graph", {
        orchestratorBotId: lead,
        goal: planGoal,
        tasks,
        chatroomId: room.id,
      });
      // Reload the room thread the plan was posted into.
      const tid = res.thread_id || threadId;
      if (tid) {
        threadId = tid;
        staticEntries = true;
        messages = (await invoke("list_messages", { threadId: tid })) as any[];
        await tick();
        staticEntries = false;
        scrollToBottom(true);
      }
      rehydrateBoard(res);
    } catch (e: any) {
      notify(`Plan dispatch failed: ${String(e)}`, "error");
    } finally {
      planGenerating = false;
      commitLanes();
    }
  }

  async function send(taskText?: string) {
    const typed = (taskText || newMessage).trim();
    if (sending) return;
    const attachments = pendingAttachments;
    if (!typed && attachments.length === 0) return;
    const text = typed || "(shared files)";

    sending = true;
    newMessage = "";
    pendingAttachments = [];
    resetRun();

    // Optimistically insert user prompt
    const tempUserMsg = {
      id: "temp-" + Date.now(),
      role: "user",
      content: text,
      attachments: attachments.map((a) => ({
        name: a.name,
        mime_type: a.mime,
        is_image: a.isImage,
        data: a.data,
      })),
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempUserMsg];
    // An explicit dispatch always returns the feed to the bottom (OpenBot).
    scrollToBottom(true);

    try {
      const res: any = await invoke("send_to_chatroom", {
        chatroomId: room.id,
        content: text,
        attachments: attachments.length ? attachments : undefined,
      });
      threadId = res.thread_id;
      staticEntries = true;
      messages = (await invoke("list_messages", { threadId })) as any[];
      await tick();
      staticEntries = false;
      rehydrateBoard(res);
      scrollToBottom(true);
    } catch (e) {
      // Never swallow the failure: toast it, and still refetch so persisted
      // agent replies surface even if the call rejected late.
      notify(`Office dispatch failed: ${String(e)}`, "error");
      if (threadId) {
        try {
          staticEntries = true;
          messages = (await invoke("list_messages", { threadId })) as any[];
        } catch {
          /* keep the optimistic row if the refetch also failed */
        } finally {
          staticEntries = false;
        }
      }
    } finally {
      sending = false;
      commitLanes();
      scrollToBottom();
    }
  }

  function statusDot(status: string): string {
    switch (status) {
      case "thinking": return "bg-warning";
      case "running_tool": return "bg-info";
      case "waiting_on_user": return "bg-danger";
      case "paused": return "bg-[var(--brand)]";
      default: return "bg-success";
    }
  }

  // Header roster: live per-member activity from the global fleet state machine
  function rosterRing(act: "working" | "attention" | "responded" | "idle"): string {
    switch (act) {
      case "working": return "ring-2 ring-[var(--brand)]";
      case "attention": return "ring-2 ring-warning";
      case "responded": return "ring-2 ring-success";
      default: return "ring-1 ring-[var(--brand)]/40";
    }
  }
  function rosterDot(act: "working" | "attention" | "responded" | "idle"): string {
    switch (act) {
      case "working": return "bg-[var(--brand)] animate-pulse";
      case "attention": return "bg-warning animate-pulse";
      case "responded": return "bg-success";
      default: return "";
    }
  }

  function formatTime(isoStr: string) {
    if (!isoStr) return "";
    try {
      const d = new Date(isoStr);
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } catch {
      return "";
    }
  }

  let isListening = $state(false);
  let activeRecorder: any = null;
  let pendingAttachments = $state<PendingAttachment[]>([]);

  async function attachFiles(files: FileList | File[]) {
    for (const file of Array.from(files)) {
      const mime = (file.type || "").toLowerCase();
      const ext = (file.name?.split(".").pop() || "").toLowerCase();
      if (ACCEPTED_IMAGE_MIMES.includes(mime)) {
        const base64 = (await readAsDataUrl(file)).split(",")[1] || "";
        if (base64) {
          pendingAttachments = [...pendingAttachments, {
            name: file.name || "pasted-image.png",
            mime: mime || "image/png",
            data: base64,
            isImage: true,
          }];
        }
        continue;
      }
      if (isTextFile(file)) {
        const text = await readAsText(file);
        if (!text) continue;
        newMessage = (newMessage ? newMessage + "\n\n" : "") +
          `Attached file [${file.name}]:\n\`\`\`${ext}\n${text}\n\`\`\`\n`;
        continue;
      }
      const base64 = (await readAsDataUrl(file)).split(",")[1] || "";
      if (!base64) continue;
      pendingAttachments = [...pendingAttachments, {
        name: file.name || "attachment.bin",
        mime: mime || "application/octet-stream",
        data: base64,
        isImage: false,
      }];
    }
  }

  function removePendingAttachment(idx: number) {
    pendingAttachments = pendingAttachments.filter((_, i) => i !== idx);
  }

  function attachFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.onchange = async (e) => {
      const files = (e.target as HTMLInputElement).files;
      if (files?.length) await attachFiles(files);
    };
    input.click();
  }

  async function toggleVoice() {
    if (isListening) {
      activeRecorder?.stop();
      isListening = false;
      return;
    }
    isListening = true;
    try {
      const rec = await recordUtterance({ maxMs: 25000, silenceMs: 1700 });
      activeRecorder = rec;
      const blob = await rec.result;
      activeRecorder = null;
      if (!blob) return;
      const text = await transcribeBlob(blob);
      if (text) newMessage = (newMessage ? newMessage + " " : "") + text;
    } catch (e: any) {
      console.error("Voice input failed:", e);
      alert(voiceErrorMessage(e));
    } finally {
      isListening = false;
    }
  }
</script>

<div class="flex flex-col h-full overflow-hidden select-none bg-[var(--surface-0)] text-[var(--text-primary)] font-sans">
  <!-- Top Office Header Bar -->
  <header class="h-15 px-4 border-b border-[var(--hairline)] bg-[var(--surface-0)] flex items-center justify-between z-10 shrink-0">
    <div class="flex items-center gap-3">
      <!-- Office Avatar -->
      <div class="size-10 rounded-2xl overflow-hidden bg-[var(--surface-3)] border border-[var(--brand)]/40 p-0.5 shrink-0 shadow-md">
        <RavenAvatar name={room.name} imageUrl={room.avatar_url} />
      </div>

      <div class="flex flex-col">
        <div class="flex items-center gap-2">
          <span class="font-bold text-sm text-[var(--text-primary)]">{room.name}</span>
          <span class="font-mono text-[10px] py-[2px] px-2 rounded-md bg-[var(--surface-3)] border border-[var(--hairline)] text-[var(--brand-text)] capitalize">
            {room.office_template.replace("-", " ")}
          </span>
          <button
            type="button"
            class="icon-btn size-6"
            onclick={() => (showOfficeSettings = true)}
            title={t("room.editOffice")}
            aria-label={t("room.editOffice")}
          >
            <Pencil class="size-3.5" />
          </button>
        </div>
        <div class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)] mt-0.5">
          <span class="text-success font-mono flex items-center gap-1">
            <Radio class="size-2.5" />
            {t("room.parallelLane")}
          </span>
          <span class="text-[var(--text-muted)]">·</span>
          <span>{members.length === 1 ? t("room.specialistAssigned", { n: members.length }) : t("room.specialistsAssigned", { n: members.length })}</span>
          <span class="text-[var(--text-muted)]">·</span>
          <span class="font-mono text-[10px] px-1.5 py-0.5 rounded bg-success/10 border border-success/30 text-success" title={t("room.telemetry")}>
            ${officeCost.toFixed(4)} · {officeTokens.toLocaleString()} tok
          </span>
        </div>
      </div>
    </div>

    <!-- Assigned Bot Roster Avatars -->
    <div class="flex items-center gap-3">
      <div class="flex -space-x-2">
        {#each members.slice(0, 5) as m}
          {@const botId = m.bot?.id ?? ""}
          {@const act = fleetActivity.get(botId)}
          <div class="relative size-8 shrink-0 transition-transform hover:scale-110 hover:z-10" title={`${m.bot?.name || m.rank} (${m.specialty})`}>
            <!-- The face follows the work, so a glance at the roster shows
                 which agent is thinking, which needs an answer, and which
                 failed — without reading a single label. -->
            <RavenAvatar
              name={m.bot?.name || m.rank}
              mood={fleetActivity.mood(botId)}
              imageUrl={m.bot?.avatar_url}
              decorative
              class={cn("size-full rounded-full", rosterRing(act))}
            />
            {#if act !== "idle"}
              <span class={cn("absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full ring-2 ring-[var(--surface-0)]", rosterDot(act))}></span>
            {/if}
          </div>
        {/each}
        {#if members.length > 5}
          <div class="size-8 rounded-full bg-[var(--surface-3)] border border-[var(--hairline)] ring-1 ring-[var(--brand)]/40 flex items-center justify-center text-[10px] font-bold text-[var(--brand-text)]">
            +{members.length - 5}
          </div>
        {/if}
      </div>
      <button type="button" onclick={() => openHireModal()} class="size-8 rounded-xl bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-success/50 flex items-center justify-center shrink-0 ml-1" title={t("room.hire")} aria-label={t("room.hire")}>
        <Users class="size-4" />
      </button>
      <button type="button" onclick={() => (showOfficeSettings = true)} class="size-8 rounded-xl bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] flex items-center justify-center shrink-0 ml-1" title={t("room.settings")} aria-label={t("room.settings")}>
        <Settings class="size-4" />
      </button>
      <button type="button" onclick={() => (showOfficeMemory = true)} class="size-8 rounded-xl bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] flex items-center justify-center shrink-0 ml-1" title={t("memory.title")} aria-label={t("memory.title")}>
        <Brain class="size-4" />
      </button>
      <button type="button" onclick={() => openPlanModal()} class="size-8 rounded-xl bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] flex items-center justify-center shrink-0 ml-1" title={t("room.plan")} aria-label={t("room.plan")}>
        <Workflow class="size-4" />
      </button>
    </div>
  </header>

  <!-- Team strip: stable chips (no layout shift, status as a dot) -->
  {#if members.length > 0}
    <div class="px-4 py-2.5 bg-[var(--surface-1)] border-b border-[var(--hairline)] flex items-center gap-3 shrink-0">
      <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-muted)] shrink-0 flex items-center gap-1.5">
        <Workflow class="size-3 text-[var(--brand)]" />
        Team
        <span class="font-mono text-[var(--text-muted)]">{members.length}</span>
      </span>
      <div class="flex items-center gap-1.5 min-w-0 overflow-x-auto no-scrollbar">
        {#each members as m}
          {@const st = agentStatus[m.bot?.id] || "idle"}
          <button
            type="button"
            onclick={() => openManageAgent(m)}
            class="inline-flex items-center gap-2 h-8 pl-1 pr-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--brand)] hover:bg-[var(--surface-3)] shrink-0 transition-colors cursor-pointer"
            title={`${m.bot?.name || m.rank} — ${m.specialty}${st !== "idle" ? ` · ${st.replace("_", " ")}` : ""} (click to manage)`}
          >
            <span class="relative shrink-0">
              <RavenAvatar
                name={m.bot?.name || m.rank}
                mood={fleetActivity.mood(m.bot?.id ?? "")}
                imageUrl={m.bot?.avatar_url}
                decorative
                class="size-6 rounded-md"
              />
              <span class="absolute -bottom-0.5 -right-0.5 size-2 rounded-full ring-2 ring-[var(--surface-2)] {statusDot(st)}"></span>
            </span>
            <span class="text-[11px] font-semibold text-[var(--text-primary)] whitespace-nowrap">{m.rank}</span>
            <span class="hidden xl:inline text-[10px] text-[var(--text-muted)] truncate max-w-[8rem]">{m.specialty}</span>
            {#if st !== "idle"}
              <Loader2 class="size-3 text-warning animate-spin shrink-0" />
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Office board: planner → kanban → live dependency graph -->
  {#if boardNodes.length > 0}
    <div class="px-4 pt-2.5 shrink-0 min-w-0">
      <OfficeBoard goal={boardGoal} nodes={boardNodes} {members} todos={botTodos} runActive={teamActive} />
    </div>
  {/if}

  <!-- Run timeline: live "who is doing what" pills from agent-stream -->
  {#if runTimeline.events.length > 0}
    <RunTimelineStrip
      events={runTimeline.events}
      nameFor={(id: string) => members.find((m: any) => m.bot_id === id)?.bot?.name || t("ui.fallbackAgent")}
    />
  {/if}

  <!-- Transient notice (hiring / agent updates) -->
  {#if notice}
    <div role="status" class="px-4 py-1.5 bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[11px] text-[var(--text-secondary)] flex items-center justify-between gap-3 shrink-0 animate-fade-in">
      <span>{notice}</span>
      <button type="button" onclick={() => (notice = null)} class="text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer" aria-label={t("room.dismiss")}>✕</button>
    </div>
  {/if}

  <!-- Main Chat & Task Feed -->
  <div class="flex-1 flex flex-col overflow-hidden bg-[var(--surface-0)]">
    <div class="relative flex-1 min-h-0">
    <div bind:this={chatContainer} onscroll={handleFeedScroll} class="h-full overflow-y-auto p-4 sm:p-6 space-y-6 {staticEntries ? 'entries-static' : ''}">
      <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto space-y-6">
        {#each messages as msg, mi (msg.id || msg.created_at)}
          {@const isUser = isUserMessage(msg)}
          {@const isError = typeof msg.content === "string" && msg.content.includes("⚠️ **Model Error:**")}
          {@const rawText = typeof msg.content === "string" ? msg.content : msg.content?.text || JSON.stringify(msg.content)}
          {@const senderBot = msg.sender_bot_id ? bots.find((b: any) => b.id === msg.sender_bot_id) : null}
          {@const senderName = senderBot?.name || msg.sender_name || room.name}
          {@const senderAvatar = senderBot?.avatar_url || getDiceBearUrl(senderName, senderBot?.avatar_style || "bottts")}

          <ChatMessageRow
            {isUser}
            {isError}
            text={rawText}
            time={formatTime(msg.created_at)}
            grouped={!showAuthorHeader(messages, mi)}
            gutter={!isUser}
            gutterName={showAuthorHeader(messages, mi) ? (senderName || senderBot?.name || "") : ""}
                  gutterImage={senderAvatar}
                  gutterMood={fleetActivity.mood(senderBot?.id ?? "")}
            author={!isUser && !isError && showAuthorHeader(messages, mi)
              ? { name: senderName, specialty: senderBot?.specialty }
              : null}
            authorColor={`hsl(${authorHue(msg.sender_bot_id || senderName)} 55% 68%)`}
            userAvatar={userAvatar || getDiceBearUrl("You", "micah")}
          >
            {#snippet errorCard()}
              <div class="rounded-2xl p-4 bg-[var(--danger-soft)] border border-[var(--danger-border)] text-[var(--text-secondary)] space-y-2">
                <div class="flex items-center gap-2 text-[var(--danger-text)] font-semibold text-xs">
                  <Shield class="size-4 shrink-0" />
                  <span>{t("room.pipelineError")}</span>
                </div>
                <p class="text-xs text-[var(--text-secondary)] leading-relaxed font-sans">
                  {rawText.replace("⚠️ **Model Error:** ", "")}
                </p>
              </div>
            {/snippet}
          </ChatMessageRow>
        {:else}
          <!-- Empty State: Modern Office Mission Control -->
          <div class="p-8 text-center border border-dashed border-[var(--hairline)] rounded-xl bg-[var(--surface-0)]/80 max-w-xl mx-auto my-6 shadow-xl space-y-4">
            <div class="size-16 rounded-2xl bg-[var(--surface-2)] border border-[var(--hairline-strong)] mx-auto flex items-center justify-center text-[var(--brand-text)] shadow-2xl">
              <Building2 class="size-8" />
            </div>

            <div>
              <h3 class="font-bold text-base text-[var(--text-primary)] tracking-tight">
                {t("room.workspaceTitle", { name: room.name })}
              </h3>
              <p class="text-xs text-[var(--text-tertiary)] mt-1 max-w-md mx-auto leading-relaxed">
                {room.description || t("room.defaultDesc")}
              </p>
            </div>

            <!-- Pre-configured Team Lane Overview -->
            {#if members.length > 0}
              <div class="flex items-center justify-center gap-3 pt-2">
                {#each members as m}
                  <div class="flex flex-col items-center gap-1">
                    <div class="size-9 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline-strong)] shadow-md">
                      <RavenAvatar name={m.rank} imageUrl={m.bot?.avatar_url} />
                    </div>
                    <span class="text-[10px] font-bold text-[var(--text-secondary)] font-mono">{m.rank}</span>
                  </div>
                {/each}
              </div>
            {:else}
              <!-- No team yet: let the CEO staff the office -->
              <div class="p-4 mt-2 rounded-2xl border border-success/50 bg-success/15 text-left space-y-2">
                <div class="text-xs font-bold text-success flex items-center gap-1.5">
                  <Users class="size-3.5" /> {t("room.noTeam")}
                </div>
                <p class="text-[11px] text-[var(--text-tertiary)] leading-relaxed">
                  {t("room.ceoStaffDesc")}
                </p>
                <button
                  type="button"
                  onclick={() => openHireModal()}
                  class="h-8 px-3 rounded-xl bg-success/15 border border-success/40 text-success hover:bg-success/25 text-xs font-bold flex items-center gap-1.5 cursor-pointer"
                >
                  <Sparkles class="size-3.5" /> {t("room.staffOffice")}
                </button>
              </div>
            {/if}

            <!-- Quick Starter Tasks -->
            <div class="space-y-2 pt-4 text-left">
              <span class="text-[10px] font-bold text-[var(--text-muted)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                <Sparkles class="size-3 text-[var(--brand-text)]" />
                Collaborative Directives:
              </span>
              <div class="grid grid-cols-1 gap-2">
                {#each roomTasks as task}
                  <button
                    type="button"
                    class="text-left text-xs p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-2)] transition-all text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-between group cursor-pointer"
                    onclick={() => send(task)}
                  >
                    <span class="truncate">{task}</span>
                    <ArrowRight class="size-3.5 text-[var(--brand-text)] opacity-0 group-hover:opacity-100 transition-opacity shrink-0 ml-2" />
                  </button>
                {/each}
              </div>
            </div>
          </div>
        {/each}

        <!-- Human-in-the-loop cards from office nodes -->
        {#each pendingApprovals as ap (ap.id)}
          {@const deciding = decidingApproval === ap.id}
          <div class="w-full max-w-[min(42rem,78%)] rounded-2xl border {ap.risk === 'high' ? 'border-warning/40' : 'border-[var(--hairline)]'} bg-[var(--surface-1)] p-4 space-y-2.5 shadow-xl">
            <div class="flex items-baseline justify-between gap-3 min-w-0">
              <div class="text-[13px] font-semibold text-[var(--text-primary)] truncate">
                {agentNameFor(ap.bot_id)} wants to {ap.tool_label || ap.tool_name}
              </div>
              <span class="shrink-0 font-mono text-[10px] text-[var(--text-muted)]">{ap.tool_name}</span>
            </div>
            {#if approvalSummary(ap.arguments)}
              <pre class="max-h-32 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-black/50 border border-[var(--hairline)] px-3 py-2 font-mono text-[11.5px] leading-relaxed text-[var(--text-secondary)]">{approvalSummary(ap.arguments)}</pre>
            {/if}
            {#if ap.risk === 'high'}
              <p class="text-[11px] text-warning/90">{t("thread.highStakes")}</p>
            {/if}
            <div class="flex items-center gap-2">
              <button
                type="button"
                disabled={deciding}
                onclick={() => decideApproval(ap.id, true)}
                class="h-8 px-4 rounded-full bg-success text-[var(--text-primary)] text-xs font-bold hover:bg-success transition-colors cursor-pointer disabled:opacity-50"
              >
                {deciding ? 'Allowing…' : 'Allow'}
              </button>
              <button
                type="button"
                disabled={deciding}
                onclick={() => decideApproval(ap.id, false)}
                class="h-8 px-4 rounded-full border border-danger/40 text-danger text-xs font-bold hover:bg-danger/15 transition-colors cursor-pointer disabled:opacity-50"
              >
                Deny
              </button>
              <span class="text-[10px] font-mono text-[var(--text-muted)] ml-1">the run resumes after you decide</span>
            </div>
          </div>
        {/each}

        {#each pendingQuestions as q (q.id)}
          {@const answering = answeringQuestion === q.id}
          <div class="w-full max-w-[min(42rem,78%)] rounded-2xl border border-[var(--brand)]/40 bg-[var(--surface-1)] p-4 space-y-3 shadow-xl">
            <div class="flex items-baseline justify-between gap-3 min-w-0">
              <div class="text-[13px] font-semibold text-[var(--text-primary)] truncate">{q.header || 'Question'} — {agentNameFor(q.bot_id)}</div>
              <span class="shrink-0 font-mono text-[10px] text-[var(--brand-text)]/80">ask_user</span>
            </div>
            <p class="text-[12.5px] leading-relaxed text-[var(--text-secondary)] whitespace-pre-wrap">{q.question}</p>
            {#if q.options?.length}
              <div class="flex flex-wrap gap-2">
                {#each q.options as opt}
                  <button
                    type="button"
                    disabled={answering}
                    onclick={() => answerQuestion(q.id, opt)}
                    class="h-8 px-3.5 rounded-full border border-[var(--brand)]/40 text-[var(--brand-text)] text-xs font-semibold hover:bg-[var(--brand-soft)] transition-colors cursor-pointer disabled:opacity-50"
                  >
                    {opt}
                  </button>
                {/each}
              </div>
            {/if}
            {#if q.allow_custom !== false}
              <div class="flex items-center gap-2 min-w-0">
                <input
                  type="text"
                  aria-label="Answer"
                  bind:value={questionDraft[q.id]}
                  onkeydown={(e) => { if (e.key === 'Enter') answerQuestion(q.id, questionDraft[q.id] ?? ''); }}
                  placeholder={t("thread.answerPlaceholder")}
                  class="flex-1 min-w-0 h-9 rounded-xl border border-[var(--hairline)] bg-black/50 px-3 text-xs text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--brand)]/50"
                />
                <button
                  type="button"
                  disabled={answering || !(questionDraft[q.id] ?? '').trim()}
                  onclick={() => answerQuestion(q.id, questionDraft[q.id] ?? '')}
                  class="h-9 px-4 rounded-full bg-[var(--brand)] text-[var(--text-on-light)] text-xs font-bold hover:bg-[var(--brand-hover)] transition-colors cursor-pointer disabled:opacity-50 shrink-0"
                >
                  {answering ? 'Sending…' : 'Answer'}
                </button>
              </div>
            {/if}
          </div>
        {/each}

        <!--
          Agent-to-agent handoffs, in flight.

          A delegation used to leave one trace: a tool result inside the calling
          agent's message. In an office that reads as the lead going quiet, and
          "asked a colleague to check the logs" and "stuck" look identical. This
          is the edge, while it is an edge — and a refused one too, because an
          agent that cannot ask for help is the case a user needs explained.
        -->
        {#if threadHandoffs.length}
          <div class="space-y-1.5" aria-live="polite">
            {#each threadHandoffs as h (h.key)}
              {@const from = memberForBot(h.fromBotId)}
              {@const to = memberForBot(h.toBotId)}
              {@const fromName = from?.bot?.name || from?.rank || t("ui.fallbackAgent")}
              {@const toName = to?.bot?.name || h.toBotName || t("ui.fallbackAgent")}
              <div
                class="raven-handoff animate-rise-in"
                class:raven-handoff--refused={Boolean(h.error)}
                class:raven-handoff--done={h.done && !h.error}
              >
                <span class="flex items-center gap-1.5 shrink-0">
                  <span class="font-bold text-[11px] truncate max-w-28" style={`color: ${authorHue(h.fromBotId || fromName)}`}>
                    {fromName}
                  </span>
                  {#if h.done && !h.error}
                    <Check class="size-3 text-success shrink-0" />
                  {:else if h.error}
                    <AlertTriangle class="size-3 text-warning shrink-0" />
                  {:else}
                    <Loader2 class="size-3 animate-spin text-[var(--brand-text)] shrink-0" />
                  {/if}
                </span>
                <ArrowRight class="size-3 text-[var(--text-muted)] shrink-0" />
                <span class="font-bold text-[11px] truncate max-w-28" style={`color: ${authorHue(h.toBotId || toName)}`}>
                  {toName}
                </span>
                <span class="flex-1 min-w-0 text-[11px] text-[var(--text-muted)] truncate" title={h.instruction}>
                  {h.error ?? h.instruction}
                </span>
                {#if h.response}
                  <button
                    type="button"
                    class="text-[10px] text-[var(--brand-text)] hover:underline shrink-0 cursor-pointer"
                    aria-expanded={h.expanded}
                    onclick={() => handoffs.toggle(h.key)}
                  >
                    {h.expanded ? t("wsb.hideReply") : t("wsb.showReply")}
                  </button>
                {/if}
              </div>
              <!--
                The reply, inline. An office has exactly one thread and the
                delegated agent's is a different one, so this is the only place
                its answer can appear — and the handoff row saying "asked Sam"
                without showing what Sam said is a handoff the user still has to
                take on trust.
              -->
              {#if h.response && h.expanded}
                <div class="raven-handoff__reply">
                  <MarkdownRenderer content={h.response} />
                </div>
              {/if}
            {/each}
          </div>
        {/if}

        <!-- Live team activity: streamed node lanes through the shared row shell -->
        {#if teamActive}
          <div class="space-y-3">
            <div class="flex items-center gap-2 text-[10px] font-bold uppercase tracking-wider text-[var(--text-muted)]">
              <span class="size-1.5 rounded-full bg-success animate-pulse"></span>
              Live team activity · {activeMembers.length > 0 ? `${activeMembers.length} working` : "orchestrating"}
            </div>

            {#snippet statusRow(name: string, avatar: string, color: string, toolName?: string)}
              <div class="flex items-center gap-2 px-1 min-w-0 animate-rise-in">
                <img src={avatar} alt="" class="size-6 rounded-lg object-cover border border-[var(--hairline)] shrink-0" />
                <span class="text-[11px] font-bold truncate" style={`color: ${color}`}>{name}</span>
                {#if toolName}
                  <span class="font-mono text-[9px] px-1.5 py-0.5 rounded bg-info/10 border border-info/30 text-info inline-flex items-center gap-1 shrink-0">
                    <Wrench class="size-2.5" /> {toolName}
                  </span>
                {/if}
                <span class="rounded-[var(--radius-bubble)] px-3.5 py-2 bg-[var(--surface-1)] inline-flex gap-1 items-center">
                  <span class="typing-dot"></span><span class="typing-dot"></span><span class="typing-dot"></span>
                </span>
              </div>
            {/snippet}

            {#each Object.entries(lanes) as [key, lane] (key)}
              {@const m = memberForBot(lane.botId)}
              {@const st = agentStatus[lane.botId] || "idle"}
              {@const reveal = laneRevealOf(key)}
              {@const toolName = agentTool[lane.botId]}
              {@const nodeName = boardNodes.find((n) => n.nodeId === key)?.label}
              {@const name = m?.bot?.name || m?.rank || t("ui.fallbackAgent")}
              {@const avatar = m?.bot?.avatar_url ?? null}
              {@const color = `hsl(${authorHue(lane.botId || name)} 55% 68%)`}
              {#if reveal.shown || lane.rounds.length}
                <ChatMessageRow
                  isUser={false}
                  text={reveal.body}
                  streamTail={reveal.tail}
                  grouped={false}
                  showMeta={false}
                  ghost={isGhostContent(reveal.body)}
                  gutter={true}
                  gutterName={name}
                  gutterImage={avatar}
                  gutterMood={fleetActivity.mood(lane.botId || "")}
                  author={{ name, specialty: nodeName || m?.specialty }}
                  authorColor={color}
                >
                  {#snippet aboveBubble()}
                    {#if lane.rounds.length}
                      <div class="space-y-1 px-3 min-w-0">
                        {#each lane.rounds as round, ri (ri)}
                          <div class="text-[11px] text-[var(--text-muted)] leading-snug border-l-2 border-[var(--hairline)] pl-2.5 line-clamp-3">{round}</div>
                        {/each}
                      </div>
                    {/if}
                  {/snippet}
                  {#snippet belowBubble()}
                    {#if toolName}
                      <div class="px-3">
                        <span class="font-mono text-[9px] px-1.5 py-0.5 rounded bg-info/10 border border-info/30 text-info inline-flex items-center gap-1">
                          <Wrench class="size-2.5" /> {toolName}
                        </span>
                      </div>
                    {:else if st === "thinking" || st === "running_tool" || st === "waiting_on_user"}
                      <div class="px-3">
                        <span class="font-mono text-[9px] px-1.5 py-0.5 rounded bg-warning/10 border border-warning/30 text-warning">
                          {st === "waiting_on_user" ? "waiting" : "thinking"}
                        </span>
                      </div>
                    {/if}
                  {/snippet}
                </ChatMessageRow>
              {:else}
                {@render statusRow(name, avatar, color, toolName)}
              {/if}
            {/each}

            <!-- Active bots that haven't streamed a token yet (pre-first-delta) -->
            {#each activeMembers.filter((m: any) => !laneBotIds.has(m.bot?.id)) as m (m.bot?.id)}
              {@const name = m?.bot?.name || m.rank || "?"}
              {@render statusRow(
                name,
                m?.bot?.avatar_url || getDiceBearUrl(name, m?.bot?.avatar_style || "bottts"),
                `hsl(${authorHue(m.bot?.id || name)} 55% 68%)`,
                agentTool[m.bot?.id],
              )}
            {/each}

            {#if activeMembers.length === 0}
              <div class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)] pl-1">
                <Loader2 class="size-3.5 animate-spin text-[var(--brand)]" />
                <span>{planGenerating ? "Dispatching the plan…" : "CEO is planning the work…"}</span>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
      {#if !stickToLatest}
        <button
          type="button"
          class="jump-latest"
          onclick={jumpToLatest}
          title={t("room.jumpLatest")}
          aria-label={t("room.jumpLatest")}
        >
          <ArrowDown class="size-3.5" />
          <span>{t("room.jumpLatest")}</span>
        </button>
      {/if}
    </div>

    <!-- Grok Floating Capsule Compose Bar -->
    <div class="p-4 bg-[var(--surface-0)] shrink-0">
      <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]  p-3 shadow-2xl transition-all duration-200 focus-within:border-[var(--brand)]">
        {#if pendingAttachments.length}
          <div class="flex flex-wrap gap-1.5 pb-2">
            {#each pendingAttachments as att, idx}
              {#if att.isImage}
                <div class="relative size-12 rounded-lg overflow-hidden border border-[var(--hairline-strong)] bg-[var(--surface-2)]">
                  <img src={`data:${att.mime};base64,${att.data}`} alt={att.name} class="size-full object-cover" />
                  <button type="button" class="absolute top-0.5 right-0.5 size-4 rounded-full bg-black/60 text-[var(--text-primary)] text-[9px] flex items-center justify-center cursor-pointer hover:bg-danger/80" onclick={() => removePendingAttachment(idx)} title={t("room.removeAttachment")}>✕</button>
                </div>
              {:else}
                <div class="relative flex items-center gap-1.5 h-12 max-w-[200px] pl-2.5 pr-6 rounded-lg border border-[var(--hairline-strong)] bg-[var(--surface-2)]" title={att.name}>
                  <Paperclip class="size-3.5 text-[var(--brand-text)] shrink-0" />
                  <div class="min-w-0">
                    <div class="text-[10px] text-[var(--text-secondary)] truncate">{att.name}</div>
                    <div class="text-[9px] text-[var(--text-muted)] font-mono uppercase">{att.mime.split("/").pop()}</div>
                  </div>
                  <button type="button" class="absolute top-1 right-1 size-4 rounded-full bg-black/60 text-[var(--text-primary)] text-[9px] flex items-center justify-center cursor-pointer hover:bg-danger/80" onclick={() => removePendingAttachment(idx)} aria-label={t("room.removeAttachment")} title={t("room.removeAttachment")}>✕</button>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
        <textarea
          bind:value={newMessage}
          aria-label="Message"
          placeholder={members.length === 0
            ? t("room.hireFirstPh")
            : t("room.taskPh", { name: room.name, n: members.length })}
          rows={1}
          class="w-full bg-transparent text-xs sm:text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none min-h-[44px] max-h-40 leading-relaxed font-sans"
          onkeydown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        ></textarea>

        <!-- Action Toolbar Inside Capsule -->
        <div class="flex items-center justify-between pt-2 border-t border-[var(--hairline)] mt-1">
          <div class="flex items-center gap-1.5">
            <button
              type="button"
              class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center transition-colors cursor-pointer"
              onclick={attachFile}
              title={t("home.attach")}
            >
              <Paperclip class="size-3.5" />
            </button>

            <button
              type="button"
              class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {isListening ? 'text-danger bg-danger/20 border border-danger/50 animate-pulse shadow-sm' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
              onclick={toggleVoice}
              title={isListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
            >
              <Mic class="size-3.5" />
            </button>

            <span class="text-[10px] font-mono text-[var(--text-muted)] ml-1">
              {members.length} Parallel Agents Assigned
            </span>
          </div>

          <button
            type="button"
            onclick={() => send()}
            disabled={((!newMessage.trim() && pendingAttachments.length === 0) || sending || members.length === 0)}
            class="h-8 px-4 rounded-full flex items-center gap-1.5 transition-all duration-200 cursor-pointer font-semibold text-xs {(newMessage.trim() || pendingAttachments.length) && !sending && members.length > 0
              ? 'btn-brand text-[var(--text-primary)] hover:scale-[1.03] active:scale-95'
              : 'bg-[var(--surface-3)] text-[var(--text-muted)] cursor-not-allowed'}"
          >
            {#if sending}
              <Loader2 class="size-3.5 animate-spin" />
              <span>{t("room.splitting")}</span>
            {:else}
              <ArrowUp class="size-3.5 stroke-[2.5]" />
              <span>{t("room.dispatch")}</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  </div>
</div>

{#if showOfficeSettings}
  <OfficeSettings
    room={room}
    {bots}
    open={showOfficeSettings}
    onClose={() => (showOfficeSettings = false)}
    onUpdated={(updated) => {
      room = updated;
      window.dispatchEvent(new CustomEvent("office-updated", { detail: { room: updated } }));
    }}
    onDeleted={() => {
      showOfficeSettings = false;
      window.dispatchEvent(new CustomEvent("office-deleted", { detail: { roomId: room?.id } }));
    }}
  />
{/if}

<!-- Office Brain drawer: shared memory at home in the room -->
{#if showOfficeMemory}
  <div
    class="fixed inset-0 z-50 bg-black/60"
    role="button"
    tabindex="0"
    aria-label={t("room.close")}
    onclick={(e) => { if (e.target === e.currentTarget) showOfficeMemory = false; }}
    onkeydown={(e) => { if (e.key === "Escape") showOfficeMemory = false; }}
  >
    <div class="absolute top-0 right-0 h-full w-[440px] max-w-[92vw] bg-[var(--surface-0)] border-l border-[var(--hairline)] flex flex-col shadow-2xl animate-fade-in" role="dialog" aria-label={t("memory.title")}>
      <div class="flex items-center justify-between gap-3 px-4 py-3 border-b border-[var(--hairline)] bg-[var(--surface-1)] shrink-0">
        <span class="flex items-center gap-2 min-w-0 text-xs font-bold uppercase tracking-wider font-mono text-[var(--text-primary)]">
          <Brain class="size-4 text-[var(--brand-text)] shrink-0" />
          <span class="truncate">{room.name}</span>
        </span>
        <button type="button" onclick={() => (showOfficeMemory = false)} class="size-7 rounded-lg bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center shrink-0 cursor-pointer" title={t("room.close")} aria-label={t("room.close")}>
          ✕
        </button>
      </div>
      <div class="flex-1 overflow-y-auto p-4 overscroll-contain">
        <OfficeMemoryPanel chatroomId={room.id} />
      </div>
    </div>
  </div>
{/if}

<!-- Plan Mode Modal -->
{#if showPlanModal}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 " role="button" tabindex="0" aria-label={t("room.close")} onclick={(e) => { if (e.target === e.currentTarget) showPlanModal = false; }} onkeydown={(e) => { if (e.key === "Escape") showPlanModal = false; }}>
    <div class="modal-panel w-full max-w-2xl max-h-[85vh] flex flex-col">
      <div class="px-6 pt-5 pb-3 border-b border-[var(--hairline)] shrink-0">
        <h3 class="text-base font-bold text-[var(--text-primary)]">{t("room.plan")} — {room.name}</h3>
        <p class="text-xs text-[var(--text-tertiary)]">{t("room.planDesc")}</p>
      </div>

      <div class="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        <div class="space-y-1.5">
          <div class="flex items-center justify-between">
            <label for="plan-goal" class="text-xs font-bold text-[var(--text-primary)]">{t("room.goal")}</label>
            <button
              type="button"
              onclick={draftPlan}
              disabled={draftingPlan || !planGoal.trim()}
              class="h-6 px-2.5 text-[10px] rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer disabled:opacity-40 flex items-center gap-1"
              title={t("room.draftTitle")}
            >
              {#if draftingPlan}
                <Loader2 class="size-3 animate-spin" /> {t("room.drafting")}
              {:else}
                <Sparkles class="size-3" /> {t("room.draftWithLead")}
              {/if}
            </button>
          </div>
          <textarea id="plan-goal" bind:value={planGoal} rows={2} placeholder={t("room.planGoalPlaceholder")} class="w-full px-3 py-2 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus:border-[var(--brand)]/50"></textarea>
        </div>

        {#if planQuestion}
          <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-xs text-[var(--brand-text)] space-y-1">
            <div class="font-bold flex items-center gap-1.5">❓ {t("room.leadClarify")}</div>
            <p>{planQuestion}</p>
            <p class="text-[10px] text-[var(--brand-text)]/70">{t("room.refineHint")}</p>
          </div>
        {/if}

        {#if members.length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each members as m}
              <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)]">{m.rank} · {m.specialty}</span>
            {/each}
          </div>
        {/if}

        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <p class="text-xs font-bold text-[var(--text-primary)]">{t("room.tasks")} ({planTasks.length})</p>
            <span onclick={addPlanTask} class="text-[10px] text-[var(--brand-text)] hover:text-[var(--brand-text)] cursor-pointer" role="button" tabindex="0" onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); addPlanTask(); } }}>{t("room.addTask")}</span>
          </div>
          {#each planTasks as task, idx}
            <div class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] space-y-2">
              <div class="flex items-center gap-2">
                <SimpleSelect
                  value={task.botId}
                  options={members.map((m: any) => ({ value: m.bot_id, label: `${m.bot?.name || m.rank} · ${m.specialty}` }))}
                  onValueChange={(v) => updateTaskBot(idx, v)}
                  class="h-7 w-44 text-[10px] rounded-lg"
                />
                <input bind:value={task.label} oninput={(e) => updatePlanTaskLabel(idx, (e.target as HTMLInputElement).value)} class="flex-1 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-[var(--brand)]/50" placeholder={t("room.taskPlaceholder")} />
                <button type="button" onclick={() => removePlanTask(idx)} aria-label="Remove task" class="text-[var(--text-muted)] hover:text-danger cursor-pointer">✕</button>
              </div>
              {#if idx > 0}
                <div class="flex items-center gap-1 flex-wrap">
                  <span class="text-[9px] font-mono text-[var(--text-muted)] uppercase tracking-wider">{t("room.dependsOn")}</span>
                  {#each planTasks as _, oidx}
                    {#if oidx < idx}
                      {@const depOn = task.dependsOn.includes(oidx)}
                      <button
                        type="button"
                        onclick={() => togglePlanDep(idx, oidx)}
                        aria-pressed={depOn}
                        class="h-5 px-1.5 text-[9px] font-mono rounded-md border cursor-pointer {depOn
                          ? 'border-[var(--brand)]/60 bg-[var(--brand-soft)] text-[var(--brand-text)]'
                          : 'border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-muted)] hover:text-[var(--text-secondary)]'}"
                      >#{oidx + 1}</button>
                    {/if}
                  {/each}
                </div>
              {/if}
            </div>
          {:else}
            <div class="py-6 text-center text-xs text-[var(--text-muted)] border border-dashed border-[var(--hairline)] rounded-xl">{t("room.noTasks")}</div>
          {/each}
        </div>

        {#if planTasks.length > 0}
          <div class="space-y-1.5">
            <p class="text-xs font-bold text-[var(--text-primary)]">{t("room.dagTitle")}</p>
            <div class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-0)]">
              <PlanDag tasks={planTasks} {members} />
            </div>
          </div>
        {/if}

        {#if planError}
          <div class="p-3 rounded-xl bg-danger/40 border border-danger/40 text-xs text-danger">{planError}</div>
        {/if}
      </div>

      <div class="px-6 py-3 border-t border-[var(--hairline)] flex justify-end gap-2 shrink-0">
        <button type="button" onclick={() => (showPlanModal = false)} class="h-8 px-4 text-xs bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-xl cursor-pointer">{t("ui.cancel")}</button>
        <button type="button" onclick={dispatchPlan} disabled={planGenerating} class="h-8 px-4 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium rounded-xl cursor-pointer disabled:opacity-50">
          {#if planGenerating}
            <span>{t("room.dispatching")}</span>
          {:else}
            <span>{t("room.dispatchPlan")}</span>
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Hire Team Modal — CEO proposes the office org -->
{#if showHireModal}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 " role="button" tabindex="0" aria-label={t("room.close")} onclick={(e) => { if (e.target === e.currentTarget) showHireModal = false; }} onkeydown={(e) => { if (e.key === "Escape") showHireModal = false; }}>
    <div class="modal-panel w-full max-w-3xl max-h-[88vh] flex flex-col">
      <div class="px-6 pt-5 pb-3 border-b border-[var(--hairline)] shrink-0">
        <h3 class="text-base font-bold text-[var(--text-primary)] flex items-center gap-2">
          <Users class="size-4 text-success" /> {t("room.hire")} — {room.name}
        </h3>
        <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
          {t("room.hireDesc")}
        </p>
      </div>

      <div class="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        <div class="space-y-1.5">
          <div class="flex items-center justify-between">
            <label for="hire-brief" class="text-xs font-bold text-[var(--text-primary)]">{t("room.hireQuestion")}</label>
            <span class="text-[10px] text-[var(--text-muted)]">
              {#if hireSource === "ceo"}{t("room.ceoProposed")}{:else if hireSource === "template"}{t("room.templateBlueprint")}{:else}{t("room.manual")}{/if}
            </span>
          </div>
          <textarea
            id="hire-brief"
            bind:value={hireBrief}
            rows={2}
            placeholder={t("room.hirePlaceholder")}
            class="w-full px-3 py-2 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus:border-success/50"
          ></textarea>
          <button
            type="button"
            onclick={draftTeamWithCeo}
            disabled={hireDrafting}
            class="h-7 px-3 text-[11px] rounded-lg border border-success/40 bg-success/40 text-success hover:bg-success/50 cursor-pointer disabled:opacity-40 flex items-center gap-1.5"
          >
            {#if hireDrafting}
              <Loader2 class="size-3 animate-spin" /> {t("room.askingCeo")}
            {:else}
              <Sparkles class="size-3" /> {t("room.ceoProposeBtn")}
            {/if}
          </button>
          <p class="text-[10px] text-[var(--text-muted)] leading-relaxed pt-0.5">
            {t("room.reuseHint")}
          </p>
        </div>

        {#if hireQuestion}
          <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-xs text-[var(--brand-text)] space-y-1">
            <div class="font-bold">❓ {t("room.ceoNeedsClarify")}</div>
            <p>{hireQuestion}</p>
          </div>
        {/if}

        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <p class="text-xs font-bold text-[var(--text-primary)]">{t("room.roster")} ({hireRoles.length})</p>
            <span onclick={addHireRole} class="text-[10px] text-success hover:text-success cursor-pointer" role="button" tabindex="0" onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); addHireRole(); } }}>{t("room.addRole")}</span>
          </div>

          {#each hireRoles as role, idx}
            <div class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] space-y-2">
              <div class="flex items-center gap-2">
                <label class="flex items-center gap-1 text-[10px] text-[var(--text-tertiary)] shrink-0 cursor-pointer" title={t("room.makeLead")}>
                  <input type="radio" name="hire-lead" checked={role.is_lead} onchange={() => setHireLead(idx)} class="accent-[var(--brand)] cursor-pointer" />
                  {t("room.lead")}
                </label>
                <input bind:value={role.name} aria-label={t("room.name")} placeholder={t("room.name")} class="w-28 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <input bind:value={role.rank} aria-label={t("room.rank")} placeholder={t("room.rank")} class="w-24 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <input bind:value={role.specialty} aria-label={t("room.specialty")} placeholder={t("room.specialty")} class="flex-1 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <button type="button" onclick={() => removeHireRole(idx)} aria-label="Remove role" class="text-[var(--text-muted)] hover:text-danger cursor-pointer">✕</button>
              </div>
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  onclick={() => (openRolePrompt = openRolePrompt === idx ? null : idx)}
                  aria-expanded={openRolePrompt === idx}
                  class="text-[10px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
                >
                  {openRolePrompt === idx ? `▾ ${t("room.hideInstructions")}` : `▸ ${t("room.roleInstructions")}`}
                </button>
                {#if role.is_lead}
                  <span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] font-bold">{t("room.ceoBadge")}</span>
                {/if}
                {#if role.skills.length}
                  <span class="text-[9px] text-[var(--text-muted)] truncate">{t("room.skillsCount", { n: role.skills.length })}</span>
                {/if}
              </div>
              {#if openRolePrompt === idx}
                <textarea
                  bind:value={role.system_prompt}
                  aria-label={t("room.roleInstructions")}
                  rows={3}
                  placeholder={t("room.instructionsPlaceholder")}
                  class="w-full px-2 py-1.5 text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg resize-none focus:outline-none focus:border-success/50"
                ></textarea>
              {/if}
            </div>
          {:else}
            <div class="py-6 text-center text-xs text-[var(--text-muted)] border border-dashed border-[var(--hairline)] rounded-xl">
              {t("room.noRoles")}
            </div>
          {/each}
        </div>

        {#if hireError}
          <div class="p-3 rounded-xl bg-danger/40 border border-danger/40 text-xs text-danger">{hireError}</div>
        {/if}

        {#if hireResult}
          <div class="p-3 rounded-xl bg-success/40 border border-success/40 text-xs text-success flex items-center gap-2">
            <CheckCircle2 class="size-3.5 shrink-0" />
            <span>{hireResult}</span>
          </div>
        {/if}
      </div>

      <div class="px-6 py-3 border-t border-[var(--hairline)] flex justify-end gap-2 shrink-0">
        {#if hireResult}
          <button type="button" onclick={() => (showHireModal = false)} class="h-8 px-4 text-xs bg-success/15 border border-success/40 text-success hover:bg-success/25 font-bold rounded-xl cursor-pointer">{t("ui.done")}</button>
        {:else}
          <button type="button" onclick={() => (showHireModal = false)} class="h-8 px-4 text-xs bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-xl cursor-pointer">{t("ui.cancel")}</button>
          <button
            type="button"
            onclick={provisionHiredTeam}
            disabled={hireProvisioning || hireRoles.length === 0}
            class="h-8 px-4 text-xs bg-success/15 border border-success/40 text-success hover:bg-success/25 font-bold rounded-xl cursor-pointer disabled:opacity-50 flex items-center gap-1.5"
          >
            {#if hireProvisioning}
              <Loader2 class="size-3.5 animate-spin" /> {t("room.hiring")}
            {:else}
              {hireRoles.length === 1 ? t("room.hire1") : t("room.hireN", { n: hireRoles.length })}
            {/if}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}


<!-- Manage Agent chooser — owner can edit model, skills, MCP & connectors -->
{#if manageBot && !showBotSettings && !showSkillManager}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 "
    role="button"
    tabindex="0"
    aria-label={t("room.close")}
    onclick={(e) => { if (e.target === e.currentTarget) manageBot = null; }}
    onkeydown={(e) => { if (e.key === "Escape") manageBot = null; }}
  >
    <div class="modal-panel w-full max-w-md p-5 space-y-4 animate-scale-in">
      <div class="flex items-center gap-3">
        <div class="size-11 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline-strong)] shrink-0">
          <RavenAvatar name={manageBot.name} imageUrl={manageBot.avatar_url} />
        </div>
        <div class="min-w-0">
          <div class="font-bold text-sm text-[var(--text-primary)] truncate flex items-center gap-1.5">
            {manageBot.name}
            {#if manageBot.is_orchestrator}<span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] font-bold uppercase">{t("room.leadBadge")}</span>{/if}
          </div>
          <div class="text-[11px] text-[var(--text-tertiary)] truncate">
            {manageBot.rank || t("ui.fallbackAgent")} · {manageBot.specialty || t("ui.fallbackGeneralist")}
          </div>
          <div class="text-[10px] font-mono text-[var(--text-muted)] truncate mt-0.5">
            {manageBot.config?.model_provider || "?"}/{manageBot.config?.model_id || "?"}
          </div>
        </div>
      </div>

      <div class="grid grid-cols-1 gap-2">
        <button
          type="button"
          class="w-full flex items-center gap-3 p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)] hover:bg-[var(--surface-2)] transition-colors cursor-pointer text-left"
          onclick={() => (showBotSettings = true)}
        >
          <Cpu class="size-4 text-[var(--brand)] shrink-0" />
          <span class="min-w-0">
            <span class="block text-xs font-bold text-[var(--text-primary)]">{t("room.manageModel")}</span>
            <span class="block text-[10px] text-[var(--text-tertiary)]">{t("room.manageModelDesc")}</span>
          </span>
        </button>

        <button
          type="button"
          class="w-full flex items-center gap-3 p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)] hover:bg-[var(--surface-2)] transition-colors cursor-pointer text-left"
          onclick={() => (showSkillManager = true)}
        >
          <Wrench class="size-4 text-[var(--brand)] shrink-0" />
          <span class="min-w-0">
            <span class="block text-xs font-bold text-[var(--text-primary)]">{t("room.manageSkills")}</span>
            <span class="block text-[10px] text-[var(--text-tertiary)]">{t("room.manageSkillsDesc")}</span>
          </span>
        </button>

        <button
          type="button"
          class="w-full flex items-center gap-3 p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)] hover:bg-[var(--surface-2)] transition-colors cursor-pointer text-left"
          onclick={() => {
            window.dispatchEvent(new CustomEvent("open-connectors", { detail: { botId: manageBot.id } }));
            manageBot = null;
          }}
        >
          <Server class="size-4 text-[var(--brand)] shrink-0" />
          <span class="min-w-0">
            <span class="block text-xs font-bold text-[var(--text-primary)]">{t("room.manageMcp")}</span>
            <span class="block text-[10px] text-[var(--text-tertiary)]">{t("room.manageMcpDesc")}</span>
          </span>
        </button>
      </div>

      <div class="flex justify-end">
        <button type="button" onclick={() => (manageBot = null)} class="h-8 px-4 text-xs bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-xl cursor-pointer">{t("room.close")}</button>
      </div>
    </div>
  </div>
{/if}

{#if showBotSettings && manageBot}
  {#await import("$lib/components/BotSettings.svelte") then BotSettings}
    <BotSettings.default
      bot={manageBot}
      open={showBotSettings}
      onClose={() => { showBotSettings = false; manageBot = null; }}
      onUpdated={(updated: any) => { handleAgentUpdated(updated); }}
    />
  {/await}
{/if}

{#if showSkillManager && manageBot}
  {#await import("$lib/components/SkillManager.svelte") then SkillManager}
    <SkillManager.default
      bot={manageBot}
      open={showSkillManager}
      onClose={() => { showSkillManager = false; manageBot = null; }}
      onUpdated={(updated: any) => { handleAgentUpdated(updated); }}
    />
  {/await}
{/if}
