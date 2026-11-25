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
  import { getDiceBearUrl, isUserMessage, OFFICE_TEMPLATES, DEFAULT_AVATAR_STYLE } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { t } from "$lib/i18n";
  import { sessionRole } from "$lib/sessionRole.svelte";
  import { recordUtterance, transcribeBlob, voiceErrorMessage } from "$lib/voice";
  import {
    ACCEPTED_IMAGE_MIMES,
    MAX_ATTACHMENT_BYTES,
    isTextFile,
    readAsDataUrl,
    readAsText,
    type PendingAttachment,
  } from "$lib/attachments";
  import { onMount, onDestroy, tick } from "svelte";
  import { notify } from "$lib/toast";
  import { announceAttentionChanged } from "$lib/attention.svelte";
  import OfficeSettings from "$lib/components/OfficeSettings.svelte";
  import OfficeMemoryPanel from "$lib/components/OfficeMemoryPanel.svelte";
  import ChatMessageRow from "$lib/components/chat/ChatMessageRow.svelte";
  import TranscriptRail from "$lib/components/chat/TranscriptRail.svelte";
  import VirtualTranscript from "$lib/components/chat/VirtualTranscript.svelte";
  import { workflowRunOutcome } from "$lib/workflowRuns";
  import {
    WORKFLOW_TEMPLATES,
    materializeWorkflowTemplate,
    type WorkflowTemplate,
  } from "$lib/workflowTemplates";
  import {
    mergeOlderMessages,
    oldestMessageId,
    MESSAGE_PAGE_SIZE,
  } from "$lib/chat/messagePaging";
  import {
    mergeReplyTargets,
    replyTargetsToLoad,
  } from "$lib/chat/replyResolution";
  import MessageSelectionToolbar from "$lib/components/chat/MessageSelectionToolbar.svelte";
  import { quoteAsMarkdown, type SelectionQuote } from "$lib/chat/selection";
  import { isPersistedMessageId, toggleReaction as nextReactions } from "$lib/chat/reactions";
  import { matchingMessageIds, messageKey, stepMatchIndex } from "$lib/chat/search";
  import { replyQuoteFor } from "$lib/chat/replies";
  import { messageDayKey, messageDayLabel } from "$lib/chat/dayMarkers";
  import { isEditableTarget, navigateMessage } from "$lib/chat/navigation";
  import { anchoredScrollTop, transcriptWindow } from "$lib/chat/messageWindow";
  import { estimateMessageHeight } from "$lib/chat/virtualRows";
  import { prewarmMarkdownMessages } from "$lib/markdown/offThread";
  import { drafts, officeDraftKey } from "$lib/chat/drafts";
  import {
    applyMention,
    filterMentionItems,
    mentionAtCaret,
    type MentionItem,
    type MentionSigil,
    type MentionToken,
  } from "$lib/chat/mentions";
  import OfficeHITL from "$lib/components/chat/OfficeHITL.svelte";
  import OfficeComposer from "$lib/components/chat/OfficeComposer.svelte";
  import OfficeHeader from "$lib/components/chat/OfficeHeader.svelte";
  import type {
    InteractionBlock as InteractionBlockPayload,
    QuestionKind,
  } from "$lib/chat/blocks";
  import PlanDag from "$lib/components/chat/PlanDag.svelte";
  import OfficeBoard from "$lib/components/chat/OfficeBoard.svelte";
  import WorkflowLibrary from "$lib/components/chat/WorkflowLibrary.svelte";
  import WorkItemDrawer from "$lib/components/chat/WorkItemDrawer.svelte";
  import type { BoardNode, BotTodo, BoardWorkItem } from "$lib/components/chat/OfficeBoard.svelte";
  import RunTimelineStrip from "$lib/components/chat/RunTimeline.svelte";
  import { RunTimeline as RunTimelineState } from "$lib/chat/runTimeline.svelte";
  import { showAuthorHeader, authorHue, isGhostContent } from "$lib/chat/grouping";
  import { fleetActivity } from "$lib/fleetActivity.svelte";
  import { handoffs } from "$lib/handoffs.svelte";
  import { workspace } from "$lib/workspace.svelte";
  import { sidePanel } from "$lib/workspace/sidePanel.svelte";
  import { makeArtifact } from "$lib/artifact";
  import { artifactStore } from "$lib/artifacts.svelte";
  import { requestThreadFocus } from "$lib/chat/messageFocus";
  import { reasoning } from "$lib/chat/reasoning.svelte";
  import { contextUsage, type ContextUsage } from "$lib/chat/contextUsage.svelte";
  import { persistedContextUsage } from "$lib/chat/contextPersistence";
  import ImageGallery from "$lib/components/chat/ImageGallery.svelte";
  import SourceChips from "$lib/components/chat/SourceChips.svelte";
  import { decodeMessages } from "$lib/ipc/decode";
  import FileAttachmentCard from "$lib/components/chat/FileAttachmentCard.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { StreamReveal } from "$lib/chat/streamReveal.svelte";
  import { prefersReducedMotion } from "$lib/a11y";
  import {
    Building2,
    Users,
    Send,
    Sparkles,
    CheckCircle2,
    Loader2,
    Circle,
    ArrowRight,
    AlertTriangle,
    Shield,
    Workflow,
    Layers,
    Cpu,
    Wrench,
    Server,
    Terminal,
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

  // ── In-thread search ───────────────────────────────────────────────────
  // The office timeline is a conversation too, so it gets the same search
  // panel as a one-to-one thread rather than a second, different control.
  let roomSearchOpen = $state(false);
  let roomSearchQuery = $state("");
  let roomSearchIndex = $state(0);
  let roomSearchInput = $state<HTMLInputElement | null>(null);
  let roomSearchTimer: ReturnType<typeof setTimeout> | null = null;
  const roomSearchMatches = $derived(matchingMessageIds(messages, roomSearchQuery));
  const roomSearchMatchSet = $derived(new Set(roomSearchMatches));
  const roomSearchPosition = $derived(
    roomSearchMatches.length ? Math.min(roomSearchIndex, roomSearchMatches.length - 1) : -1,
  );
  const roomCurrentSearchKey = $derived(
    roomSearchPosition >= 0 ? roomSearchMatches[roomSearchPosition] : "",
  );

  /**
   * Point the window at a message, then scroll to it.
   *
   * Pinning the window makes a scroll to the bottom of the *mounted* rows
   * indistinguishable from the end of the real transcript, so `stickToLatest`
   * is cleared and the jump pill stays available. A jump to the newest message
   * releases the pin instead.
   */
  async function revealMessage(key: string) {
    const index = messages.findIndex((message) => messageKey(message) === key);
    if (index < 0) return;
    if (index >= messages.length - 1) {
      windowEnd = 0;
    } else {
      windowEnd = index + 1;
      stickToLatest = false;
    }
    await tick();
    // The virtualizer mounts the row and performs the scroll; the row may not
    // exist in the DOM when this reveal starts.
    virtualTarget = key;
  }

  async function scrollToRoomMatch(index: number) {
    const key = roomSearchMatches[index];
    if (key) await revealMessage(key);
  }

  function onRoomSearchInput(event: Event) {
    roomSearchQuery = (event.currentTarget as HTMLInputElement).value;
    roomSearchIndex = 0;
    if (roomSearchTimer) clearTimeout(roomSearchTimer);
    roomSearchTimer = setTimeout(() => {
      if (roomSearchQuery.trim()) void scrollToRoomMatch(0);
    }, 160);
  }

  function stepRoomSearch(delta: number) {
    if (!roomSearchMatches.length) return;
    const base = roomSearchPosition < 0 ? 0 : roomSearchPosition;
    roomSearchIndex = stepMatchIndex(base, roomSearchMatches.length, delta);
    void scrollToRoomMatch(roomSearchIndex);
  }

  function openRoomSearch() {
    roomSearchOpen = true;
    void tick().then(() => roomSearchInput?.focus());
  }

  function closeRoomSearch() {
    roomSearchOpen = false;
    roomSearchQuery = "";
    roomSearchIndex = 0;
    if (roomSearchTimer) clearTimeout(roomSearchTimer);
  }

  // ── Quoted replies ─────────────────────────────────────────────────────
  let replyingTo = $state<any | null>(null);
  let replyTargets = $state<any[]>([]);
  const replyLoadPending = new Set<string>();
  const messagesForReplies = $derived(mergeReplyTargets(messages, replyTargets));

  async function resolveReplyTargets(threadId: string, msgs: any[]) {
    const known = new Set(
      [...msgs, ...replyTargets].map((message) => String(message.id)),
    );
    const missing = replyTargetsToLoad(msgs, known, replyLoadPending);
    if (missing.length === 0) return;
    for (const id of missing) replyLoadPending.add(id);
    try {
      const fetched: any[] = await invoke("get_messages_by_ids", {
        threadId,
        ids: missing,
      });
      replyTargets = mergeReplyTargets(
        replyTargets,
        Array.isArray(fetched) ? fetched : [],
      );
    } catch {
      /* the quote falls back to the unavailable label */
    } finally {
      for (const id of missing) replyLoadPending.delete(id);
    }
  }

  const activeReply = $derived(
    replyQuoteFor(
      messagesForReplies,
      replyingTo?.id,
      roomReplyAuthor,
      t("thread.replyUnavailable"),
    ),
  );

  function roomReplyAuthor(message: any): string {
    if (isUserMessage(message)) return "You";
    return (
      message.sender_name ||
      (message.sender_bot_id ? agentNameFor(message.sender_bot_id) : room?.name || "")
    );
  }

  function startReply(message: any) {
    if (!isPersistedMessageId(message?.id)) return;
    replyingTo = message;
    void tick().then(() => composerRef?.focus());
  }

  function cancelReply() {
    replyingTo = null;
  }

  function messageById(id: string) {
    return messages.find((message) => messageKey(message) === id) ?? null;
  }

  function replyFromSelection(messageId: string) {
    const message = messageById(messageId);
    if (message) startReply(message);
  }

  function quoteFromSelection(selection: SelectionQuote) {
    const message = messageById(selection.messageId);
    if (!message) return;
    startReply(message);
    newMessage = `${quoteAsMarkdown(selection.text)}\n\n${newMessage}`.trimStart();
    void tick().then(() => composerRef?.focus());
  }

  async function jumpToMessage(id: string) {
    await revealMessage(id);
  }

  // Reactions are stored with the message, so a reload or a rerun keeps them.
  // The row owns the chip UI; the room owns the write and the rollback.
  async function reactToMessage(message: any, emoji: string) {
    const id = String(message?.id || "");
    if (!isPersistedMessageId(id)) return;
    const current: string[] = Array.isArray(message.reactions) ? message.reactions : [];
    const optimistic = nextReactions(current, emoji);
    messages = messages.map((m) =>
      String(m.id) === id ? { ...m, reactions: optimistic } : m,
    );
    try {
      const confirmed = await invoke<string[]>("toggle_message_reaction", {
        messageId: id,
        emoji,
      });
      messages = messages.map((m) =>
        String(m.id) === id ? { ...m, reactions: confirmed } : m,
      );
    } catch (e) {
      messages = messages.map((m) =>
        String(m.id) === id ? { ...m, reactions: current } : m,
      );
      notify(`Could not save reaction: ${String(e)}`, "error");
    }
  }
  let newMessage = $state("");
  let loadError = $state<string | null>(null);
  let loadingRoom = $state(false);
  let draftRoomId: string | null = null;
  let mentionOpen = $state(false);
  let mentionSigil = $state<MentionSigil>("#");
  let mentionToken = $state<MentionToken | null>(null);
  let mentionItems = $state<MentionItem[]>([]);
  let mentionIndex = $state(0);
  let addressedTo = $state<{ id: string; label: string } | null>(null);
  let fileMentions = $state<MentionItem[]>([]);
  let skillMentions = $state<MentionItem[]>([]);
  let threadId: string | null = $state(null);
  /**
   * Handoffs in this conversation, newest first.
   *
   * Derived from the store rather than copied into local state, so a handoff
   * that settles while the view is open cannot leave a stale row behind.
   */
  const threadHandoffs = $derived(threadId ? handoffs.forThread(threadId) : []);

  let sending = $state(false);
  let pendingMessageId = $state<string | null>(null);
  /** Continuous office work; the header badge is the switch. */
  let autonomyOn = $state(true);
  let autonomyBusy = $state(false);
  /** Backend-owned run state, so returning mid-planning still shows progress. */
  let officeActive = $state(false);
  let officePhase = $state<string | null>(null);
  interface OfficeQueueItemView {
    id: string;
    content: string;
    visible: string;
    attachment_names: string[];
    state: "pending" | "running" | "failed";
    error?: string | null;
    created_at: string;
    started_at?: string | null;
  }
  let composerRef = $state<HTMLTextAreaElement | null>(null);
  let queueItems = $state<OfficeQueueItemView[]>([]);
  let queuePaused = $state(false);
  let queueBusy = $state(false);
  /** The newest node context; office nodes run on threads other than the room. */
  let officeContext = $state<ContextUsage | null>(null);

  $effect(() => {
    if (!threadId) return;
    const current = messages.filter((message) => message.thread_id === threadId);
    const persisted = persistedContextUsage(current);
    if (!persisted) return;
    contextUsage.hydrate(threadId, current);
    officeContext = persisted;
  });
  /** Resolves `send()` when the detached office turn reports `done`. */
  let dispatchDone: (() => void) | null = null;
  let dispatchTimeout: ReturnType<typeof setTimeout> | null = null;
  let dispatchPoll: ReturnType<typeof setInterval> | null = null;
  let dispatchFinished = false;
  let chatContainer = $state<HTMLDivElement | null>(null);
  let showOfficeSettings = $state(false);
  // Office Brain drawer — memory is a first-class room surface, not a settings tab only
  let showOfficeMemory = $state(false);
  // Plan mode
  let showPlanModal = $state(false);
  let planGoal = $state("");
  type PlanCondition = {
    source: number;
    operator: string;
    value: string;
  };
  type PlanLoopStop = {
    operator: string;
    value: string;
  };
  let planTasks = $state<
    Array<{
      id: string;
      botId: string;
      label: string;
      dependsOn: number[];
      condition: PlanCondition | null;
      dataFrom: number[] | null;
      retries: number;
      outputSchema: string;
      loopIterations: number;
      loopUntil: PlanLoopStop | null;
      loopGroup: string;
      loopGroupTerminal: boolean;
      loopGroupIterations: number;
      loopGroupStop: PlanLoopStop | null;
      nodeType: string;
    }>
  >([]);
  let loadedWorkflow = $state<{ id: string; version: number; name: string } | null>(null);
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
      const roles = hireRoles.map((r) => ({
        name: r.name,
        rank: r.rank,
        specialty: r.specialty,
        system_prompt: r.system_prompt || null,
        is_lead: r.is_lead,
        skills: r.skills,
      }));
      let removeMissing = false;
      try {
        const preview: any = await invoke("preview_office_provisioning", {
          chatroomId: room.id,
          roles,
        });
        const stale = Array.isArray(preview?.remove) ? preview.remove : [];
        if (stale.length > 0) {
          const names = stale.map((member: any) => String(member.name || member.bot_id)).join(", ");
          removeMissing = confirm(t("room.removeMissingConfirm", { names }));
        }
      } catch {
        // A preview failure must not block staffing. It only means the
        // destructive removal option is unavailable for this run.
      }
      const res: any = await invoke("provision_office_org", {
        chatroomId: room.id,
        roles,
        provider: null,
        model: null,
        removeMissing,
      });
      const createdCount = Array.isArray(res?.created) ? res.created.length : 0;
      const reusedCount = Array.isArray(res?.reused) ? res.reused.length : 0;
      const removedCount = Array.isArray(res?.removed) ? res.removed.length : 0;
      hireResult = `Hired ${createdCount} new agent${createdCount === 1 ? "" : "s"} · reused ${reusedCount} existing from the fleet${removedCount > 0 ? ` · removed ${removedCount}` : ""}.`;
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
  let laneSources = $state<Record<string, any[]>>({});
  let laneImages = $state<Record<string, { name: string; data_url: string }[]>>({});
  // Current tool each agent is running (live)
  let agentTool = $state<Record<string, string>>({});
  // Office board (planner → kanban → live DAG) fed by plan_ready/node_* events
  let boardGoal = $state("");
  let boardNodes = $state<BoardNode[]>([]);
  // Durable work items from earlier cycles; the live board only knows this run.
  let workItems = $state<BoardWorkItem[]>([]);
  // The durable task record opened from a board card.
  let selectedBoardNode = $state<BoardNode | null>(null);
  // A task record belongs to one office; switching rooms must not leave the
  // previous office's drawer over the new conversation.
  $effect(() => {
    room?.id;
    selectedBoardNode = null;
  });
  // Threads owned by the current room run: every node thread opened so far.
  // Events from any other thread are ignored (no cross-run clobbering).
  const activeThreads = new Set<string>();
  const threadToNode = new Map<string, string>();
  /**
   * Fallback thread attribution for lanes keyed by bot (`bot:<id>`), which is
   * what a non-graph office turn produces. Reasoning is addressed to a thread,
   * so without this the lead's thinking existed but had no lane to appear in.
   */
  const botToThread = new Map<string, string>();
  /**
   * Reverse of `threadToNode`, for reading state that is keyed by thread —
   * reasoning, specifically. Lanes are keyed by node so parallel runs of the same
   * bot stay separate, but reasoning arrives addressed to its thread, and mapping
   * one way would mean keeping a second map that drifts.
   */
  const laneThreadId = (key: string): string | null => {
    if (key.startsWith("bot:")) return botToThread.get(key.slice(4)) || null;
    // Lanes are keyed by the raw node id (`threadToNode.get(tid)`), with
    // `node:` accepted too for older persisted boards. The previous version
    // required the prefix, so every live office lane looked up `""` and its
    // reasoning panel stayed empty — the thinking existed, but had no thread
    // to read it from.
    const nodeId = key.startsWith("node:") ? key.slice(5) : key;
    for (const [threadId, id] of threadToNode) if (id === nodeId) return threadId;
    return null;
  };
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
    kind?: QuestionKind | null; payload?: InteractionBlockPayload | null;
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

  async function decideApproval(id: string, allowed: boolean) {
    if (decidingApproval) return;
    decidingApproval = id;
    try {
      await invoke("decide_approval", { approvalId: id, allowed, note: null });
      pendingApprovals = pendingApprovals.filter((a) => a.id !== id);
      announceAttentionChanged();
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
      announceAttentionChanged();
    } catch (e) {
      notify(`Failed to answer: ${String(e)}`, "error");
    } finally {
      answeringQuestion = null;
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

  function openBoardNodeThread(node: BoardNode) {
    if (!node.nodeThreadId) return;
    workspace.selectBot(node.botId);
    requestThreadFocus({ botId: node.botId, threadId: node.nodeThreadId });
  }

  function openHandoffThread(handoff: { toBotId: string; childThreadId: string | null }) {
    if (!handoff.childThreadId) return;
    workspace.selectBot(handoff.toBotId);
    requestThreadFocus({ botId: handoff.toBotId, threadId: handoff.childThreadId });
  }

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
    laneSources = {};
    laneImages = {};
    agentStatus = {};
    agentTool = {};
    boardGoal = "";
    boardNodes = [];
    botTodos = {};
    pendingApprovals = [];
    pendingQuestions = [];
    activeThreads.clear();
    threadToNode.clear();
    botToThread.clear();
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
      laneSources = {};
      laneImages = {};
      agentTool = {};
      agentStatus = {};
      activeThreads.clear();
      threadToNode.clear();
      botToThread.clear();
    }, 500);
  }
  function clipPreview(value: string): string {
    const text = value.replace(/\s+/g, " ").trim();
    return text.length > 320 ? `${text.slice(0, 319)}…` : text;
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
      preview: clipPreview(String(it.result || "")),
    }));
  }
  // Rehydrate the persisted board when (re)opening a room. A stored
  // "running" node only stays Working if the server says its node run is
  // still non-terminal (startup reconciliation guarantees that means LIVE);
  // otherwise it is demoted to pending. Parked HITL cards are re-queried
  // for the room thread and every node thread we know about.
  async function rehydratePersistedBoard(force = false) {
    if (!threadId || (!force && boardNodes.length > 0)) return;
    let saved: { goal?: string; nodes?: any[]; liveNodeThreads?: unknown } | null = null;
    try {
      saved = await invoke<{ goal?: string; nodes?: any[]; liveNodeThreads?: unknown } | null>(
        "get_office_board",
        { threadId },
      );
    } catch {
      return;
    }
    if (!saved || (!force && boardNodes.length > 0)) return;
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
          preview: n?.preview ? clipPreview(String(n.preview)) : undefined,
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
      // Threads seen on the wire but not yet on the board. The board is
      // persisted and reloaded; this map is built from live events, so it knows
      // about a node the moment it opens — which is exactly when a card can
      // already be parked there.
      ...[...threadToNode.keys()].filter(Boolean),
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
  const officePhaseLabel = $derived(
    officePhase === "planning"
      ? t("room.phasePlanning")
      : officePhase === "summarizing"
        ? t("room.phaseSummarizing")
        : t("room.phaseWorking"),
  );

  let templateInfo = $derived(
    OFFICE_TEMPLATES[room?.office_template as keyof typeof OFFICE_TEMPLATES] || OFFICE_TEMPLATES.custom
  );

  const hasRunningBoardNodes = $derived(boardNodes.some((node) => node.state === "running"));

  /**
   * Watchdog for a UI that kept a live lane after the run behind it ended.
   *
   * The `done` event is the primary signal, but a restart, a dropped stream, or
   * a run killed between events can leave "Working…" on screen with no process
   * behind it. The backend's active marker is the authority; when it says idle
   * and no dispatch is waiting in this view, clear the lanes and re-read the
   * persisted board so a stale card is demoted instead of pulsing forever.
   */
  $effect(() => {
    if ((!teamActive && !hasRunningBoardNodes) || sending) return;
    const timer = setInterval(async () => {
      if (sending) return;
      try {
        const active = await invoke<boolean>("get_office_active", { chatroomId: room.id });
        if (active) return;
        lanes = {};
        laneSources = {};
        laneImages = {};
        agentTool = {};
        agentStatus = {};
        activeThreads.clear();
        threadToNode.clear();
        botToThread.clear();
        void rehydratePersistedBoard(true);
      } catch {
        /* older backend or transient IPC failure: keep the event path */
      }
    }, 15000);
    return () => clearInterval(timer);
  });

  // OpenBot scroll behavior: follow the feed only while stuck to the bottom
  // (within 80px); scrolling up during a run unsticks and reveals the jump
  // pill. `staticEntries` flags bulk history loads so row entrances animate
  // only live appends (see $lib/chat/entrance).
  let stickToLatest = $state(true);
  let staticEntries = $state(true);
  let unseenMessages = $state(0);
  let seenMessageCount = 0;

  // The transcript is rendered in a bounded window. An office with a long
  // history used to mount every message at once: 216 messages was ~20k DOM
  // nodes and 1.7s of render work before the first frame. Keeping the tail
  // mounted makes opening a room proportional to what is on screen, not to
  // how long the office has existed.
  // Offices are denser than one-to-one threads: 30 recent rows are enough to
  // see the current plan, the last answers, and the live edge. Anything older
  // is one click away, and the full list stays in memory for search and quotes.
  const MESSAGE_WINDOW = 30;
  const MESSAGE_WINDOW_STEP = 30;
  let messageWindow = $state(MESSAGE_WINDOW);
  let hasEarlier = $state(false);
  let loadingEarlier = $state(false);
  // 0 means "follow the tail"; a number pins the window while the reader is
  // in older history so incoming messages cannot shift the rows under them.
  let windowEnd = $state(0);
  const windowBounds = $derived(
    transcriptWindow(messages.length, messageWindow, windowEnd),
  );
  const visibleStart = $derived(windowBounds.start);
  const visibleEnd = $derived(windowBounds.end);
  const visibleMessages = $derived(messages.slice(visibleStart, visibleEnd));

  // Viewport mounting for the office feed. The loaded-window paging above is
  // unchanged; this only decides which loaded rows have a DOM node.
  let virtualTarget = $state("");
  let virtualPin = $state(0);
  let railPositions = $state<Record<string, number>>({});
  function handleVirtualLayout(layout: { tops: Record<string, number> }) {
    // Rail entries are keyed by DOM id (`message-<key>`); the virtualizer
    // reports message keys, because that is what its row identity uses.
    const positioned: Record<string, number> = {};
    for (const [key, top] of Object.entries(layout.tops)) {
      positioned[`message-${key}`] = top;
    }
    railPositions = positioned;
  }

  let railEntries = $derived(
    visibleMessages
      .filter((msg) => {
        if (!isUserMessage(msg)) return false;
        const text =
          typeof msg.content === "string"
            ? msg.content
            : msg.content?.text || "";
        return !text.startsWith("[Autonomous cycle]");
      })
      .map((msg) => {
        const text =
          typeof msg.content === "string"
            ? msg.content
            : msg.content?.text || "";
        return {
          id: `message-${messageKey(msg)}`,
          kind: "user" as const,
          label: text.trim().slice(0, 80) || t("thread.you"),
        };
      }),
  );

  function jumpToRail(id: string) {
    void revealMessage(id.replace(/^message-/, ""));
  }

  const hiddenBefore = $derived(visibleStart);

  // Prepare long bodies in the loaded window while the cheap rows paint. The
  // worker dedupes and caps its own batch; this is a warming hint, not work the
  // renderer waits for.
  $effect(() => {
    prewarmMarkdownMessages(visibleMessages);
  });

  async function fetchMessageTail(targetThread: string): Promise<any[]> {
    const page: any = await invoke("list_messages_page", {
      threadId: targetThread,
      beforeId: null,
      limit: MESSAGE_PAGE_SIZE,
    });
    hasEarlier = !!page?.has_more;
    const messages = decodeMessages(page?.messages);
    void resolveReplyTargets(targetThread, messages);
    return messages;
  }

  async function loadEarlier() {
    if (loadingEarlier || (hiddenBefore <= 0 && !hasEarlier)) return;
    const el = chatContainer;
    const previousTop = el?.scrollTop ?? 0;
    const previousHeight = el?.scrollHeight ?? 0;
    loadingEarlier = true;
    try {
      if (hiddenBefore > 0) {
        messageWindow += MESSAGE_WINDOW_STEP;
      } else if (threadId) {
        const page: any = await invoke("list_messages_page", {
          threadId,
          beforeId: oldestMessageId(messages),
          limit: MESSAGE_PAGE_SIZE,
        });
        const older = decodeMessages(page?.messages);
        hasEarlier = !!page?.has_more;
        if (older.length === 0) return;
        messages = mergeOlderMessages(messages, older);
        messageWindow += older.length;
        void resolveReplyTargets(threadId, messages);
      }
      await tick();
      if (el) {
        el.scrollTop = anchoredScrollTop(
          previousTop,
          previousHeight,
          el.scrollHeight,
        );
      }
    } finally {
      loadingEarlier = false;
    }
  }

  function handleFeedScroll() {
    const el = chatContainer;
    if (!el) return;
    const nearTop = el.scrollTop <= 120;
    if (windowEnd !== 0) {
      // The bottom of a pinned window is not the bottom of the transcript.
      stickToLatest = false;
      if (nearTop && (hiddenBefore > 0 || hasEarlier)) void loadEarlier();
      return;
    }
    stickToLatest = el.scrollHeight - el.scrollTop - el.clientHeight <= 80;
    if (stickToLatest) unseenMessages = 0;
    // History loads as the reader reaches the old edge; the button remains the
    // explicit path for keyboard and pointer users.
    if (nearTop && (hiddenBefore > 0 || hasEarlier)) void loadEarlier();
  }

  function jumpToLatest() {
    stickToLatest = true;
    unseenMessages = 0;
    windowEnd = 0;
    messageWindow = MESSAGE_WINDOW;
    // The virtualizer mounts the tail and performs the scroll after the
    // paging window has actually changed.
    virtualPin += 1;
  }

  /**
   * Alt+Arrow reached the edge of the mounted window. Mount the adjacent
   * message and reveal it instead of stopping at the last DOM row.
   */
  function revealAdjacentFromBoundary(
    direction: 1 | -1,
    row: HTMLElement | null,
  ) {
    const id = row?.id ?? "";
    if (!id.startsWith("message-")) return;
    const key = id.slice("message-".length);
    const index = messages.findIndex((message) => messageKey(message) === key);
    const next = index + direction;
    if (index < 0 || next < 0 || next >= messages.length) return;
    void revealMessage(messageKey(messages[next]));
  }

  function handleMessageNavKey(event: KeyboardEvent) {
    if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    if (isEditableTarget(event.target)) return;
    event.preventDefault();
    navigateMessage(
      chatContainer,
      event.key === "ArrowUp" ? -1 : 1,
      !prefersReducedMotion(),
      revealAdjacentFromBoundary,
    );
  }

  $effect(() => {
    const count = messages.length;
    if (count <= seenMessageCount) {
      seenMessageCount = count;
      return;
    }
    const added = count - seenMessageCount;
    seenMessageCount = count;
    if (!stickToLatest) unseenMessages += added;
  });

  // The lane reveals advance on a timer, after the raw delta has already
  // asked to scroll. Follow the visible lane text so the office feed truly
  // reaches the bottom instead of stopping where the last token arrived.
  $effect(() => {
    for (const key of Object.keys(lanes)) {
      const shown = laneRevealOf(key).shown;
      if (shown) {
        // Read `shown` so the effect re-runs on every reveal tick.
      }
    }
    if (stickToLatest) void scrollToBottom();
  });

  async function scrollToBottom(force = false) {
    if (!force && !stickToLatest) return;
    stickToLatest = true;
    const jump = windowEnd !== 0 || messageWindow !== MESSAGE_WINDOW;
    windowEnd = 0;
    // Returning to the live end also releases any earlier batches the reader
    // loaded while paging through history.
    messageWindow = MESSAGE_WINDOW;
    if (force || jump) virtualPin += 1;
    await tick();
    if (chatContainer) {
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }
  }

  async function load() {
    replyingTo = null;
    stickToLatest = true;
    unseenMessages = 0;
    seenMessageCount = 0;
    messageWindow = MESSAGE_WINDOW;
    windowEnd = 0;
    virtualTarget = "";
    railPositions = {};
    hasEarlier = false;
    replyTargets = [];
    staticEntries = true;
    loadingRoom = true;
    loadError = null;
    try {
      const mems = (await invoke("list_chatroom_members", { chatroomId: room.id })) as any[];
      members = mems.map((m) => ({ ...m, bot: bots.find((b: any) => b.id === m.bot_id) }));
      autonomyOn = await invoke<boolean>("get_office_autonomy", { chatroomId: room.id }).catch(
        () => true,
      );
      for (const m of members) refreshBotTodos(m.bot?.id ?? "");
      void refreshQueue();
      const tid = await invoke("get_chatroom_thread", { chatroomId: room.id });
      if (tid) {
        threadId = tid as string;
        messages = await fetchMessageTail(threadId);
        await rehydratePersistedBoard();
      } else {
        threadId = null;
        messages = [];
        hasEarlier = false;
      }
      await tick();
      scrollToBottom(true);
    } catch (e) {
      loadError = String(e);
      // A failed room load must not leave the previous office's transcript,
      // members or thread id behind: sending would target the wrong room.
      messages = [];
      members = [];
      threadId = null;
      console.error(e);
    } finally {
      loadingRoom = false;
      staticEntries = false;
    }
  }

  async function stopOfficeTurn() {
    if (!room?.id) return;
    try {
      const cancelled = await invoke<number>("cancel_office_turn", { chatroomId: room.id });
      notify(
        cancelled > 0
          ? `Stopped ${cancelled} active office run${cancelled === 1 ? "" : "s"}.`
          : "The office has no active run to stop.",
        "success",
      );
      await refreshOfficeRuntime();
      await refreshQueue();
    } catch (e) {
      notify(`Could not stop the office: ${String(e)}`, "error");
    }
  }

  async function refreshWorkItems() {
    if (!room?.id) return;
    try {
      const items = await invoke<BoardWorkItem[]>("list_work_items", {
        officeId: room.id,
        limit: 200,
      });
      workItems = Array.isArray(items) ? items : [];
    } catch {
      /* transient IPC failure: keep the last known backlog */
    }
  }

  async function refreshQueue() {
    if (!room?.id) return;
    try {
      const [items, paused] = await Promise.all([
        invoke<OfficeQueueItemView[]>("get_office_queue", { chatroomId: room.id }),
        invoke<boolean>("get_office_queue_paused", { chatroomId: room.id }).catch(() => false),
      ]);
      queueItems = items;
      queuePaused = paused;
    } catch {
      /* transient IPC failure: keep the last known queue */
    }
  }

  async function toggleQueuePause() {
    if (!room?.id || queueBusy) return;
    queueBusy = true;
    const next = !queuePaused;
    try {
      await invoke("set_office_queue_paused", { chatroomId: room.id, paused: next });
      queuePaused = next;
      if (!next) void refreshOfficeRuntime();
      await refreshQueue();
      announceAttentionChanged();
    } catch (e) {
      notify(`Could not change the office queue: ${String(e)}`, "error");
    } finally {
      queueBusy = false;
    }
  }

  function pendingQueueItems(): OfficeQueueItemView[] {
    return queueItems.filter((item) => item.state === "pending");
  }

  function canMoveQueueItem(item: OfficeQueueItemView, delta: -1 | 1): boolean {
    if (item.state !== "pending" || queueBusy) return false;
    const pending = pendingQueueItems();
    const index = pending.findIndex((candidate) => candidate.id === item.id);
    return index >= 0 && index + delta >= 0 && index + delta < pending.length;
  }

  async function runQueueItemNext(item: OfficeQueueItemView) {
    if (!room?.id || queueBusy || item.state !== "pending") return;
    queueBusy = true;
    try {
      const started = await invoke<boolean>("run_office_queue_item_next", {
        chatroomId: room.id,
        id: item.id,
      });
      await refreshQueue();
      announceAttentionChanged();
      if (started) await refreshOfficeRuntime();
    } catch (e) {
      notify(`Could not start the queued message: ${String(e)}`, "error");
    } finally {
      queueBusy = false;
    }
  }

  async function moveQueueItem(item: OfficeQueueItemView, delta: -1 | 1) {
    if (!room?.id || !canMoveQueueItem(item, delta)) return;
    const pending = pendingQueueItems();
    const index = pending.findIndex((candidate) => candidate.id === item.id);
    const target = pending[index + delta];
    if (!target) return;

    // Reorder the visible list, not just the pending subset: the backend
    // persists one position per row, and leaving the running/failed rows out
    // would let their old positions overtake the chosen order on reload.
    const ordered = [...queueItems];
    const from = ordered.findIndex((candidate) => candidate.id === item.id);
    const to = ordered.findIndex((candidate) => candidate.id === target.id);
    if (from < 0 || to < 0) return;
    [ordered[from], ordered[to]] = [ordered[to], ordered[from]];

    queueBusy = true;
    try {
      await invoke("reorder_office_queue", {
        chatroomId: room.id,
        orderedIds: ordered.map((candidate) => candidate.id),
      });
      await refreshQueue();
      announceAttentionChanged();
    } catch (e) {
      notify(`Could not reorder the office queue: ${String(e)}`, "error");
    } finally {
      queueBusy = false;
    }
  }

  async function removeQueueItem(item: OfficeQueueItemView) {
    if (!room?.id || queueBusy) return;
    queueBusy = true;
    try {
      await invoke(
        item.state === "failed" ? "dismiss_office_queue_item" : "cancel_office_queue_item",
        { chatroomId: room.id, id: item.id },
      );
      await refreshQueue();
    } catch (e) {
      notify(`Could not update the office queue: ${String(e)}`, "error");
    } finally {
      queueBusy = false;
    }
  }

  /**
   * Read the backend run state. The office can be planning before a single
   * board event exists; without this, returning to the room during planning
   * looks exactly like a dead office.
   */
  async function refreshOfficeRuntime() {
    if (!room?.id) return;
    try {
      const [active, phase, autonomy] = await Promise.all([
        invoke<boolean>("get_office_active", { chatroomId: room.id }).catch(() => false),
        invoke<string | null>("get_office_phase", { chatroomId: room.id }).catch(() => null),
        invoke<boolean>("get_office_autonomy", { chatroomId: room.id }).catch(() => autonomyOn),
      ]);
      const wasActive = officeActive;
      officeActive = active;
      officePhase = active ? phase : null;
      // The backend pauses autonomy itself when a budget ceiling or repeated
      // no-progress cycles are hit. Follow that state so the header badge
      // cannot keep claiming the office is autonomous after it stopped.
      if (!autonomyBusy) autonomyOn = autonomy;
      void refreshQueue();
      void refreshWorkItems();
      if (wasActive && !active) {
        await load();
        await rehydratePersistedBoard(true);
      }
    } catch {
      /* transient IPC failure: keep the last known state */
    }
  }

  $effect(() => {
    if (!room?.id) return;
    void refreshOfficeRuntime();
    const timer = setInterval(() => void refreshOfficeRuntime(), 10000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (room?.id) {
      threadId = null;
      messages = [];
      resetRun();
      signalDispatchDone();
      dispatchFinished = false;
      autonomyOn = true;
      officeActive = false;
      officePhase = null;
      officeContext = null;
      queueItems = [];
      queuePaused = false;
      queueBusy = false;
      workItems = [];
      officeTokens = 0;
      officeCost = 0;
      // Drafts and attachments are scoped to the room being left; the room
      // being opened restores its own draft.
      if (draftRoomId && draftRoomId !== room.id) {
        drafts.saveNow(officeDraftKey(draftRoomId), newMessage);
      }
      draftRoomId = room.id;
      newMessage = drafts.load(officeDraftKey(room.id));
      pendingAttachments = [];
      load();
    }
  });

  function handleTryLocalSkill(event: Event) {
    const prompt = (event as CustomEvent<{ prompt?: string }>).detail?.prompt;
    if (prompt) {
      newMessage = prompt;
      if (room?.id) drafts.save(officeDraftKey(room.id), prompt);
    }
  }

  onMount(() => {
    void loadMentionSources();
    window.addEventListener("local-skill-try", handleTryLocalSkill);
    listen<any>("agent-stream", (event) => {
        const p = event.payload;
            if (!p) return;
        const kind = String(p.kind || "");

        // Board topology events also own thread attribution, so they are
        // handled before the thread gate below.
        if (kind === "plan_ready") {
          void refreshQueue();
          if (p.thread_id) activeThreads.add(String(p.thread_id));
          boardGoal = String(p.goal || "");
          boardNodes = (Array.isArray(p.nodes) ? p.nodes : []).map((n: any) => ({
            nodeId: String(n.node_id ?? ""),
            botId: String(n.bot_id ?? ""),
            label: String(n.label || "Task"),
            dependsOn: (Array.isArray(n.depends_on) ? n.depends_on : []).map(String),
            state: "pending" as const,
          }));
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
          return;
        }
        if (kind === "node_iteration") {
          const nodeId = String(p.node_id ?? "");
          patchNode(nodeId, {
            state: "running",
            preview: clipPreview(String(p.preview || "")),
          });
          return;
        }
        if (kind === "node_finished") {
          const nodeId = String(p.node_id ?? "");
          patchNode(nodeId, {
            state: p.state === "failed" ? "failed" : "done",
            preview: clipPreview(String(p.preview || "")),
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
          return;
        }
        if (kind === "graph_status") {
          for (const n of Array.isArray(p.nodes) ? p.nodes : []) {
            if (n?.state) patchNode(String(n.node_id ?? ""), { state: String(n.state) as BoardNode["state"] });
          }
          return;
        }

        // Everything else must belong to THIS room's current run — the room
        // thread or a node thread opened by it. (Fixes cross-run/cross-room
        // lanes being clobbered by foreign streams.)
        //
        // Interactions are the exception, and the exception is the point: a node
        // can park an approval on its thread *before* `node_ready` has told this
        // view that thread exists, and a gate that runs first drops it. The run
        // then waits for a decision no card ever offers — silent, permanent,
        // exactly what "stops working mid-run" looks like from the outside.
        // The card carries its own identity, so it does not need the lane
        // registry to be correct first.
        const tid = p.thread_id ? String(p.thread_id) : "";
        const foreign = !!tid && tid !== String(threadId || "") && !activeThreads.has(tid);
        const isInteraction =
          kind === "approval_requested" ||
          kind === "approval_decided" ||
          kind === "question_asked" ||
          kind === "question_answered";
        if (foreign && !isInteraction) return;
        // A foreign interaction must not enter the timeline either: the strip
        // narrates this room's run, and an unattributed card from another room
        // would claim a step that never happened here.
        if (!foreign) runTimeline.track(p);

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
        } else if (kind === "reasoning") {
          // Its own event, and the `clear` below only closes the round — it does
          // not wipe this. In an office this is the one that matters most: the
          // lead's reasoning between delegations is where you can see whether it
          // is on track, and it used to be cleared away the moment any tool ran.
          if (tid && p.bot_id) botToThread.set(String(p.bot_id), tid);
          reasoning.ingest(p);
        } else if (kind === "sources") {
          const botId = String(p.bot_id || "");
          if (!botId) return;
          if (tid) botToThread.set(botId, tid);
          const key = threadToNode.get(tid) || `bot:${botId}`;
          const current = laneSources[key] || [];
          const next = [...current];
          for (const source of p.sources || []) {
            if (source?.url && !next.some((existing) => existing.url === source.url)) {
              next.push(source);
            }
          }
          if (next.length !== current.length) {
            laneSources = { ...laneSources, [key]: next };
            scrollToBottom();
          }
        } else if (kind === "image") {
          const botId = String(p.bot_id || "");
          if (!botId || !p.data_url) return;
          if (tid) botToThread.set(botId, tid);
          const key = threadToNode.get(tid) || `bot:${botId}`;
          laneImages = {
            ...laneImages,
            [key]: [
              ...(laneImages[key] || []),
              { name: String(p.name || "image"), data_url: String(p.data_url) },
            ],
          };
          scrollToBottom();
        } else if (kind === "delta") {
          // Route the token stream to its NODE lane (parallel same-bot nodes
          // stay separate); unattributed deltas fall back to a per-bot lane.
          const botId = String(p.bot_id || "");
          if (!botId || !memberForBot(botId)) return;
          if (tid) botToThread.set(botId, tid);
          const key = threadToNode.get(tid) || `bot:${botId}`;
          const cur = lanes[key] || { botId, text: "", rounds: [] };
          lanes = { ...lanes, [key]: { ...cur, text: cur.text + (p.content || "") } };
          scrollToBottom();
        } else if (kind === "context") {
          contextUsage.ingest(p);
          officeContext = {
            botId: String(p.bot_id || ""),
            usedTokens: Number(p.used_tokens) || 0,
            windowTokens: Number(p.window_tokens) || 0,
            compacted: p.compacted === true,
            at: Date.now(),
          };
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
          // Close the reasoning stretch that just ended. `endRound`, not a wipe:
          // reasoning used to ride the text channel, so this boundary is exactly
          // where every tool-using turn's thinking was destroyed.
          reasoning.endRound(tid);
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
          reasoning.finish(tid);
          void refreshQueue();
          if (tid === String(threadId || "")) {
            signalDispatchDone();
            // Returning mid-run can leave this view with the board it loaded
            // at mount; the disk snapshot is authoritative once `done` lands.
            if (!sending) {
              void load().finally(() => {
                commitLanes();
                void rehydratePersistedBoard(true);
              });
            }
          }
          // Snap the lane's word reveal to the full buffer. Without this a
          // fast answer that arrived in a burst keeps typing itself out for
          // another minute, and the office looks slow when it is finished.
          const doneKey = threadToNode.get(tid) || `bot:${p.bot_id}`;
          laneReveals.get(doneKey)?.finish();
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
    window.removeEventListener("local-skill-try", handleTryLocalSkill);
    signalDispatchDone();
    if (roomSearchTimer) clearTimeout(roomSearchTimer);
    if (commitTimer) window.clearTimeout(commitTimer);
    for (const r of laneReveals.values()) r.stop();
  });

  function openPlanModal() {
    planGoal = "";
    planTasks = [];
    loadedWorkflow = null;
    planError = null;
    planQuestion = null;
    planTasks = members.map((m, i) => ({
      id: String(i),
      botId: m.bot_id,
      label: "Address your part of the goal",
      dependsOn: [] as number[],
      condition: null,
      dataFrom: null,
      retries: 0,
      outputSchema: "",
      loopIterations: 1,
      loopUntil: null,
      loopGroup: "",
      loopGroupTerminal: false,
      loopGroupIterations: 2,
      loopGroupStop: null,
      nodeType: "agent",
    }));
    showPlanModal = true;
  }

  function addPlanTask() {
    loadedWorkflow = null;
    planTasks = [
      ...planTasks,
      {
        id: String(planTasks.length),
        botId: members[0]?.bot_id ?? "",
        label: "New task",
        dependsOn: [],
        condition: null,
        dataFrom: null,
        retries: 0,
        outputSchema: "",
        loopIterations: 1,
        loopUntil: null,
        loopGroup: "",
        loopGroupTerminal: false,
        loopGroupIterations: 2,
        loopGroupStop: null,
        nodeType: "agent",
      },
    ];
  }

  function removePlanTask(idx: number) {
    loadedWorkflow = null;
    planTasks = planTasks
      .filter((_, i) => i !== idx)
      .map((task, i) => {
        const dependsOn = task.dependsOn
          .filter((d) => d !== idx)
          .map((d) => (d > idx ? d - 1 : d));
        let condition = task.condition;
        if (condition && condition.source === idx) {
          condition = null;
        } else if (condition && condition.source > idx) {
          condition = { ...condition, source: condition.source - 1 };
        }
        let dataFrom = task.dataFrom;
        if (dataFrom !== null) {
          dataFrom = dataFrom
            .filter((d) => d !== idx)
            .map((d) => (d > idx ? d - 1 : d));
          if (dataFrom.length === dependsOn.length) dataFrom = null;
        }
        return { ...task, id: String(i), dependsOn, condition, dataFrom };
      });
  }

  function updatePlanTaskLabel(idx: number, label: string) {
    loadedWorkflow = null;
    planTasks = planTasks.map((t, i) => (i === idx ? { ...t, label } : t));
  }

  // Let the room's lead draft the plan from the goal, then let the user edit it.
  async function draftPlan() {
    if (!planGoal.trim()) { planError = "Describe the goal first."; return; }
    loadedWorkflow = null;
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
        condition: t.condition ?? null,
        dataFrom: t.data_from ?? null,
        retries: t.retries ?? 0,
        outputSchema: t.output_schema ?? "",
        loopIterations: t.loop_iterations ?? 1,
        loopUntil: t.loop_until ?? null,
        loopGroup: t.loop_group ?? "",
        loopGroupTerminal: Boolean(t.loop_group_terminal),
        loopGroupIterations: t.loop_group_iterations ?? 2,
        loopGroupStop: t.loop_group_stop ?? null,
        nodeType: t.node_type ?? "agent",
      }));
    } catch (e: any) {
      planError = String(e);
    } finally {
      draftingPlan = false;
    }
  }

  function applyWorkflowTemplate(template: WorkflowTemplate) {
    const applied = materializeWorkflowTemplate(template, members);
    loadedWorkflow = null;
    planError = null;
    planQuestion = null;
    planGoal = applied.goal;
    planTasks = applied.tasks;
  }

  function updatePlanNodeType(idx: number, nodeType: string) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === idx ? { ...task, nodeType } : task,
    );
  }

  function updateTaskBot(idx: number, botId: string) {
    loadedWorkflow = null;
    planTasks = planTasks.map((t, i) => (i === idx ? { ...t, botId } : t));
  }

  function togglePlanDep(taskIdx: number, depIdx: number) {
    loadedWorkflow = null;
    planTasks = planTasks.map((t, i) => {
      if (i !== taskIdx) return t;
      const has = t.dependsOn.includes(depIdx);
      const dependsOn = has
        ? t.dependsOn.filter((d) => d !== depIdx)
        : [...t.dependsOn, depIdx];
      const condition =
        t.condition && dependsOn.includes(t.condition.source)
          ? t.condition
          : null;
      let dataFrom = t.dataFrom;
      if (dataFrom !== null) {
        dataFrom = dataFrom.filter((d) => dependsOn.includes(d));
        if (dataFrom.length === dependsOn.length) dataFrom = null;
      }
      return { ...t, dependsOn, condition, dataFrom };
    });
  }

  function togglePlanInput(taskIdx: number, depIdx: number) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) => {
      if (index !== taskIdx) return task;
      const current = task.dataFrom ?? task.dependsOn;
      const has = current.includes(depIdx);
      const next = has
        ? current.filter((d) => d !== depIdx)
        : [...current, depIdx];
      const dataFrom =
        next.length === task.dependsOn.length ? null : next;
      return { ...task, dataFrom };
    });
  }

  function updatePlanCondition(
    taskIdx: number,
    patch: Partial<PlanCondition>,
  ) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) => {
      if (index !== taskIdx) return task;
      const current =
        task.condition ??
        ({
          source: task.dependsOn[0] ?? 0,
          operator: "contains",
          value: "",
        } satisfies PlanCondition);
      return { ...task, condition: { ...current, ...patch } };
    });
  }

  function updatePlanRetries(taskIdx: number, value: string) {
    const retries = Math.max(0, Math.min(3, Number(value) || 0));
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx ? { ...task, retries } : task,
    );
  }

  function updatePlanOutputSchema(taskIdx: number, value: string) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx ? { ...task, outputSchema: value } : task,
    );
  }

  function updatePlanLoopIterations(taskIdx: number, value: string) {
    const loopIterations = Math.max(1, Math.min(10, Number(value) || 1));
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx
        ? {
            ...task,
            loopIterations,
            loopUntil:
              loopIterations > 1
                ? (task.loopUntil ?? {
                    operator: "contains",
                    value: "",
                  })
                : null,
          }
        : task,
    );
  }

  function updatePlanLoopUntil(
    taskIdx: number,
    patch: Partial<PlanLoopStop>,
  ) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) => {
      if (index !== taskIdx) return task;
      const current =
        task.loopUntil ??
        ({
          operator: "contains",
          value: "",
        } satisfies PlanLoopStop);
      return { ...task, loopUntil: { ...current, ...patch } };
    });
  }

  function clearPlanLoopUntil(taskIdx: number) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx ? { ...task, loopUntil: null } : task,
    );
  }

  function updatePlanLoopGroup(taskIdx: number, value: string) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx
        ? {
            ...task,
            loopGroup: value,
            loopGroupTerminal: value.trim() ? task.loopGroupTerminal : false,
            loopGroupStop: value.trim() ? task.loopGroupStop : null,
          }
        : task,
    );
  }

  function updatePlanGroupTerminal(taskIdx: number, terminal: boolean) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx
        ? {
            ...task,
            loopGroupTerminal: terminal,
            loopGroupStop: terminal
              ? (task.loopGroupStop ?? { operator: "contains", value: "" })
              : null,
          }
        : task,
    );
  }

  function updatePlanGroupIterations(taskIdx: number, value: string) {
    const loopGroupIterations = Math.max(2, Math.min(10, Number(value) || 2));
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx ? { ...task, loopGroupIterations } : task,
    );
  }

  function updatePlanGroupUntil(
    taskIdx: number,
    patch: Partial<PlanLoopStop>,
  ) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) => {
      if (index !== taskIdx) return task;
      const current =
        task.loopGroupStop ??
        ({ operator: "contains", value: "" } satisfies PlanLoopStop);
      return { ...task, loopGroupStop: { ...current, ...patch } };
    });
  }

  function clearPlanGroupUntil(taskIdx: number) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx ? { ...task, loopGroupStop: null } : task,
    );
  }

  function clearPlanCondition(taskIdx: number) {
    loadedWorkflow = null;
    planTasks = planTasks.map((task, index) =>
      index === taskIdx ? { ...task, condition: null } : task,
    );
  }

  async function dispatchPlan() {
    if (!planGoal.trim()) { planError = "Describe the goal first."; return; }
    if (planTasks.length === 0) { planError = "Add at least one task."; return; }
    const workflowRun = loadedWorkflow;
    const runGoal = planGoal;
    planError = null;
    planGenerating = true;
    let workflowRunId: string | null = null;
    if (workflowRun) {
      try {
        workflowRunId = await invoke<string>("begin_workflow_run", {
          workflowId: workflowRun.id,
          workflowVersion: workflowRun.version,
          goal: runGoal,
        });
      } catch {
        /* the plan can still run even if history could not be opened */
      }
    }
    resetRun();
    // Optimistically show the dispatched goal in the room feed.
    const tempUserMsg = {
      id: "temp-plan-" + Date.now(),
      role: "user",
      content: `🎯 **Plan Mode** — goal: ${planGoal.trim()}`,
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempUserMsg];
    pendingMessageId = tempUserMsg.id;
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
        condition: t.condition,
        data_from: t.dataFrom,
        retries: t.retries,
        output_schema: t.outputSchema.trim() ? t.outputSchema : null,
        loop_iterations: t.loopIterations,
        loop_until:
          t.loopIterations > 1 && t.loopUntil?.value.trim()
            ? t.loopUntil
            : null,
        loop_group: t.loopGroup.trim() || null,
        loop_group_terminal: Boolean(t.loopGroupTerminal),
        loop_group_iterations: t.loopGroupIterations,
        loop_group_stop:
          t.loopGroupTerminal &&
          t.loopGroupIterations > 1 &&
          t.loopGroupStop?.value.trim()
            ? t.loopGroupStop
            : null,
        node_type: t.nodeType,
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
        messages = await fetchMessageTail(tid);
        await tick();
        staticEntries = false;
        scrollToBottom(true);
      }
      rehydrateBoard(res);
      const outcome = workflowRunOutcome(res);
      if (workflowRunId) {
        void invoke("finish_workflow_run", {
          runId: workflowRunId,
          stateName: outcome.stateName,
          threadId: res.thread_id ?? null,
          detail: outcome.detail,
        }).catch(() => undefined);
      }
      if (outcome.stateName === "failed") {
        notify("Plan finished with incomplete tasks.", "error");
      }
    } catch (e: any) {
      if (workflowRunId) {
        void invoke("finish_workflow_run", {
          runId: workflowRunId,
          stateName: "failed",
          threadId: null,
          detail: String(e),
        }).catch(() => undefined);
      }
      notify(`Plan dispatch failed: ${String(e)}`, "error");
    } finally {
      planGenerating = false;
      commitLanes();
    }
  }

  /**
   * Wait for the detached office turn to finish.
   *
   * `send_to_chatroom` now returns as soon as the run is accepted, so the
   * composer must not re-enable while the office is still working. The `done`
   * event is the signal; the timeout is the escape hatch when a provider
   * hangs without ever reaching a terminal state.
   */
  function waitForDispatchDone(): Promise<void> {
    if (dispatchFinished) return Promise.resolve();
    return new Promise((resolve) => {
      dispatchDone = resolve;
      if (dispatchTimeout) clearTimeout(dispatchTimeout);
      dispatchTimeout = setTimeout(() => {
        dispatchTimeout = null;
        const finish = dispatchDone;
        dispatchDone = null;
        finish?.();
      }, 4 * 60 * 60 * 1000);

      // The `done` event is the fast path. The poll is the safety net for the
      // one case an event cannot cover: the run finished (or failed) before
      // this view had the thread id, so the event was filtered as foreign.
      if (dispatchPoll) clearInterval(dispatchPoll);
      dispatchPoll = setInterval(async () => {
        if (!dispatchDone) return;
        try {
          const active = await invoke<boolean>("get_office_active", { chatroomId: room.id });
          if (!active) signalDispatchDone();
        } catch {
          /* older backend: rely on the stream event */
        }
      }, 5000);
    });
  }

  function signalDispatchDone() {
    dispatchFinished = true;
    if (dispatchTimeout) {
      clearTimeout(dispatchTimeout);
      dispatchTimeout = null;
    }
    if (dispatchPoll) {
      clearInterval(dispatchPoll);
      dispatchPoll = null;
    }
    const finish = dispatchDone;
    dispatchDone = null;
    finish?.();
  }

  const agentMentions = $derived<MentionItem[]>(
    members
      .map((member: any) => ({
        id: String(member.bot?.id || member.bot_id || ""),
        label: String(member.bot?.name || member.bot_id || ""),
        detail: String(member.specialty || member.rank || ""),
        kind: "agent" as const,
      }))
      .filter((item: MentionItem) => item.id && item.label),
  );

  async function loadMentionSources() {
    try {
      const files = (await invoke<any[]>("list_recent_file_changes", { limit: 200 })) || [];
      const paths = [...new Set(files.map((file) => String(file.path || "")).filter(Boolean))];
      fileMentions = paths.slice(0, 100).map((path) => ({
        id: path,
        label: path,
        detail: "recent file",
        kind: "file" as const,
      }));
    } catch {
      fileMentions = [];
    }
    try {
      const skills = (await invoke<any[]>("list_all_skills")) || [];
      skillMentions = skills.slice(0, 100).map((skill: any) => ({
        id: String(skill.id || ""),
        label: String(skill.id || ""),
        detail: String(skill.description || ""),
        kind: "skill" as const,
      })).filter((item: MentionItem) => item.id);
    } catch {
      skillMentions = [];
    }
  }

  function closeMentions() {
    mentionOpen = false;
    mentionToken = null;
  }

  function updateMentions(value: string, caret: number) {
    const token = mentionAtCaret(value, caret);
    if (!token) {
      closeMentions();
      return;
    }
    const source =
      token.sigil === "@"
        ? agentMentions
        : token.sigil === "#"
          ? fileMentions
          : skillMentions;
    if (source.length === 0) {
      closeMentions();
      return;
    }
    mentionToken = token;
    mentionSigil = token.sigil;
    mentionItems = filterMentionItems(source, token.query);
    mentionIndex = 0;
    mentionOpen = mentionItems.length > 0;
  }

  function handleComposerInput(event: Event) {
    const target = event.target as HTMLTextAreaElement;
    const value = target.value;
    if (addressedTo && !value.includes(`@${addressedTo.label}`)) {
      addressedTo = null;
    }
    if (room?.id) drafts.save(officeDraftKey(room.id), value);
    updateMentions(value, target.selectionStart ?? value.length);
  }

  function chooseMention(item: MentionItem) {
    if (!mentionToken || !room?.id) return;
    const applied = applyMention(newMessage, mentionToken, item.label);
    newMessage = applied.text;
    if (item.kind === "agent") {
      addressedTo = { id: item.id, label: item.label };
    }
    closeMentions();
    drafts.save(officeDraftKey(room.id), applied.text);
    void tick().then(() => {
      composerRef?.focus();
      composerRef?.setSelectionRange(applied.caret, applied.caret);
    });
  }

  function clearAddressee() {
    if (!addressedTo || !room?.id) return;
    newMessage = newMessage
      .replace(`@${addressedTo.label}`, "")
      .replace(/[ \t]{2,}/gu, " ")
      .trimStart();
    addressedTo = null;
    drafts.save(officeDraftKey(room.id), newMessage);
    closeMentions();
    void tick().then(() => composerRef?.focus());
  }

  function handleMentionKeydown(event: KeyboardEvent): boolean {
    if (!mentionOpen) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      const total = mentionItems.length;
      mentionIndex = total > 0 ? (mentionIndex + delta + total) % total : 0;
      return true;
    }
    if (event.key === "Enter" || event.key === "Tab") {
      event.preventDefault();
      const item = mentionItems[mentionIndex];
      if (item) chooseMention(item);
      return true;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      closeMentions();
      return true;
    }
    return false;
  }

  function handleComposerKeydown(event: KeyboardEvent) {
    if (handleMentionKeydown(event)) return;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape" && replyingTo) {
      event.preventDefault();
      cancelReply();
    }
  }

  async function send(taskText?: string) {
    if (sessionRole.role === "viewer") return;
    const typed = (taskText || newMessage).trim();
    if (sending) return;
    // Sending starts a fresh turn and clears the local HITL cards. If a run is
    // parked on a decision, that would strand it with no way to answer.
    if (pendingApprovals.length > 0 || pendingQuestions.length > 0) {
      notify("Resolve the pending decision before sending a new task.", "error");
      return;
    }
    const attachments = pendingAttachments;
    if (!typed && attachments.length === 0) return;
    const text = typed || "(shared files)";

    sending = true;
    dispatchFinished = false;
    let shouldSignalDone = true;
    const replyToId = replyingTo?.id ?? null;
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
      reply_to_id: replyToId,
      reactions: [],
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempUserMsg];
    // An explicit dispatch always returns the feed to the bottom (OpenBot).
    scrollToBottom(true);

    try {
      const recipientBotId =
        addressedTo && text.includes(`@${addressedTo.label}`)
          ? addressedTo.id
          : undefined;
      const res: any = await invoke("send_to_chatroom", {
        chatroomId: room.id,
        content: text,
        attachments: attachments.length ? attachments : undefined,
        replyToId,
        recipientBotId,
      });
      replyingTo = null;
      addressedTo = null;
      drafts.clear(officeDraftKey(room.id));
      if (res?.queued) {
        announceAttentionChanged();
        // The queued message is not part of the transcript until its turn
        // starts; the queue panel is the honest place to show it.
        messages = messages.filter((message) => message.id !== tempUserMsg.id);
        pendingMessageId = null;
        if (res.thread_id) threadId = res.thread_id;
        await refreshQueue();
        if (res.started) {
          // An older queued turn (or this one) is now running. Follow it to
          // completion, then the queue worker will start the next item.
          if (threadId) {
            staticEntries = true;
            messages = await fetchMessageTail(threadId);
            await tick();
            staticEntries = false;
            scrollToBottom(true);
          }
          await waitForDispatchDone();
          if (threadId) {
            staticEntries = true;
            messages = await fetchMessageTail(threadId);
            await tick();
            staticEntries = false;
            commitLanes();
            void rehydratePersistedBoard(true);
            scrollToBottom(true);
          }
        } else {
          shouldSignalDone = false;
        }
        return;
      }
      if (res.thread_id) threadId = res.thread_id;
      staticEntries = true;
      if (threadId) messages = await fetchMessageTail(threadId);
      pendingMessageId = null;
      await tick();
      staticEntries = false;
      rehydrateBoard(res);
      scrollToBottom(true);

      // The command returned the accepted run, not its result. Keep the
      // composer locked until the office reports done, then fetch the final
      // conversation (the detached task posts it before emitting done).
      if (res?.accepted) {
        await waitForDispatchDone();
        if (threadId) {
          staticEntries = true;
          messages = await fetchMessageTail(threadId);
          await tick();
          staticEntries = false;
          commitLanes();
          void rehydratePersistedBoard(true);
          scrollToBottom(true);
        }
      }
    } catch (e) {
      // Never swallow the failure: toast it, restore the draft, and still
      // refetch so persisted agent replies surface even if the call rejected
      // late.
      newMessage = text;
      drafts.saveNow(officeDraftKey(room.id), text);
      pendingMessageId = null;
      notify(`Office dispatch failed: ${String(e)}`, "error");
      if (threadId) {
        try {
          staticEntries = true;
          messages = await fetchMessageTail(threadId);
        } catch {
          /* keep the optimistic row if the refetch also failed */
        } finally {
          staticEntries = false;
        }
      }
    } finally {
      pendingMessageId = null;
      if (shouldSignalDone) signalDispatchDone();
      sending = false;
      commitLanes();
      scrollToBottom();
    }
  }

  async function toggleAutonomy() {
    if (autonomyBusy) return;
    const next = !autonomyOn;
    autonomyBusy = true;
    autonomyOn = next;
    try {
      await invoke("set_office_autonomy", { chatroomId: room.id, enabled: next });
    } catch (e) {
      autonomyOn = !next;
      notify(`Could not change office autonomy: ${String(e)}`, "error");
    } finally {
      autonomyBusy = false;
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
      if (file.size > MAX_ATTACHMENT_BYTES) {
        notify(`Attachment is larger than ${MAX_ATTACHMENT_BYTES / (1024 * 1024)} MB.`, "error");
        continue;
      }
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

<svelte:window onkeydown={handleMessageNavKey} />

<div class="flex flex-col h-full overflow-hidden bg-[var(--surface-0)] text-[var(--text-primary)] font-sans">
  <!-- Office header. Three things, three places: who this office is (left),
       what it is doing (under the name), and who is in it plus the office
       actions (right).

       The roster used to be drawn twice — once as avatars here, again as name
       chips in a strip below — which spent a whole row saying the same thing.
       The avatars are now the roster, and they open the same manage sheet the
       chips did. The four action buttons lost their resting borders for the
       same reason: four equally weighted boxes were the loudest thing on the
       bar. -->
  <OfficeHeader
    roomName={room.name}
    roomAvatarUrl={room.avatar_url}
    roomAvatarStyle={room.avatar_style}
    officeTemplate={room.office_template}
    {members}
    {autonomyOn}
    {autonomyBusy}
    {officeActive}
    {sending}
    {teamActive}
    {officeTokens}
    {officeCost}
    {roomSearchOpen}
    {roomSearchQuery}
    roomSearchMatchCount={roomSearchMatches.length}
    {roomSearchPosition}
    bind:roomSearchInput
    activityFor={(botId) => fleetActivity.get(botId)}
    moodFor={(botId) => fleetActivity.mood(botId)}
    onToggleAutonomy={toggleAutonomy}
    onStopOffice={stopOfficeTurn}
    onManageAgent={openManageAgent}
    onOpenSearch={openRoomSearch}
    onCloseSearch={closeRoomSearch}
    onSearchInput={onRoomSearchInput}
    onStepSearch={stepRoomSearch}
    onHire={openHireModal}
    onMemory={() => (showOfficeMemory = true)}
    onPlan={openPlanModal}
    onSettings={() => (showOfficeSettings = true)}
  />

  <!-- Below the office header: conversation on the left, the board docked on
       the right. The board used to consume a full-width strip above the chat,
       which pushed the conversation down and made the office feel like a
       dashboard with a chat box attached. On a narrow window it falls back to
       the top of the column, where there is no room for a rail. -->
  <div
    class="flex-1 min-h-0 grid overflow-hidden {boardNodes.length > 0
      ? 'grid-cols-1 grid-rows-[auto_minmax(0,1fr)] xl:grid-cols-[minmax(0,1fr)_380px] xl:grid-rows-1 2xl:grid-cols-[minmax(0,1fr)_420px]'
      : 'grid-cols-1 grid-rows-1'}"
  >
    {#if boardNodes.length > 0}
      <aside
        class="order-first xl:order-last min-h-0 max-h-[42vh] xl:max-h-none overflow-y-auto border-b xl:border-b-0 xl:border-l border-[var(--hairline)] bg-[var(--surface-0)] px-3 py-2.5"
        aria-label={t("room.board.title")}
      >
        <OfficeBoard
          goal={boardGoal}
          nodes={boardNodes}
          {members}
          todos={botTodos}
          {workItems}
          runActive={teamActive}
          variant="rail"
          onSelect={(node) => (selectedBoardNode = node)}
        />
      </aside>
    {/if}

    <div class="min-w-0 min-h-0 flex flex-col overflow-hidden {boardNodes.length > 0 ? 'xl:col-span-1' : ''}">
      {#if officeActive}
        <div class="flex items-center gap-2 px-4 py-1.5 border-b border-[var(--hairline)] bg-[var(--brand-soft)] text-[11px] text-[var(--text-secondary)] shrink-0" role="status">
          <Loader2 class="size-3.5 animate-spin text-[var(--brand)] shrink-0" />
          <span class="font-semibold text-[var(--text-primary)]">{officePhaseLabel}</span>
          <span class="text-[var(--text-muted)]">{t("room.officeKeepsWorking")}</span>
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
    <div bind:this={chatContainer} onscroll={handleFeedScroll} aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown" class="virtual-scroll h-full overflow-y-auto p-4 sm:p-6 space-y-6 {staticEntries ? 'entries-static' : ''}">
      <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto min-h-full flex flex-col justify-end gap-6">
        {#if hiddenBefore > 0 || hasEarlier}
          <div class="load-earlier-row">
            <button
              type="button"
              class="load-earlier"
              onclick={() => void loadEarlier()}
            >
              {t("thread.loadEarlier", {
                count: hiddenBefore > 0 ? hiddenBefore : MESSAGE_PAGE_SIZE,
              })}
            </button>
          </div>
        {/if}
        {#if loadError && messages.length === 0}
          <div class="mx-auto my-8 max-w-xl rounded-2xl border border-[var(--danger-border)] bg-[var(--danger-soft)] p-5 text-center space-y-2" role="alert">
            <AlertTriangle class="mx-auto size-5 text-[var(--danger-text)]" />
            <p class="text-sm font-semibold text-[var(--danger-text)]">{t("errors.loadFailed")}</p>
            <p class="break-words text-xs text-[var(--text-secondary)]">{loadError}</p>
            <button
              type="button"
              class="mt-2 inline-flex h-8 items-center rounded-lg border border-[var(--danger-border)] bg-[var(--surface-1)] px-3 text-xs font-semibold text-[var(--danger-text)] hover:bg-[var(--surface-2)] cursor-pointer"
              onclick={() => void load()}
            >
              {t("ui.retry")}
            </button>
          </div>
        {:else if loadingRoom && messages.length === 0}
          <div class="flex items-center justify-center gap-2 py-16 text-xs text-[var(--text-tertiary)]" role="status">
            <Loader2 class="size-4 animate-spin" />
            {t("ui.loading")}
          </div>
        {:else if messages.length === 0}
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
                      <RavenAvatar name={m.rank} imageUrl={m.bot?.avatar_url}
                 style={m.bot?.avatar_style} />
                    </div>
                    <span class="text-[11px] font-bold text-[var(--text-secondary)] font-mono">{m.rank}</span>
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
              <span class="text-[11px] font-bold text-[var(--text-muted)] uppercase tracking-wider font-mono flex items-center gap-1.5">
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
        {:else}
          <VirtualTranscript
            items={visibleMessages}
            container={chatContainer}
            itemKey={messageKey}
            estimate={estimateMessageHeight}
            resetKey={room.id}
            gap={24}
            itemClass="flex flex-col gap-6"
            bind:target={virtualTarget}
            pin={virtualPin}
            onLayout={handleVirtualLayout}
          >
            {#snippet row(msg, index)}
          {@const mi = visibleStart + index}
          {@const dayKey = messageDayKey(msg.created_at)}
          {@const previousDayKey = mi > 0 ? messageDayKey(messages[mi - 1]?.created_at) : ""}
          {@const isUser = isUserMessage(msg)}
          {@const isError = typeof msg.content === "string" && msg.content.includes("⚠️ **Model Error:**")}
          {@const rawText = typeof msg.content === "string" ? msg.content : msg.content?.text || JSON.stringify(msg.content)}
          {@const autonomousCycle = isUser && rawText.startsWith("[Autonomous cycle]")}
          {@const senderBot = msg.sender_bot_id ? bots.find((b: any) => b.id === msg.sender_bot_id) : null}
          {@const senderName = senderBot?.name || msg.sender_name || room.name}
          {@const senderAvatar = senderBot?.avatar_url || getDiceBearUrl(senderName, senderBot?.avatar_style || DEFAULT_AVATAR_STYLE)}
          {@const messageAttachments = Array.isArray(msg.attachments) ? msg.attachments.filter((a: any) => a?.data) : []}
          {@const messageImages = messageAttachments.filter((a: any) => a.is_image)}
          {@const messageMedia = messageAttachments.filter((a: any) => !a.is_image && /^(audio|video)\//i.test(a.mime_type || ""))}
          {@const messageDocs = messageAttachments.filter((a: any) => !a.is_image && !/^(audio|video)\//i.test(a.mime_type || ""))}
          {@const messageSources = Array.isArray(msg.content?.sources) ? msg.content.sources : []}

          {#snippet messageAttachmentsBlock()}
            {#if messageImages.length}
              <ImageGallery
                images={messageImages.map((att: any) => ({
                  src: `data:${att.mime_type};base64,${att.data}`,
                  alt: att.name || "attached image",
                }))}
                label={`${messageImages.length} attached image${messageImages.length === 1 ? "" : "s"}`}
              />
            {/if}
            {#if messageMedia.length}
              <div class="message-media-list">
                {#each messageMedia as att}
                  {#if /^video\//i.test(att.mime_type)}
                    <!-- svelte-ignore a11y_media_has_caption -->
                    <video
                      class="message-media"
                      src={`data:${att.mime_type};base64,${att.data}`}
                      controls
                      preload="metadata"
                    ></video>
                  {:else}
                    <audio
                      class="message-media message-media--audio"
                      src={`data:${att.mime_type};base64,${att.data}`}
                      controls
                      preload="metadata"
                    ></audio>
                  {/if}
                {/each}
              </div>
            {/if}
            {#if messageDocs.length}
              <div class="file-attachment-list">
                {#each messageDocs as att (att.id || att.name)}
                  <FileAttachmentCard {att} />
                {/each}
              </div>
            {/if}
            {#if messageSources.length}
              <div class="px-3 pt-2">
                <SourceChips sources={messageSources} />
              </div>
            {/if}
          {/snippet}

          {#if dayKey && dayKey !== previousDayKey}
            <div
              class="flex items-center gap-3 px-1 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]"
              role="separator"
              aria-label={messageDayLabel(msg.created_at)}
            >
              <span class="h-px flex-1 bg-[var(--hairline)]" aria-hidden="true"></span>
              <span>{messageDayLabel(msg.created_at)}</span>
              <span class="h-px flex-1 bg-[var(--hairline)]" aria-hidden="true"></span>
            </div>
          {/if}

          {#if autonomousCycle}
            <div class="flex items-center gap-3 px-1 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
              <span class="h-px flex-1 bg-[var(--hairline)]" aria-hidden="true"></span>
              <span class="inline-flex items-center gap-1.5">
                <Workflow class="size-3 text-[var(--brand-text)]" strokeWidth={2} />
                {t("room.autonomousCycle")}
              </span>
              <span class="h-px flex-1 bg-[var(--hairline)]" aria-hidden="true"></span>
            </div>
          {:else}
          <ChatMessageRow
            {isUser}
            {isError}
            rowId={`message-${messageKey(msg)}`}
            messageId={isPersistedMessageId(msg.id) ? msg.id : undefined}
            searchHit={roomSearchOpen && roomSearchMatchSet.has(messageKey(msg))}
            searchCurrent={roomSearchOpen && roomCurrentSearchKey === messageKey(msg)}
            replyTo={replyQuoteFor(
              messagesForReplies,
              msg.reply_to_id,
              roomReplyAuthor,
              t("thread.replyUnavailable"),
            )}
            onReply={isPersistedMessageId(msg.id) ? () => startReply(msg) : undefined}
            onJumpToReply={jumpToMessage}
            reactions={Array.isArray(msg.reactions) ? msg.reactions : []}
            onReact={isPersistedMessageId(msg.id)
              ? (emoji) => void reactToMessage(msg, emoji)
              : undefined}
            text={rawText}
            onOpenArtifact={(code, lang) => {
              artifactStore.open(makeArtifact(code, lang));
              sidePanel.select("artifacts", sidePanel.scope());
            }}
            reasoning={typeof msg.content === "object" ? (msg.content?.reasoning ?? null) : null}
            tools={(typeof msg.content === "object" && msg.content?.tools) || []}
            spans={(typeof msg.content === "object" && msg.content?.spans) || []}
            time={formatTime(msg.created_at)}
            pending={msg.id === pendingMessageId}
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
            {#snippet userExtras()}
              {@render messageAttachmentsBlock()}
            {/snippet}
            {#snippet belowBubble()}
              {@render messageAttachmentsBlock()}
            {/snippet}
          </ChatMessageRow>
          {/if}
            {/snippet}
          </VirtualTranscript>
        {/if}

        <!-- Human-in-the-loop cards from office nodes -->
        <OfficeHITL
          approvals={pendingApprovals}
          questions={pendingQuestions}
          botNameFor={agentNameFor}
          decidingApproval={decidingApproval}
          answeringQuestion={answeringQuestion}
          onDecide={decideApproval}
          onAnswer={answerQuestion}
        />

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
                {#if h.childThreadId}
                  <button
                    type="button"
                    class="text-[11px] text-[var(--brand-text)] hover:underline shrink-0 cursor-pointer"
                    onclick={() => openHandoffThread(h)}
                  >
                    {t("attention.open")}
                  </button>
                {/if}
                {#if h.response}
                  <button
                    type="button"
                    class="text-[11px] text-[var(--brand-text)] hover:underline shrink-0 cursor-pointer"
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
            <div class="flex items-center gap-2 text-[11px] font-bold uppercase tracking-wider text-[var(--text-muted)]">
              <span class="size-1.5 rounded-full bg-success animate-pulse"></span>
              {t("room.liveTeam")} · {activeMembers.length > 0 ? t("room.liveWorking", { count: activeMembers.length }) : t("room.orchestrating")}
            </div>

            {#snippet statusRow(name: string, avatar: string, color: string, toolName?: string)}
              <div class="flex items-center gap-2 px-1 min-w-0 animate-rise-in">
                <img src={avatar} alt="" class="size-6 rounded-lg object-cover border border-[var(--hairline)] shrink-0" />
                <span class="text-[11px] font-bold truncate" style={`color: ${color}`}>{name}</span>
                {#if toolName}
                  <span class="font-mono text-[11px] px-1.5 py-0.5 rounded bg-info/10 border border-info/30 text-info inline-flex items-center gap-1 shrink-0">
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
              {@const isLive = !["idle", "done", "completed", "failed", "cancelled"].includes(st)}
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
                  reasoningThreadId={laneThreadId(key)}
                  reasoningLive={isLive}
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
                        <span class="font-mono text-[11px] px-1.5 py-0.5 rounded bg-info/10 border border-info/30 text-info inline-flex items-center gap-1">
                          <Wrench class="size-2.5" /> {toolName}
                        </span>
                      </div>
                    {:else if st === "thinking" || st === "running_tool" || st === "waiting_on_user"}
                      <div class="px-3">
                        <span class="font-mono text-[11px] px-1.5 py-0.5 rounded bg-warning/10 border border-warning/30 text-warning">
                          {st === "waiting_on_user" ? t("room.waiting") : t("room.thinking")}
                        </span>
                      </div>
                    {/if}
                    {#if laneSources[key]?.length || laneImages[key]?.length}
                      <div class="px-3 pt-2 space-y-2">
                        {#if laneSources[key]?.length}
                          <SourceChips sources={laneSources[key]} />
                        {/if}
                        {#if laneImages[key]?.length}
                          <div class="flex flex-wrap gap-2">
                            {#each laneImages[key] as img, index (img.data_url + index)}
                              <img
                                src={img.data_url}
                                alt={img.name}
                                class="max-h-64 max-w-full rounded-lg object-contain bg-[var(--surface-1)]"
                              />
                            {/each}
                          </div>
                        {/if}
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
                m?.bot?.avatar_url || getDiceBearUrl(name, m?.bot?.avatar_style || DEFAULT_AVATAR_STYLE),
                `hsl(${authorHue(m.bot?.id || name)} 55% 68%)`,
                agentTool[m.bot?.id],
              )}
            {/each}

            {#if activeMembers.length === 0}
              <div class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)] pl-1">
                <Loader2 class="size-3.5 animate-spin text-[var(--brand)]" />
                <span>{planGenerating ? t("room.dispatchingPlan") : t("room.planningWork")}</span>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
      <MessageSelectionToolbar
        container={chatContainer}
        onReply={replyFromSelection}
        onQuote={quoteFromSelection}
      />
      {#if !stickToLatest}
        <button
          type="button"
          class="jump-latest"
          onclick={jumpToLatest}
          title={unseenMessages > 0
            ? t(unseenMessages === 1 ? "thread.newMessage" : "thread.newMessages", {
                count: unseenMessages,
              })
            : t("room.jumpLatest")}
          aria-label={unseenMessages > 0
            ? t(unseenMessages === 1 ? "thread.newMessage" : "thread.newMessages", {
                count: unseenMessages,
              })
            : t("room.jumpLatest")}
        >
          <ArrowDown class="size-3.5" />
          {#if unseenMessages > 0}
            <span class="jump-latest__count" aria-hidden="true">{unseenMessages}</span>
          {/if}
          <span>{t("room.jumpLatest")}</span>
        </button>
      {/if}
      <TranscriptRail
        container={chatContainer}
        entries={railEntries}
        positions={railPositions}
        onJump={jumpToRail}
      />
    </div>

    <!-- Grok Floating Capsule Compose Bar -->
    <OfficeComposer
      nodes={boardNodes}
      handoffs={threadHandoffs}
      {queueItems}
      {queuePaused}
      {queueBusy}
      nameFor={agentNameFor}
      onOpenNode={openBoardNodeThread}
      onOpenHandoff={openHandoffThread}
      onToggleQueue={toggleQueuePause}
      onRunQueueNext={runQueueItemNext}
      canMoveQueueItem={canMoveQueueItem}
      onMoveQueueItem={moveQueueItem}
      onRemoveQueueItem={removeQueueItem}
      reply={activeReply}
      onCancelReply={cancelReply}
      attachments={pendingAttachments}
      onRemoveAttachment={removePendingAttachment}
      addressee={addressedTo}
      onClearAddressee={clearAddressee}
      {mentionOpen}
      {mentionSigil}
      {mentionItems}
      {mentionIndex}
      onPickMention={chooseMention}
      bind:draft={newMessage}
      bind:composerRef
      onInput={handleComposerInput}
      onKeydown={handleComposerKeydown}
      {sending}
      memberCount={members.length}
      roomName={room.name}
      contextUsage={officeContext}
      {isListening}
      onAttach={attachFile}
      onToggleVoice={toggleVoice}
      onSend={send}
    />
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
    class="fixed inset-0 z-50 bg-[var(--scrim)]"
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
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--scrim)] " role="button" tabindex="0" aria-label={t("room.close")} onclick={(e) => { if (e.target === e.currentTarget) showPlanModal = false; }} onkeydown={(e) => { if (e.key === "Escape") showPlanModal = false; }}>
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
              class="h-6 px-2.5 text-[11px] rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer disabled:opacity-40 flex items-center gap-1"
              title={t("room.draftTitle")}
            >
              {#if draftingPlan}
                <Loader2 class="size-3 animate-spin" /> {t("room.drafting")}
              {:else}
                <Sparkles class="size-3" /> {t("room.draftWithLead")}
              {/if}
            </button>
          </div>
          <textarea id="plan-goal" bind:value={planGoal} oninput={() => (loadedWorkflow = null)} rows={2} placeholder={t("room.planGoalPlaceholder")} class="w-full px-3 py-2 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus:border-[var(--brand)]/50"></textarea>
        </div>

        {#if members.length > 0}
        <div class="space-y-1.5">
          <p class="text-[11px] font-mono uppercase tracking-wider text-[var(--text-muted)]">{t("room.templates")}</p>
          <div class="flex flex-wrap gap-1.5">
            {#each WORKFLOW_TEMPLATES as template (template.id)}
              <button
                type="button"
                onclick={() => applyWorkflowTemplate(template)}
                title={template.description}
                class="px-2.5 py-1 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[11px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--brand)]/40 cursor-pointer"
              >
                {template.name}
              </button>
            {/each}
          </div>
          <p class="text-[11px] text-[var(--text-muted)]">{t("room.templatesHint")}</p>
        </div>
        {/if}

        <WorkflowLibrary
          officeId={room.id}
          {members}
          goal={planGoal}
          tasks={planTasks}
          onLoad={(
            loadedGoal: string,
            loadedTasks: Array<{
              id: string;
              botId: string;
              label: string;
              dependsOn: number[];
              condition: PlanCondition | null;
              dataFrom: number[] | null;
              retries: number;
              outputSchema: string;
              loopIterations: number;
              loopUntil: PlanLoopStop | null;
              loopGroup: string;
              loopGroupTerminal: boolean;
              loopGroupIterations: number;
              loopGroupStop: PlanLoopStop | null;
              nodeType: string;
            }>,
            meta: { id: string; version: number; name: string },
          ) => {
            planGoal = loadedGoal;
            planTasks = loadedTasks;
            loadedWorkflow = meta;
            planError = null;
          }}
          onRun={(
            runGoal: string,
            runTasks: Array<{
              id: string;
              botId: string;
              label: string;
              dependsOn: number[];
              condition: PlanCondition | null;
              dataFrom: number[] | null;
              retries: number;
              outputSchema: string;
              loopIterations: number;
              loopUntil: PlanLoopStop | null;
              loopGroup: string;
              loopGroupTerminal: boolean;
              loopGroupIterations: number;
              loopGroupStop: PlanLoopStop | null;
              nodeType: string;
            }>,
            meta: { id: string; version: number; name: string },
          ) => {
            planGoal = runGoal;
            planTasks = runTasks;
            loadedWorkflow = meta;
            planError = null;
            void tick().then(() => void dispatchPlan());
          }}
        />

        {#if planQuestion}
          <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-xs text-[var(--brand-text)] space-y-1">
            <div class="font-bold flex items-center gap-1.5">❓ {t("room.leadClarify")}</div>
            <p>{planQuestion}</p>
            <p class="text-[11px] text-[var(--brand-text)]/70">{t("room.refineHint")}</p>
          </div>
        {/if}

        {#if members.length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each members as m}
              <span class="text-[11px] font-mono px-2 py-0.5 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)]">{m.rank} · {m.specialty}</span>
            {/each}
          </div>
        {/if}

        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <p class="text-xs font-bold text-[var(--text-primary)]">{t("room.tasks")} ({planTasks.length})</p>
            <span onclick={addPlanTask} class="text-[11px] text-[var(--brand-text)] hover:text-[var(--brand-text)] cursor-pointer" role="button" tabindex="0" onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); addPlanTask(); } }}>{t("room.addTask")}</span>
          </div>
          {#each planTasks as task, idx}
            <div class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] space-y-2">
              <div class="flex items-center gap-2">
                <SimpleSelect
                  value={task.nodeType}
                  options={[
                    { value: "trigger", label: t("room.nodeTrigger") },
                    { value: "agent", label: t("room.nodeAgent") },
                    { value: "llm", label: t("room.nodeLlm") },
                    { value: "approval", label: t("room.nodeApproval") },
                    { value: "output", label: t("room.nodeOutput") },
                  ]}
                  onValueChange={(v) => updatePlanNodeType(idx, v)}
                  class="h-7 w-28 text-[11px] rounded-lg"
                />
                <SimpleSelect
                  value={task.botId}
                  options={members.map((m: any) => ({ value: m.bot_id, label: `${m.bot?.name || m.rank} · ${m.specialty}` }))}
                  onValueChange={(v) => updateTaskBot(idx, v)}
                  class="h-7 w-44 text-[11px] rounded-lg"
                />
                <input bind:value={task.label} oninput={(e) => updatePlanTaskLabel(idx, (e.target as HTMLInputElement).value)} class="flex-1 h-7 px-2 text-[11px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-[var(--brand)]/50" placeholder={t("room.taskPlaceholder")} />
                <label class="flex shrink-0 items-center gap-1 text-[11px] text-[var(--text-muted)]" title={t("room.retriesHint")}>
                  {t("room.retries")}
                  <input
                    type="number"
                    min="0"
                    max="3"
                    value={task.retries}
                    aria-label={t("room.retries")}
                    oninput={(event) =>
                      updatePlanRetries(
                        idx,
                        (event.target as HTMLInputElement).value,
                      )}
                    class="h-7 w-11 px-1 text-[11px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-[var(--brand)]/50"
                  />
                </label>
                <button type="button" onclick={() => removePlanTask(idx)} aria-label={t("a11y.removeTask")} class="text-[var(--text-muted)] hover:text-danger cursor-pointer">✕</button>
              </div>
              {#if idx > 0}
                <div class="flex items-center gap-1 flex-wrap">
                  <span class="text-[11px] font-mono text-[var(--text-muted)] uppercase tracking-wider">{t("room.dependsOn")}</span>
                  {#each planTasks as _, oidx}
                    {#if oidx < idx}
                      {@const depOn = task.dependsOn.includes(oidx)}
                      <button
                        type="button"
                        onclick={() => togglePlanDep(idx, oidx)}
                        aria-pressed={depOn}
                        class="h-5 px-1.5 text-[11px] font-mono rounded-md border cursor-pointer {depOn
                          ? 'border-[var(--brand)]/60 bg-[var(--brand-soft)] text-[var(--brand-text)]'
                          : 'border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-muted)] hover:text-[var(--text-secondary)]'}"
                      >#{oidx + 1}</button>
                    {/if}
                  {/each}
                </div>
              {/if}
              {#if task.dependsOn.length > 0}
                <div class="flex flex-wrap items-center gap-1">
                  <span class="text-[11px] font-mono text-[var(--text-muted)] uppercase tracking-wider">
                    {t("room.runIf")}
                  </span>
                  {#if task.condition}
                    <SimpleSelect
                      value={String(task.condition.source)}
                      options={task.dependsOn.map((dep) => ({
                        value: String(dep),
                        label: `#${dep + 1}`,
                      }))}
                      onValueChange={(value) =>
                        updatePlanCondition(idx, { source: Number(value) })}
                      class="h-6 w-14 text-[11px] rounded-md"
                    />
                    <SimpleSelect
                      value={task.condition.operator}
                      options={[
                        { value: "contains", label: t("room.contains") },
                        {
                          value: "not_contains",
                          label: t("room.notContains"),
                        },
                      ]}
                      onValueChange={(value) =>
                        updatePlanCondition(idx, { operator: value })}
                      class="h-6 w-32 text-[11px] rounded-md"
                    />
                    <input
                      value={task.condition.value}
                      oninput={(event) =>
                        updatePlanCondition(idx, {
                          value: (event.target as HTMLInputElement).value,
                        })}
                      aria-label={t("room.conditionValue")}
                      placeholder={t("room.conditionValue")}
                      class="min-w-32 flex-1 h-6 px-2 text-[11px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-md focus:outline-none focus:border-[var(--brand)]/50"
                    />
                    <button
                      type="button"
                      onclick={() => clearPlanCondition(idx)}
                      class="text-[11px] text-[var(--text-muted)] hover:text-danger cursor-pointer"
                    >
                      {t("room.removeCondition")}
                    </button>
                  {:else}
                    <button
                      type="button"
                      onclick={() => updatePlanCondition(idx, {})}
                      class="text-[11px] text-[var(--brand-text)] hover:underline cursor-pointer"
                    >
                      {t("room.addCondition")}
                    </button>
                  {/if}
                </div>
              {/if}
              {#if task.dependsOn.length > 0}
                <div class="flex flex-wrap items-center gap-1">
                  <span class="text-[11px] font-mono text-[var(--text-muted)] uppercase tracking-wider">
                    {t("room.inputs")}
                  </span>
                  {#each task.dependsOn as dep}
                    {@const inputOn = (task.dataFrom ?? task.dependsOn).includes(dep)}
                    <button
                      type="button"
                      onclick={() => togglePlanInput(idx, dep)}
                      aria-pressed={inputOn}
                      class="h-5 px-1.5 text-[11px] font-mono rounded-md border cursor-pointer {inputOn
                        ? 'border-[var(--brand)]/60 bg-[var(--brand-soft)] text-[var(--brand-text)]'
                        : 'border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-muted)] hover:text-[var(--text-secondary)]'}"
                    >#{dep + 1}</button>
                  {/each}
                  <span class="text-[11px] text-[var(--text-muted)]">
                    {t("room.inputsHint")}
                  </span>
                </div>
              {/if}
              <details class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)]/50">
                <summary class="cursor-pointer px-2 py-1 text-[11px] text-[var(--text-muted)] hover:text-[var(--text-secondary)]">
                  {t("room.typedOutput")}
                  {#if task.outputSchema.trim()}
                    <span class="ml-1 text-[var(--brand-text)]">· {t("room.typedOutputOn")}</span>
                  {/if}
                </summary>
                <div class="px-2 pb-2 space-y-1">
                  <textarea
                    value={task.outputSchema}
                    oninput={(event) =>
                      updatePlanOutputSchema(
                        idx,
                        (event.target as HTMLTextAreaElement).value,
                      )}
                    rows={4}
                    spellcheck="false"
                    aria-label={t("room.typedOutput")}
                    placeholder={t("room.typedOutputPlaceholder")}
                    class="w-full px-2 py-1.5 font-mono text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-md resize-y focus:outline-none focus:border-[var(--brand)]/50"
                  ></textarea>
                  <p class="text-[11px] text-[var(--text-muted)]">
                    {t("room.typedOutputHint")}
                  </p>
                </div>
              </details>
              <details class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)]/50">
                <summary class="cursor-pointer px-2 py-1 text-[11px] text-[var(--text-muted)] hover:text-[var(--text-secondary)]">
                  {t("room.repeat")}
                  {#if task.loopIterations > 1}
                    <span class="ml-1 text-[var(--brand-text)]">· {task.loopIterations}×</span>
                  {/if}
                </summary>
                <div class="px-2 pb-2 space-y-1.5">
                  <div class="flex flex-wrap items-center gap-2">
                    <label class="flex items-center gap-1 text-[11px] text-[var(--text-muted)]">
                      {t("room.repeatRuns")}
                      <input
                        type="number"
                        min="1"
                        max="10"
                        value={task.loopIterations}
                        aria-label={t("room.repeatRuns")}
                        oninput={(event) =>
                          updatePlanLoopIterations(
                            idx,
                            (event.target as HTMLInputElement).value,
                          )}
                        class="h-7 w-12 px-1 text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-md focus:outline-none focus:border-[var(--brand)]/50"
                      />
                    </label>
                    {#if task.loopIterations > 1}
                      <span class="text-[11px] text-[var(--text-muted)]">
                        {t("room.repeatStopWhen")}
                      </span>
                      <SimpleSelect
                        value={task.loopUntil?.operator ?? "contains"}
                        options={[
                          { value: "contains", label: t("room.contains") },
                          {
                            value: "not_contains",
                            label: t("room.notContains"),
                          },
                        ]}
                        onValueChange={(value) =>
                          updatePlanLoopUntil(idx, { operator: value })}
                        class="h-7 w-32 text-[11px] rounded-lg"
                      />
                      <input
                        value={task.loopUntil?.value ?? ""}
                        oninput={(event) =>
                          updatePlanLoopUntil(idx, {
                            value: (event.target as HTMLInputElement).value,
                          })}
                        aria-label={t("room.repeatPhrase")}
                        placeholder={t("room.repeatPhrase")}
                        class="min-w-28 flex-1 h-7 px-2 text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-md focus:outline-none focus:border-[var(--brand)]/50"
                      />
                      {#if task.loopUntil}
                        <button
                          type="button"
                          onclick={() => clearPlanLoopUntil(idx)}
                          class="text-[11px] text-[var(--text-muted)] hover:text-[var(--text-secondary)] cursor-pointer"
                        >
                          {t("room.repeatClearStop")}
                        </button>
                      {/if}
                    {/if}
                  </div>
                  <div class="pt-2 border-t border-[var(--hairline)] space-y-1.5">
                    <div class="flex items-center gap-2">
                      <span class="text-[11px] text-[var(--text-muted)] shrink-0">{t("room.loopGroup")}</span>
                      <input
                        value={task.loopGroup}
                        oninput={(event) =>
                          updatePlanLoopGroup(
                            idx,
                            (event.target as HTMLInputElement).value,
                          )}
                        aria-label={t("room.loopGroup")}
                        placeholder={t("room.loopGroupPlaceholder")}
                        class="min-w-24 flex-1 h-7 px-2 text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-md focus:outline-none focus:border-[var(--brand)]/50"
                      />
                      {#if task.loopGroup.trim()}
                        <label class="flex items-center gap-1 text-[11px] text-[var(--text-muted)] cursor-pointer">
                          <input
                            type="checkbox"
                            checked={task.loopGroupTerminal}
                            onchange={(event) =>
                              updatePlanGroupTerminal(
                                idx,
                                (event.target as HTMLInputElement).checked,
                              )}
                            aria-label={t("room.loopGroupTerminal")}
                          />
                          {t("room.loopGroupTerminal")}
                        </label>
                      {/if}
                    </div>
                    {#if task.loopGroup.trim() && task.loopGroupTerminal}
                      <div class="flex flex-wrap items-center gap-2">
                        <label class="flex items-center gap-1 text-[11px] text-[var(--text-muted)]">
                          {t("room.loopGroupRuns")}
                          <input
                            type="number"
                            min="2"
                            max="10"
                            value={task.loopGroupIterations}
                            aria-label={t("room.loopGroupRuns")}
                            oninput={(event) =>
                              updatePlanGroupIterations(
                                idx,
                                (event.target as HTMLInputElement).value,
                              )}
                            class="h-7 w-12 px-1 text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-md focus:outline-none focus:border-[var(--brand)]/50"
                          />
                        </label>
                        <span class="text-[11px] text-[var(--text-muted)]">{t("room.loopGroupStop")}</span>
                        <SimpleSelect
                          value={task.loopGroupStop?.operator ?? "contains"}
                          options={[
                            { value: "contains", label: t("room.contains") },
                            { value: "not_contains", label: t("room.notContains") },
                          ]}
                          onValueChange={(value) =>
                            updatePlanGroupUntil(idx, { operator: value })}
                          class="h-7 w-32 text-[11px] rounded-lg"
                        />
                        <input
                          value={task.loopGroupStop?.value ?? ""}
                          oninput={(event) =>
                            updatePlanGroupUntil(idx, {
                              value: (event.target as HTMLInputElement).value,
                            })}
                          aria-label={t("room.repeatPhrase")}
                          placeholder={t("room.repeatPhrase")}
                          class="min-w-28 flex-1 h-7 px-2 text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-md focus:outline-none focus:border-[var(--brand)]/50"
                        />
                        {#if task.loopGroupStop}
                          <button
                            type="button"
                            onclick={() => clearPlanGroupUntil(idx)}
                            class="text-[11px] text-[var(--text-muted)] hover:text-[var(--text-secondary)] cursor-pointer"
                          >
                            {t("room.repeatClearStop")}
                          </button>
                        {/if}
                      </div>
                    {/if}
                    {#if task.loopGroup.trim()}
                      <p class="text-[11px] text-[var(--text-muted)]">{t("room.loopGroupHint")}</p>
                    {/if}
                  </div>
                  <p class="text-[11px] text-[var(--text-muted)]">
                    {t("room.repeatHint")}
                  </p>
                </div>
              </details>
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
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--scrim)] " role="button" tabindex="0" aria-label={t("room.close")} onclick={(e) => { if (e.target === e.currentTarget) showHireModal = false; }} onkeydown={(e) => { if (e.key === "Escape") showHireModal = false; }}>
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
            <span class="text-[11px] text-[var(--text-muted)]">
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
          <p class="text-[11px] text-[var(--text-muted)] leading-relaxed pt-0.5">
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
            <span onclick={addHireRole} class="text-[11px] text-success hover:text-success cursor-pointer" role="button" tabindex="0" onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); addHireRole(); } }}>{t("room.addRole")}</span>
          </div>

          {#each hireRoles as role, idx}
            <div class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] space-y-2">
              <div class="flex items-center gap-2">
                <label class="flex items-center gap-1 text-[11px] text-[var(--text-tertiary)] shrink-0 cursor-pointer" title={t("room.makeLead")}>
                  <input type="radio" name="hire-lead" checked={role.is_lead} onchange={() => setHireLead(idx)} class="accent-[var(--brand)] cursor-pointer" />
                  {t("room.lead")}
                </label>
                <input bind:value={role.name} aria-label={t("room.name")} placeholder={t("room.name")} class="w-28 h-7 px-2 text-[11px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <input bind:value={role.rank} aria-label={t("room.rank")} placeholder={t("room.rank")} class="w-24 h-7 px-2 text-[11px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <input bind:value={role.specialty} aria-label={t("room.specialty")} placeholder={t("room.specialty")} class="flex-1 h-7 px-2 text-[11px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <button type="button" onclick={() => removeHireRole(idx)} aria-label={t("a11y.removeRole")} class="text-[var(--text-muted)] hover:text-danger cursor-pointer">✕</button>
              </div>
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  onclick={() => (openRolePrompt = openRolePrompt === idx ? null : idx)}
                  aria-expanded={openRolePrompt === idx}
                  class="text-[11px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
                >
                  {openRolePrompt === idx ? `▾ ${t("room.hideInstructions")}` : `▸ ${t("room.roleInstructions")}`}
                </button>
                {#if role.is_lead}
                  <span class="text-[11px] px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] font-bold">{t("room.ceoBadge")}</span>
                {/if}
                {#if role.skills.length}
                  <span class="text-[11px] text-[var(--text-muted)] truncate">{t("room.skillsCount", { n: role.skills.length })}</span>
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
    class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--scrim)] "
    role="button"
    tabindex="0"
    aria-label={t("room.close")}
    onclick={(e) => { if (e.target === e.currentTarget) manageBot = null; }}
    onkeydown={(e) => { if (e.key === "Escape") manageBot = null; }}
  >
    <div class="modal-panel w-full max-w-md p-5 space-y-4 animate-scale-in">
      <div class="flex items-center gap-3">
        <div class="size-11 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline-strong)] shrink-0">
          <RavenAvatar name={manageBot.name} imageUrl={manageBot.avatar_url} style={manageBot.avatar_style} />
        </div>
        <div class="min-w-0">
          <div class="font-bold text-sm text-[var(--text-primary)] truncate flex items-center gap-1.5">
            {manageBot.name}
            {#if manageBot.is_orchestrator}<span class="text-[11px] px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] font-bold uppercase">{t("room.leadBadge")}</span>{/if}
          </div>
          <div class="text-[11px] text-[var(--text-tertiary)] truncate">
            {manageBot.rank || t("ui.fallbackAgent")} · {manageBot.specialty || t("ui.fallbackGeneralist")}
          </div>
          <div class="text-[11px] font-mono text-[var(--text-muted)] truncate mt-0.5">
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
            <span class="block text-[11px] text-[var(--text-tertiary)]">{t("room.manageModelDesc")}</span>
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
            <span class="block text-[11px] text-[var(--text-tertiary)]">{t("room.manageSkillsDesc")}</span>
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
            <span class="block text-[11px] text-[var(--text-tertiary)]">{t("room.manageMcpDesc")}</span>
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

{#if selectedBoardNode}
  <WorkItemDrawer
    itemId={selectedBoardNode.nodeId}
    label={selectedBoardNode.label}
    botName={agentNameFor(selectedBoardNode.botId)}
    onClose={() => (selectedBoardNode = null)}
  />
{/if}
