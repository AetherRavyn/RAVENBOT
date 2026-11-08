<script lang="ts">
  import { reasoning } from "$lib/chat/reasoning.svelte";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy, tick } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import AgentIntelligence from "$lib/components/AgentIntelligence.svelte";
  import ChatMessageRow from "$lib/components/chat/ChatMessageRow.svelte";
  import ChatEmptyState from "$lib/components/chat/ChatEmptyState.svelte";
  import { matchingMessageIds, messageKey, stepMatchIndex } from "$lib/chat/search";
  import { agentActivity } from "$lib/agentActivity.svelte";
  import {
    MESSAGE_FOCUS_EVENT,
    takePendingMessageFocus,
    type MessageFocusTarget,
  } from "$lib/chat/messageFocus";
  import { isPersistedMessageId, toggleReaction as nextReactions } from "$lib/chat/reactions";
  import { replyQuoteFor } from "$lib/chat/replies";
  import { isEditableTarget, navigateMessage } from "$lib/chat/navigation";
  import { anchoredScrollTop, transcriptWindow } from "$lib/chat/messageWindow";
  import ApprovalCard from "$lib/components/chat/ApprovalCard.svelte";
  import QuestionCard from "$lib/components/chat/QuestionCard.svelte";
  import ContextMeter from "$lib/components/chat/ContextMeter.svelte";
  import ImageGallery from "$lib/components/chat/ImageGallery.svelte";
  import FileAttachmentCard from "$lib/components/chat/FileAttachmentCard.svelte";
  import { contextUsage } from "$lib/chat/contextUsage.svelte";
  import ModelPicker from "$lib/components/ModelPicker.svelte";
  import ArtifactPanel from "$lib/components/ArtifactPanel.svelte";
  import RoutinesPanel from "$lib/components/RoutinesPanel.svelte";
  import SyncPanel from "$lib/components/SyncPanel.svelte";
  import ComputerPanel from "$lib/components/ComputerPanel.svelte";
  import ChannelsPanel from "$lib/components/ChannelsPanel.svelte";
  import TeamImport from "$lib/components/TeamImport.svelte";
  import type { Artifact } from "$lib/artifact";
  import { getDiceBearUrl, isUserMessage } from "$lib/utils";
  import { notify } from "$lib/toast";
  import { cn } from "$lib/utils.js";
  import {
    recordUtterance,
    transcribeBlob,
    speakText,
    stripForSpeech,
    voiceErrorMessage,
    type UtteranceRecorder,
    type SpeechHandle,
  } from "$lib/voice";
  import {
    ACCEPTED_IMAGE_MIMES,
    MAX_ATTACHMENT_BYTES,
    isTextFile,
    readAsDataUrl,
    readAsText,
    type PendingAttachment,
  } from "$lib/attachments";
  import {
    getCatalog, providerById, modelsFor, modelSummary, modelMetaFor,
    type CatalogProvider,
  } from "$lib/model-catalog";
  import {
    Plus,
    DollarSign,
    Pause,
    Play,
    Paperclip,
    Mic,
    Loader2,
    Volume2,
    Ghost,
    Clock,
    Pencil,
    CheckCircle2,
    XCircle,
    Circle,
    Sparkles,
    MessageSquare,
    Brain,
    AlertTriangle,
    Key,
    Settings,
    ShieldAlert,
    ArrowUp,
    ArrowDown,
    ChevronDown,
    ChevronUp,
    X,
    Reply,
    Copy,
    Check,
    RotateCcw,
    Boxes,
    Search,
    Globe,
    History,
    Trash2,
    Terminal,
    Cpu,
    Monitor,
    Hash,
    Users,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    onBotUpdated?: (bot: any) => void;
  }

  let { bot, onBotUpdated }: Props = $props();

  let threads = $state<any[]>([]);
  let selectedThreadId = $state<string | null>(null);
  let messages = $state<any[]>([]);
  /** The first thread load, so a focus request can wait for it instead of racing it. */
  let initialLoad: Promise<void> | null = null;
  let loadedBotId: string | null = null;
  let loadSeq = 0;

  // ── In-thread search ───────────────────────────────────────────────────
  // The query filters over the text each row renders. Matches are message
  // keys, not indices, so a hit survives a reload or a streaming append.
  let threadSearchOpen = $state(false);
  let threadSearchQuery = $state("");
  let threadSearchIndex = $state(0);
  let threadSearchInput = $state<HTMLInputElement | null>(null);
  let threadSearchTimer: ReturnType<typeof setTimeout> | null = null;
  const threadSearchMatches = $derived(matchingMessageIds(messages, threadSearchQuery));
  const threadSearchMatchSet = $derived(new Set(threadSearchMatches));
  const threadSearchPosition = $derived(
    threadSearchMatches.length ? Math.min(threadSearchIndex, threadSearchMatches.length - 1) : -1,
  );
  const threadCurrentSearchKey = $derived(
    threadSearchPosition >= 0 ? threadSearchMatches[threadSearchPosition] : "",
  );

  // Bounded transcript: mount a tail, not the whole history. See the same
  // rule in ChatRoomView — a long conversation should not cost more to open
  // than the part the reader can actually see.
  const MESSAGE_WINDOW = 50;
  const MESSAGE_WINDOW_STEP = 50;
  let messageWindow = $state(MESSAGE_WINDOW);
  let loadingEarlier = $state(false);
  let windowEnd = $state(0);
  const windowBounds = $derived(
    transcriptWindow(messages.length, messageWindow, windowEnd),
  );
  const visibleStart = $derived(windowBounds.start);
  const visibleEnd = $derived(windowBounds.end);
  const visibleMessages = $derived(messages.slice(visibleStart, visibleEnd));
  const hiddenBefore = $derived(visibleStart);

  async function loadEarlier() {
    if (loadingEarlier || hiddenBefore <= 0) return;
    const el = messagesContainer;
    const previousTop = el?.scrollTop ?? 0;
    const previousHeight = el?.scrollHeight ?? 0;
    loadingEarlier = true;
    try {
      messageWindow += MESSAGE_WINDOW_STEP;
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
    document
      .getElementById(`message-${key}`)
      ?.scrollIntoView({
        block: "center",
        behavior: prefersReducedMotion() ? "auto" : "smooth",
      });
  }

  async function scrollToSearchMatch(index: number) {
    const key = threadSearchMatches[index];
    if (key) await revealMessage(key);
  }

  function onSearchInput(event: Event) {
    threadSearchQuery = (event.currentTarget as HTMLInputElement).value;
    threadSearchIndex = 0;
    if (threadSearchTimer) clearTimeout(threadSearchTimer);
    // Snap to the first hit once typing settles, not on every keystroke.
    threadSearchTimer = setTimeout(() => {
      if (threadSearchQuery.trim()) void scrollToSearchMatch(0);
    }, 160);
  }

  function stepSearch(delta: number) {
    if (!threadSearchMatches.length) return;
    const base = threadSearchPosition < 0 ? 0 : threadSearchPosition;
    threadSearchIndex = stepMatchIndex(base, threadSearchMatches.length, delta);
    void scrollToSearchMatch(threadSearchIndex);
  }

  function openSearch() {
    threadSearchOpen = true;
    void tick().then(() => threadSearchInput?.focus());
  }

  function closeSearch() {
    threadSearchOpen = false;
    threadSearchQuery = "";
    threadSearchIndex = 0;
    if (threadSearchTimer) clearTimeout(threadSearchTimer);
  }

  // ── Quoted replies ─────────────────────────────────────────────────────
  // The quote is resolved from the loaded transcript rather than copied into
  // the message, so an edited original stays current and a deleted one is
  // reported honestly.
  let replyingTo = $state<any | null>(null);
  const activeReply = $derived(
    replyQuoteFor(
      messages,
      replyingTo?.id,
      replyAuthor,
      t("thread.replyUnavailable"),
    ),
  );

  function replyAuthor(message: any): string {
    return isUserMessage(message) ? "You" : message.sender_name || bot?.name || "";
  }

  function startReply(message: any) {
    if (!isPersistedMessageId(message?.id)) return;
    editingMessage = null;
    replyingTo = message;
    void tick().then(() => textareaRef?.focus());
  }

  function cancelReply() {
    replyingTo = null;
  }

  async function jumpToMessage(id: string) {
    await revealMessage(id);
  }

  async function focusMessageTarget(target: MessageFocusTarget) {
    if (target.botId !== bot.id) return;
    if (target.threadId !== selectedThreadId) {
      await loadMessages(target.threadId);
    }
    await tick();
    await revealMessage(target.messageId);
  }

  async function handleMessageFocus(event: Event) {
    const target = (event as CustomEvent<MessageFocusTarget>).detail;
    if (!target || target.botId !== bot.id) return;
    // Start (or join) this bot's load before claiming the hand-off, so the
    // normal "open the latest thread" path cannot run after the focus and
    // replace it.
    ensureBotLoaded(bot.id);
    if (initialLoad) await initialLoad;
    // The first consumer wins; a conversation that was still mounting took it
    // during its initial load.
    if (!takePendingMessageFocus(bot.id)) return;
    await focusMessageTarget(target);
  }

  /**
   * Load a conversation exactly once per bot. Both the mount path and the
   * bot-change effect call this, and the first caller owns the work.
   */
  function ensureBotLoaded(botId: string) {
    if (!botId || loadedBotId === botId) return;
    loadedBotId = botId;
    threads = [];
    selectedThreadId = null;
    messages = [];
    streamingText = "";
    streamingSources = [];
    streamingImages = [];
    const seq = ++loadSeq;
    invoke("get_session_usage", { botId })
      .then((res: any) => {
        if (seq === loadSeq) {
          sessionTokens = res?.tokens || 0;
          sessionCost = res?.cost || 0;
        }
      })
      .catch(console.error);
    initialLoad = (async () => {
      try {
        const result = (await invoke("list_threads", { botId })) as any[];
        // A later bot switch owns the conversation now.
        if (seq !== loadSeq) return;
        threads = result;
        const target = takePendingMessageFocus(botId);
        if (target) {
          await loadMessages(target.threadId);
          if (seq !== loadSeq) return;
          await tick();
          await revealMessage(target.messageId);
        } else if (threads.length > 0) {
          await loadMessages(threads[0].id);
        }
      } catch (e) {
        console.error("Failed to load threads:", e);
      } finally {
        if (seq === loadSeq) initialLoad = null;
      }
    })();
  }

  // Reactions are persisted before the UI settles, with an optimistic chip so
  // the click feels immediate. A failed write restores the previous list.
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
  let sending = $state(false);
  let showCostInfo = $state(false);
  let showIntelligence = $state(false);
  let showThreadDrawer = $state(false);
  let showThreadDropdown = $state(false);
  let showRoutines = $state(false);
  let showSync = $state(false);
  let showComputer = $state(false);
  let showChannels = $state(false);
  let showTeamImport = $state(false);
  // Active channel (context) for new threads, plus the list for the picker.
  let activeChannelId = $state<string | null>(null);
  let channelOptions = $state<{ id: string; name: string }[]>([]);

  async function loadChannelOptions() {
    try {
      const list = await invoke<any[]>("list_channels");
      channelOptions = list.map((c) => ({ id: c.id, name: c.name }));
      if (activeChannelId && !channelOptions.some((c) => c.id === activeChannelId)) {
        activeChannelId = null;
      }
    } catch {
      channelOptions = [];
    }
  }
  // Model quick switcher (per-conversation, Grok-style)
  // OpenMausBot-style: Cloud/Local provider rail + suggested list with
  // search + full list, dimmed with reason when a key is missing.
  let showModelSwitcher = $state(false);
  let switcherModel = $state("");
  let switcherSearch = $state("");
  let switcherRail = $state<string | null>(null);
  let switcherCatalog = $state<CatalogProvider[]>([]);
  let switcherConfigured = $state<string[]>([]);
  let switcherModels = $state<any[]>([]);
  let switcherLoading = $state(false);
  let switcherError = $state<string | null>(null);
  // Availability: global default model + local engine reachability (with reason)
  let switcherDefault = $state<{ provider: string; model: string } | null>(null);
  // Load the owner's global default once at mount so the header pill labels
  // honestly before the switcher has ever been opened.
  $effect(() => {
    invoke<any>("get_default_model")
      .then((def: any) => {
        if (def?.provider && !switcherDefault) switcherDefault = { provider: def.provider, model: def.model };
      })
      .catch(() => {});
  });
  let switcherOllamaOk = $state<boolean | null>(null);
  let switcherOllamaError = $state<string | null>(null);

  // ── Approval gate ("bots ask before they act") ──────────────────────────
  // Pending Allow/Deny cards. The composer blocks while any is pending; the
  // run parks server-side until decide_approval resolves it.
  interface PendingApproval {
    id: string; bot_id: string; thread_id: string; run_id: string;
    tool_name: string; tool_label: string; arguments: any; risk: string;
    status: string; created_at: string;
  }
  let pendingApprovals = $state<PendingApproval[]>([]);
  let decidingApproval = $state<string | null>(null);

  /**
   * Bot-scoped, not thread-scoped.
   *
   * A delegated child parks its approvals on its own thread, so a lookup
   * scoped to the thread on screen cannot find them — after a reload the run
   * would wait for a decision nothing could show or make, which reads as the
   * app having stopped working mid-run with no error anywhere.
   */
  async function refreshApprovals(botId: string) {
    try {
      pendingApprovals = await invoke<PendingApproval[]>("list_pending_approvals_for_bot", { botId });
    } catch {
      pendingApprovals = [];
    }
  }

  async function decideApproval(id: string, allowed: boolean) {
    if (decidingApproval) return;
    decidingApproval = id;
    try {
      await invoke("decide_approval", { approvalId: id, allowed, note: null });
      pendingApprovals = pendingApprovals.filter((a) => a.id !== id);
    } catch (e) {
      console.error("Failed to decide approval:", e);
    } finally {
      decidingApproval = null;
    }
  }


  // ── Human-in-the-loop questions ("ask_user") ────────────────────────────
  // The agent asks a question mid-run; the run parks until the user answers.
  interface PendingQuestion {
    id: string; bot_id: string; thread_id: string; run_id: string;
    header: string; question: string; options: string[]; allow_custom: boolean;
    status: string; created_at: string;
  }
  let pendingQuestions = $state<PendingQuestion[]>([]);
  let answeringQuestion = $state<string | null>(null);

  /** Bot-scoped for the same reason as `refreshApprovals` above. */
  async function refreshQuestions(botId: string) {
    try {
      pendingQuestions = await invoke<PendingQuestion[]>("list_pending_questions_for_bot", { botId });
    } catch {
      pendingQuestions = [];
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
      console.error("Failed to answer question:", e);
    } finally {
      answeringQuestion = null;
    }
  }

  // Inline image attachments (paste/drop) sent with the next message
  let pendingAttachments = $state<PendingAttachment[]>([]);
  let sessionTokens = $state(0);
  let sessionCost = $state(0.0);
  let userAvatar = $state<string | null>(null);
  let messagesContainer = $state<HTMLDivElement | null>(null);
  // OpenBot scroll behavior: auto-follow only while the reader is already at
  // the bottom (within 80px); scrolling up during a run unsticks and reveals
  // the jump-to-latest pill. `staticEntries` flags bulk history loads so row
  // entrances animate only live appends (see $lib/chat/entrance).
  let stickToLatest = $state(true);
  let staticEntries = $state(true);
  // How many messages arrived while the reader was away from the bottom.
  // The jump pill carries the number so "new" has a size, not just a state.
  let unseenMessages = $state(0);
  let seenMessageCount = 0;
  import { StreamReveal } from "$lib/chat/streamReveal.svelte";
  import { isGhostContent } from "$lib/chat/grouping";
  import { prefersReducedMotion, announce } from "$lib/a11y";
  import ChatActionMarker from "$lib/components/chat/ChatActionMarker.svelte";
  import { t } from "$lib/i18n";

  let textareaRef = $state<HTMLTextAreaElement | null>(null);

  let deepSearchActive = $state(false);
  let thinkActive = $state(false);
  let copiedMessageId = $state<string | null>(null);

  // Live streaming state (tokens arrive over the agent-stream event channel)
  let streamingText = $state("");
  // Word-by-word reveal decoupled from token arrival (OpenBot streaming feel)
  const reveal = new StreamReveal();
  $effect(() => reveal.track(streamingText));
  // OpenBot AgentActivity: a playful line is picked per run while no tokens show yet
  const ACTIVITY_LINES = [
    "activity.thinking",
    "activity.working",
    "activity.dots",
    "activity.fleet",
    "activity.gears",
  ] as const;
  let activityLine = $state<(typeof ACTIVITY_LINES)[number]>(ACTIVITY_LINES[0]);
  let wasSending = false;
  $effect(() => {
    if (sending && !wasSending) {
      activityLine = ACTIVITY_LINES[Math.floor(Math.random() * ACTIVITY_LINES.length)];
    }
    wasSending = sending;
  });
  let streamingTool = $state<string | null>(null);
  // OpenBot ChatActionMarker feed: one quiet mini-row per tool call in this run
  // `args` rides along so the live line can say what the call is doing rather
  // than only that a call with a given name happened.
  /**
   * Events the user has to act on, which must reach them regardless of which
   * thread the run happened to park on. See the gate in the stream listener.
   */
  const INTERACTION_KINDS = new Set([
    "approval_requested",
    "approval_decided",
    "question_asked",
    "question_answered",
  ]);

  // `args` rides along so the live line can say what the call is doing rather
  // than only that a call with a given name happened. `startedAt` / `ms` are
  // the other half of the same question: how long has this been going on, and
  // afterwards, how long did it take. Measured on this side rather than by the
  // marker, because the marker does not mount until the row renders and the
  // call may already have finished by then.
  let actionMarkers = $state<
    { id: number; name: string; done: boolean; args?: unknown; startedAt?: number; ms?: number }[]
  >([]);
  let markerSeq = 0;
  let streamingSources = $state<any[]>([]);
  // Live images produced by tools during the run (e.g. screenshots)
  let streamingImages = $state<{ name: string; data_url: string }[]>([]);
  // Which message is currently being read aloud (per-reply speaker button)
  let speakingMessageId = $state<string | null>(null);
  let regenerating = $state(false);
  let renamingThreadId = $state<string | null>(null);
  let renameValue = $state("");
  // Cross-thread message search
  let searchQuery = $state("");
  let searchResults = $state<any[]>([]);
  let searchPerformed = $state(false);

  async function runSearch() {
    const q = searchQuery.trim();
    if (!q) return;
    try {
      const hits = await invoke<any[]>("search_messages", { query: q, limit: 15 });
      // The command is fleet-wide; this drawer only browses the current bot.
      searchResults = hits.filter((hit) => !hit.bot_id || hit.bot_id === bot.id);
      searchPerformed = true;
    } catch (e) {
      console.error("Search failed:", e);
    }
  }
  let openArtifact = $state<Artifact | null>(null);
  // Temporary (ephemeral) chat mode: new threads skip agent-memory persistence
  let tempActive = $state(false);
  // Edit-and-resend: editing a prior user turn removes everything after it
  let editingMessage = $state<{ id: string; original: string } | null>(null);
  // Hands-free voice mode: STT → send → TTS loop
  let voiceMode = $state(false);
  let speaking = $state(false);
  let unlisten: UnlistenFn | null = null;

  async function openSource(url: string) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch (e) {
      console.error("Failed to open source:", e);
    }
  }

  function domainOf(url: string) {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  let botStatusTheme = $derived(getStatusTheme(bot?.status));
  let currentThread = $derived(threads.find((t) => t.id === selectedThreadId));

  $effect(() => {
    if (typeof localStorage !== "undefined") {
      userAvatar = localStorage.getItem("ravenbot_user_avatar");
      const h = () => (userAvatar = localStorage.getItem("ravenbot_user_avatar"));
      window.addEventListener("user-avatar-changed", h);
      return () => window.removeEventListener("user-avatar-changed", h);
    }
  });

  function handleTryLocalSkill(event: Event) {
    const prompt = (event as CustomEvent<{ prompt?: string }>).detail?.prompt;
    if (!prompt) return;
    newMessage = prompt;
    void tick().then(() => textareaRef?.focus());
  }

  onMount(async () => {
    window.addEventListener("local-skill-try", handleTryLocalSkill);
    window.addEventListener(MESSAGE_FOCUS_EVENT, handleMessageFocus);
    ensureBotLoaded(bot.id);
    await initialLoad;

    // Live token streaming from the runtime
    try {
      unlisten = await listen<any>("agent-stream", (event) => {
        const payload = event.payload;
        const tid = payload?.thread_id;
        const kind = String(payload?.kind || "");

        // Interactions are addressed to whoever is waiting on the run, and that
        // is not always the thread being read.
        //
        // A delegated child runs on its own thread, and its tool approvals park
        // on *that* thread. The blanket `tid !== selectedThreadId` below dropped
        // them, so the card never appeared: the child waited for a decision
        // nobody could see, the parent waited on the child, and the whole thing
        // sat there looking dead. That is precisely what "stops working mid run"
        // looks like from the outside — no error, no spinner, nothing.
        //
        // So the gate applies to what the thread *displays* (tokens, tools,
        // round boundaries, status) and not to what the user must *answer*.
        if (!tid || (tid !== selectedThreadId && !INTERACTION_KINDS.has(kind))) return;
        switch (payload?.kind) {
          case "run_started":
            activeRunId = payload.run_id ?? null;
            break;
          case "paused":
            // Keep activeRunId/pausedRunId so the Play button can resume it.
            if (payload?.run_id) {
              activeRunId = payload.run_id;
              pausedRunId = payload.run_id;
            }
            if (payload?.bot_id === bot.id) {
              onBotUpdated?.({ ...bot, status: "paused" });
            }
            break;
          case "delta":
            streamingText += payload.content || "";
            scrollToBottom();
            break;
          case "reasoning":
            // Its own channel, and `clear` below does not touch it. That is the
            // whole point: the answer's buffer is legitimately reset between model
            // rounds, and the reasoning is not.
            if (reasoning.ingest(payload)) scrollToBottom();
            break;
          case "clear":
            streamingText = "";
            // Close the current stretch rather than discarding it. Reasoning used
            // to ride the text channel, so this line is where the entire trace of
            // every tool-using turn went to die.
            reasoning.endRound(String(payload?.thread_id || tid || ""));
            break;
          case "tool_started":
            streamingTool = payload.name;
            actionMarkers = [
              ...actionMarkers,
              {
                id: markerSeq++,
                name: payload.name || "tool",
                done: false,
                args: payload.arguments ?? null,
                startedAt: Date.now(),
              },
            ];
            scrollToBottom();
            break;
          case "tool_finished": {
            streamingTool = null;
            const nm = payload.name || "tool";
            let flipped = false;
            // Newest open marker with this name: two calls to the same skill
            // in one round are matched to the round they belong to, or the
            // elapsed time lands on the wrong row.
            for (let i = actionMarkers.length - 1; i >= 0; i--) {
              if (!actionMarkers[i].done && actionMarkers[i].name === nm) {
                const finished = actionMarkers[i];
                actionMarkers[i] = {
                  ...finished,
                  done: true,
                  ms: finished.startedAt != null ? Date.now() - finished.startedAt : undefined,
                };
                flipped = true;
                break;
              }
            }
            if (!flipped) {
              // Completed without a start we saw — the listener attached
              // mid-run. Showing no time is right: this one's start is not
              // something we know, and guessing would be a worse answer than
              // none.
              actionMarkers = [...actionMarkers, { id: markerSeq++, name: nm, done: true }];
            }
            actionMarkers = actionMarkers.slice();
            break;
          }
          case "sources":
            for (const src of payload?.sources || []) {
              if (src?.url && !streamingSources.some((s) => s.url === src.url)) {
                streamingSources = [...streamingSources, src];
              }
            }
            break;
          case "image":
            if (payload?.data_url) {
              streamingImages = [
                ...streamingImages,
                { name: payload.name || "image", data_url: payload.data_url },
              ];
              scrollToBottom();
            }
            break;
          case "done": {
            // OpenBot: snap the reveal sharp-full and HOLD the streamed text
            // until sendMessage's finally reloads history — clearing here is
            // what flashed the activity shimmer between `done` and the commit.
            reveal.finish();
            // Close the live stretch. The run is over, so nothing is being
            // thought — and a stream that ends without a final `clear` would
            // otherwise leave the pulse running forever on a trace that stopped
            // arriving minutes ago.
            const doneTid = String(payload?.thread_id || selectedThreadId || "");
            if (doneTid) reasoning.finish(doneTid);
            streamingTool = null;
            activeRunId = null;
            pausedRunId = null;
            if (selectedThreadId) {
              if (bot) {
                refreshApprovals(bot.id);
                refreshQuestions(bot.id);
              }
            }
            break;
          }
          case "status":
            // Live status ring (thinking / running_tool / done → idle)
            if (payload?.bot_id === bot.id) {
              const nextStatus = payload.state === "done" ? "idle" : payload.state;
              if (bot.status !== nextStatus) {
                onBotUpdated?.({ ...bot, status: nextStatus });
                if (nextStatus === "emulating_tools" || nextStatus === "waiting_on_user" || nextStatus === "paused") {
                  announce(getStatusTheme(nextStatus).label);
                }
              }
            }
            break;
          case "usage":
            // Real telemetry from the completed run
            if (payload?.thread_id === selectedThreadId) {
              sessionTokens += payload.tokens || 0;
              sessionCost += payload.cost || 0;
            }
            break;
          case "context":
            contextUsage.ingest(payload);
            break;
          case "approval_requested":
            // A tool parked for a decision: card appears, composer blocks.
            if (payload?.approval && !pendingApprovals.some((a) => a.id === payload.approval.id)) {
              pendingApprovals = [...pendingApprovals, payload.approval];
              announce("Tool approval needed");
              scrollToBottom();
            }
            break;
          case "approval_decided":
            // Card flips to its settled state.
            if (payload?.approval_id) {
              pendingApprovals = pendingApprovals.filter((a) => a.id !== payload.approval_id);
            }
            break;
          case "question_asked":
            // The agent parked to ask a question: show the answer card.
            if (payload?.question && !pendingQuestions.some((q) => q.id === payload.question.id)) {
              pendingQuestions = [...pendingQuestions, payload.question];
              scrollToBottom();
            }
            break;
          case "question_answered":
            if (payload?.question_id) {
              pendingQuestions = pendingQuestions.filter((q) => q.id !== payload.question_id);
            }
            break;
        }
      });
    } catch (e) {
      console.error("Failed to attach stream listener:", e);
    }
  });

  onDestroy(() => {
    unlisten?.();
    unlisten = null;
    window.removeEventListener("local-skill-try", handleTryLocalSkill);
    window.removeEventListener(MESSAGE_FOCUS_EVENT, handleMessageFocus);
    if (threadSearchTimer) clearTimeout(threadSearchTimer);
    reveal.stop();
    stopVoiceMode();
  });

  $effect(() => {
    loadChannelOptions();
  });

  // Load the bot's threads only when we switch to a DIFFERENT bot — never on
  // a mere status/telemetry update. `selectedBot` is `$derived(bots.find(…))`,
  // so every status event hands us a brand-new object; without this guard the
  // effect re-ran mid-run and reset the conversation.
  $effect(() => {
    const botId = bot?.id;
    if (botId) ensureBotLoaded(botId);
  });

  // Play base64 audio (WAV) via the Web Audio API.
  function playAudioBase64(b64: string) {
    try {
      const binary = atob(b64);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      const blob = new Blob([bytes], { type: "audio/wav" });
      const url = URL.createObjectURL(blob);
      const audio = new Audio(url);
      audio.play().catch((e) => console.error("Audio play failed:", e));
      audio.onended = () => URL.revokeObjectURL(url);
    } catch (e) {
      console.error("Failed to play audio:", e);
    }
  }

  function handleFeedScroll() {
    const el = messagesContainer;
    if (!el) return;
    const nearTop = el.scrollTop <= 120;
    if (windowEnd !== 0) {
      // The bottom of a pinned window is not the bottom of the transcript.
      stickToLatest = false;
      if (nearTop && hiddenBefore > 0) void loadEarlier();
      return;
    }
    stickToLatest = el.scrollHeight - el.scrollTop - el.clientHeight <= 80;
    if (stickToLatest) unseenMessages = 0;
    if (nearTop && hiddenBefore > 0) void loadEarlier();
  }

  function jumpToLatest() {
    stickToLatest = true;
    unseenMessages = 0;
    windowEnd = 0;
    messageWindow = MESSAGE_WINDOW;
    messagesContainer?.scrollTo({
      top: messagesContainer.scrollHeight,
      behavior: prefersReducedMotion() ? "auto" : "smooth",
    });
  }

  function handleMessageNavKey(event: KeyboardEvent) {
    if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    if (isEditableTarget(event.target)) return;
    event.preventDefault();
    navigateMessage(
      messagesContainer,
      event.key === "ArrowUp" ? -1 : 1,
      !prefersReducedMotion(),
    );
  }

  // Count committed messages that arrive while the reader is scrolled away.
  // Bulk history loads set `stickToLatest` first, so a thread opening does not
  // report its whole transcript as new.
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

  // Tokens follow the feed only while the reader is stuck to the bottom;
  // explicit actions (send, thread open) force back to stuck. The visible text
  // is revealed on a timer, so following the raw delta alone scrolls before
  // the words exist and the answer grows below the fold.
  $effect(() => {
    const shown = reveal.shown;
    if (shown && stickToLatest) void scrollToBottom();
  });

  async function scrollToBottom(force = false) {
    if (!force && !stickToLatest) return;
    stickToLatest = true;
    windowEnd = 0;
    // Returning to the live end also releases any earlier batches the reader
    // loaded while paging through history.
    messageWindow = MESSAGE_WINDOW;
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }

  async function loadMessages(threadId: string) {
    if (threadId !== selectedThreadId) replyingTo = null;
    stickToLatest = true;
    unseenMessages = 0;
    seenMessageCount = 0;
    messageWindow = MESSAGE_WINDOW;
    windowEnd = 0;
    selectedThreadId = threadId;
    showThreadDropdown = false;
    staticEntries = true;
    streamingText = "";
    streamingTool = null;
    actionMarkers = [];
    streamingSources = []; streamingImages = [];
    if (bot) {
      refreshApprovals(bot.id);
      refreshQuestions(bot.id);
    }
    editingMessage = null;
    try {
      messages = await invoke("list_messages", { threadId });
      await tick();
      staticEntries = false;
      scrollToBottom(true);
    } catch (e) {
      staticEntries = false;
      console.error("Failed to load messages:", e);
    }
  }

  async function createNewThread() {
    try {
      const thread = await invoke("create_thread", {
        botId: bot.id,
        title: `Thread #${threads.length + 1}`,
        ephemeral: tempActive,
        channelId: activeChannelId,
      });
      threads = [thread, ...threads];
      selectedThreadId = (thread as any).id;
      messages = [];
      replyingTo = null;
      showThreadDropdown = false;
      await tick();
      textareaRef?.focus();
    } catch (e) {
      console.error("Failed to create thread:", e);
    }
  }

  function triggerOpenSettings() {
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("open-settings"));
    }
  }

  // Active run id (from run_started) and the run we paused (for resume).
  let activeRunId = $state<string | null>(null);
  let pausedRunId = $state<string | null>(null);

  async function togglePause() {
    try {
      if (bot.status === "paused") {
        // Resume this thread's run from its checkpoint when we paused it,
        // otherwise release the global kill switch.
        if (pausedRunId) {
          await invoke("resume_run", { runId: pausedRunId });
          pausedRunId = null;
          if (selectedThreadId) await loadMessages(selectedThreadId);
        } else {
          await invoke("resume_all");
        }
        onBotUpdated?.({ ...bot, status: "idle" });
      } else {
        if (activeRunId) {
          await invoke("pause_run", { runId: activeRunId });
          pausedRunId = activeRunId;
        } else {
          await invoke("pause_all");
        }
        onBotUpdated?.({ ...bot, status: "paused" });
      }
    } catch (e) {
      console.error("Failed to toggle pause:", e);
    }
  }

  async function renameThread(threadId: string) {
    if (renamingThreadId === threadId) {
      // Second click: commit
      const title = renameValue.trim();
      if (title) {
        try {
          await invoke("rename_thread", { threadId, title });
          threads = threads.map((t) => (t.id === threadId ? { ...t, title } : t));
        } catch (e) {
          console.error("Failed to rename thread:", e);
        }
      }
      renamingThreadId = null;
    } else {
      renamingThreadId = threadId;
      renameValue = threads.find((t) => t.id === threadId)?.title || "";
    }
  }

  let deleteArmed = $state<string | null>(null);

  async function deleteThread(threadId: string) {
    if (deleteArmed !== threadId) {
      // First click: arm (two-step confirm, no native confirm() in webview)
      deleteArmed = threadId;
      setTimeout(() => {
        if (deleteArmed === threadId) deleteArmed = null;
      }, 3000);
      return;
    }
    deleteArmed = null;
    try {
      await invoke("delete_thread", { threadId });
      threads = threads.filter((t) => t.id !== threadId);
      if (selectedThreadId === threadId) {
        selectedThreadId = null;
        messages = [];
        if (threads.length > 0) {
          await loadMessages(threads[0].id);
        }
      }
    } catch (e) {
      console.error("Failed to delete thread:", e);
    }
  }

  async function sendMessage(textToSend?: string) {
    const typed = (textToSend || newMessage).trim();
    const hasAttachments = pendingAttachments.length > 0;
    if (sending || (!typed && !hasAttachments)) return;
    const rawText = typed || "(shared files)";
    const replyToId = replyingTo?.id ?? null;

    // Apply DeepSearch or Think prefixes if toggled
    let text = rawText;
    if (deepSearchActive && !text.startsWith("[DeepSearch]")) {
      text = `[DeepSearch] ${text}`;
    }
    if (thinkActive && !text.startsWith("[Think]")) {
      text = `[Think] ${text}`;
    }

    sending = true;
    newMessage = "";
    actionMarkers = [];
    if (textareaRef) {
      textareaRef.style.height = "auto";
    }

    try {
      if (!selectedThreadId) {
        const thread = await invoke("create_thread", {
          botId: bot.id,
          title: rawText.slice(0, 35) + (rawText.length > 35 ? "..." : ""),
          ephemeral: tempActive,
          channelId: activeChannelId,
        });
        threads = [thread, ...threads];
        selectedThreadId = (thread as any).id;
      }

      // Optimistically show user message right away so it is NEVER lost
      const tempAttachments = pendingAttachments.map((p) => ({
        id: "temp-att-" + Date.now() + "-" + p.name,
        name: p.name,
        mime_type: p.mime,
        size: p.data.length,
        path: "",
        data: p.data,
        is_image: p.isImage,
      }));
      const tempUserMsg = {
        id: "temp-" + Date.now(),
        thread_id: selectedThreadId,
        role: "user",
        content: text,
        attachments: tempAttachments,
        reply_to_id: replyToId,
        reactions: [],
        created_at: new Date().toISOString(),
      };
      messages = [...messages, tempUserMsg];
      // An explicit send always returns the feed to the bottom (OpenBot).
      scrollToBottom(true);

      if (editingMessage) {
        // Edit-and-resend: backend removes this turn + everything after it
        await invoke("edit_and_resend", {
          threadId: selectedThreadId,
          messageId: editingMessage.id,
          content: text,
          attachments: pendingAttachments.length ? pendingAttachments : undefined,
        });
        editingMessage = null;
      } else {
        await invoke("send_message", {
          threadId: selectedThreadId,
          content: text,
          attachments: pendingAttachments.length ? pendingAttachments : undefined,
          replyToId,
        });
      }
      replyingTo = null;
      pendingAttachments = [];

      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
    } catch (e: any) {
      console.error("Failed to send message:", e);
      notify(`Message not sent: ${String(e)}`, "error");
      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
    } finally {
      sending = false;
      streamingText = "";
      streamingTool = null;
      streamingSources = []; streamingImages = [];
      editingMessage = null;
      scrollToBottom();

      // Voice mode: speak the response, then resume hands-free listening.
      // Otherwise, honour the bot's auto-read setting.
      const last = messages[messages.length - 1];
      const lastText = last
        ? (typeof last.content === "string"
          ? last.content
          : last.content?.text || "")
        : "";
      if (voiceMode) {
        if (lastText) {
          speakForVoice(lastText);
        } else {
          startVoiceLoop();
        }
      } else if (lastText) {
        autoReadIfEnabled(lastText);
      }
    }
  }

  function copyMessage(id: string, text: string) {
    navigator.clipboard.writeText(text).then(() => {
      copiedMessageId = id;
      setTimeout(() => {
        if (copiedMessageId === id) copiedMessageId = null;
      }, 2000);
    });
  }

  function startEditing(id: string, text: string) {
    if (sending || regenerating) return;
    replyingTo = null;
    editingMessage = { id, original: text };
    newMessage = text;
    textareaRef?.focus();
  }

  function cancelEditing() {
    editingMessage = null;
    newMessage = "";
    if (textareaRef) textareaRef.style.height = "auto";
  }

  // ——— Quick switcher: catalog rail + live models ———
  let switcherRailProviders = $derived(switcherCatalog);
  let switcherRailProvider = $derived(
    providerById(switcherCatalog, switcherRail ?? bot?.config?.model_provider ?? "")
    ?? switcherCatalog[0]
  );
  let switcherAvailable = $derived(
    modelsFor(switcherRailProvider, switcherModels, switcherLoading)
  );
  let switcherVisible = $derived(
    switcherSearch.trim()
      ? switcherAvailable.filter((m) =>
          m.id.toLowerCase().includes(switcherSearch.toLowerCase()) ||
          m.name.toLowerCase().includes(switcherSearch.toLowerCase()),
        )
      : switcherAvailable.slice(0, 8)
  );
  let switcherShowingAll = $derived(
    switcherSearch.trim() ? true : switcherAvailable.length <= 8
  );

  function openModelSwitcher() {
    switcherModel = bot?.config?.model_id || "";
    switcherSearch = "";
    switcherRail = bot?.config?.model_provider || null;
    showModelSwitcher = !showModelSwitcher;
    if (showModelSwitcher) refreshSwitcher();
  }

  async function refreshSwitcher() {
    try {
      switcherCatalog = await getCatalog();
    } catch { /* minimal fallback cached in module */ }
    try {
      switcherConfigured = await invoke<string[]>("get_configured_providers");
    } catch { switcherConfigured = []; }
    try {
      const def: any = await invoke("get_default_model");
      switcherDefault = def?.provider ? { provider: def.provider, model: def.model } : null;
    } catch { switcherDefault = null; }
    // Local engine reachability (so "ollama" shows an honest reason when down)
    switcherOllamaOk = null;
    switcherOllamaError = null;
    invoke("probe_ollama")
      .then(() => { switcherOllamaOk = true; switcherOllamaError = null; })
      .catch((e: any) => { switcherOllamaOk = false; switcherOllamaError = String(e); });
    const rail = switcherRail ?? bot?.config?.model_provider;
    if (rail) loadSwitcherModels(rail);
  }

  /// Why a provider can't be used right now (null = available).
  function switcherRailReason(p: CatalogProvider): string | null {
    const isLocal = p.keyless || p.id === "local" || p.id === "ollama";
    if (isLocal) {
      if (p.id === "ollama" && switcherOllamaOk === false) return "Ollama not reachable";
      return null;
    }
    if (!switcherConfigured.includes(p.id)) return "API key needed";
    return null;
  }

  /// Short badge text for a rail entry.
  function switcherRailBadge(p: CatalogProvider): string | null {
    const reason = switcherRailReason(p);
    if (!reason) return null;
    if (p.id === "ollama") return switcherOllamaOk === null ? "…" : "offline";
    return "key?";
  }

  async function loadSwitcherModels(provider: string) {
    switcherLoading = true;
    switcherError = null;
    try {
      switcherModels = (await invoke<any[]>("fetch_provider_models", { provider })) || [];
    } catch (e: any) {
      switcherError = String(e);
      switcherModels = [];
    } finally {
      switcherLoading = false;
    }
  }

  function selectSwitcherRail(id: string) {
    switcherRail = id;
    switcherSearch = "";
    loadSwitcherModels(id);
  }

  async function switchProvider(provider: string, model: string) {
    if (!bot?.config || sending) return;
    try {
      const updated = {
        ...bot,
        config: { ...bot.config, model_provider: provider, model_id: model },
      };
      await invoke("update_bot", { bot: updated });
      showModelSwitcher = false;
      onBotUpdated?.(updated);
    } catch (e) {
      console.error("Failed to switch provider:", e);
    }
  }

  async function applySwitcherModel() {
    const model = switcherModel.trim();
    if (!model || !bot?.config || sending) return;
    try {
      const updated = {
        ...bot,
        config: { ...bot.config, model_id: model },
      };
      await invoke("update_bot", { bot: updated });
      showModelSwitcher = false;
      onBotUpdated?.(updated);
    } catch (e) {
      console.error("Failed to apply model:", e);
    }
  }

  async function regenerate() {
    if (!selectedThreadId || sending || regenerating) return;
    regenerating = true;
    streamingText = "";
    streamingSources = []; streamingImages = [];
    try {
      await invoke("regenerate_message", { threadId: selectedThreadId });
      await loadMessages(selectedThreadId);
    } catch (e) {
      console.error("Failed to regenerate:", e);
      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
    } finally {
      regenerating = false;
      streamingText = "";
      // `done` no longer clears the live side-buffers (it holds them until
      // commit); this is where a regenerate run drops them.
      streamingSources = []; streamingImages = [];
      scrollToBottom();
      if (!voiceMode) {
        const last = messages[messages.length - 1];
        const lastText = last
          ? (typeof last.content === "string" ? last.content : last.content?.text || "")
          : "";
        if (lastText) autoReadIfEnabled(lastText);
      }
    }
  }

  function handleTextareaInput(e: Event) {
    const target = e.target as HTMLTextAreaElement;
    target.style.height = "auto";
    target.style.height = Math.min(target.scrollHeight, 160) + "px";
  }

  // ---- Inline image attachments (paste / drag-drop) ----

  // File classification/reading lives in the shared attachments module.

  async function attachFiles(files: FileList | File[]) {
    for (const file of Array.from(files)) {
      if (file.size > MAX_ATTACHMENT_BYTES) {
        notify(`Attachment is larger than ${MAX_ATTACHMENT_BYTES / (1024 * 1024)} MB.`, "error");
        continue;
      }
      const mime = (file.type || "").toLowerCase();
      const ext = (file.name?.split(".").pop() || "").toLowerCase();

      // Images ride as inline attachments (vision).
      if (ACCEPTED_IMAGE_MIMES.includes(mime)) {
        const dataUrl = await readAsDataUrl(file);
        const base64 = dataUrl.split(",")[1] || "";
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

      // Text-like files insert their content into the composer so the model
      // receives them directly (no size-risky binary transport).
      if (isTextFile(file)) {
        const text = await readAsText(file);
        if (!text) continue;
        newMessage = (newMessage ? newMessage + "\n\n" : "") +
          `Attached file [${file.name}]:\n\`\`\`${ext}\n${text}\n\`\`\`\n`;
        continue;
      }

      // Everything else (PDF, DOCX, XLSX, archives, binaries) is attached as a
      // document: the bytes are preserved and the model is told the file
      // exists, instead of the file being silently dropped.
      const dataUrl = await readAsDataUrl(file);
      const base64 = dataUrl.split(",")[1] || "";
      if (!base64) continue;
      pendingAttachments = [...pendingAttachments, {
        name: file.name || "attachment.bin",
        mime: mime || "application/octet-stream",
        data: base64,
        isImage: false,
      }];
    }
  }

  function handleComposerPaste(e: ClipboardEvent) {
    if (e.clipboardData?.files?.length) {
      e.preventDefault();
      attachFiles(e.clipboardData.files);
    }
  }

  function handleComposerDrop(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer?.files?.length) {
      attachFiles(e.dataTransfer.files);
    }
  }

  function removePendingAttachment(idx: number) {
    pendingAttachments = pendingAttachments.filter((_, i) => i !== idx);
  }

  function getStatusTheme(status: string) {
    switch (status) {
      case "idle":
        return { dot: "bg-success", text: "text-success", label: "Ready" };
      case "thinking":
        return { dot: "bg-warning animate-pulse", text: "text-warning", label: "Reasoning…" };
      case "running_tool":
        return { dot: "bg-[var(--status-running)] animate-pulse", text: "text-[var(--status-running)]", label: "Running Tool…" };
      case "emulating_tools":
        return { dot: "bg-[var(--brand)] animate-pulse", text: "text-[var(--brand-text)]", label: t("runtime.emulatingTools") };
      case "waiting_on_user":
        return { dot: "bg-danger", text: "text-danger", label: "Waiting on input" };
      case "paused":
        return { dot: "bg-[var(--status-paused)]", text: "text-[var(--status-paused)]", label: "Paused" };
      default:
        return { dot: "bg-success", text: "text-[var(--text-tertiary)]", label: status || "Ready" };
    }
  }

  function getModelDisplayName(b: any) {
    // No invented fallbacks: show what's configured, else the owner's global
    // default (get_default_model), else an honest "?".
    const p = b?.config?.model_provider || switcherDefault?.provider || "?";
    const m = b?.config?.model_id || switcherDefault?.model || "?";
    return `${p}/${String(m).split("/").pop()}`;
  }

  function currentModelFacts(b: any): string {
    if (!b?.config?.model_provider || !b?.config?.model_id) return "";
    return modelSummary(modelMetaFor(b.config.model_provider, b.config.model_id));
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
  // Handles to stop an in-flight recording / playback cleanly.
  let activeRecorder: UtteranceRecorder | null = null;
  let activeSpeech: SpeechHandle | null = null;

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

  // ---- Voice: real backend STT / TTS (shared module) ----
  // Recording → backend transcribe (Whisper / faster-whisper); playback →
  // backend synthesize. Works on Linux WebKitGTK + macOS and reports honest
  // errors when no engine is configured.

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

  // ---- Hands-free Voice Mode (record → transcribe → send → speak → repeat) ----

  let voiceListening = false;

  function toggleVoiceMode() {
    if (voiceMode) {
      stopVoiceMode();
      return;
    }
    if (!navigator.mediaDevices?.getUserMedia) {
      alert("Voice mode needs microphone access, which is not available in this environment.");
      return;
    }
    voiceMode = true;
    startVoiceLoop();
  }

  function stopVoiceMode() {
    voiceMode = false;
    voiceListening = false;
    activeRecorder?.stop();
    activeSpeech?.stop();
    activeSpeech = null;
    speaking = false;
  }

  async function startVoiceLoop() {
    if (!voiceMode || voiceListening) return;
    voiceListening = true;
    try {
      const rec = await recordUtterance({ maxMs: 25000, silenceMs: 1500 });
      activeRecorder = rec;
      const blob = await rec.result;
      activeRecorder = null;
      if (!voiceMode) return;
      if (!blob) {
        voiceListening = false;
        window.setTimeout(startVoiceLoop, 400);
        return;
      }
      const text = await transcribeBlob(blob);
      voiceListening = false;
      if (!voiceMode) return;
      if (!text) {
        window.setTimeout(startVoiceLoop, 400);
        return;
      }
      await sendMessage(text);
      // sendMessage's finally calls speakForVoice(), which resumes the loop.
      // Safety net in case it returned early (e.g. a send was already running).
      window.setTimeout(() => {
        if (voiceMode && !speaking && !voiceListening) startVoiceLoop();
      }, 800);
    } catch (e: any) {
      console.error("Voice mode error:", e);
      voiceMode = false;
      voiceListening = false;
      alert(voiceErrorMessage(e));
    }
  }

  /// Toggle auto-read for this bot (persisted via update_bot).
  async function toggleAutoRead() {
    if (!bot?.config) return;
    const updated = {
      ...bot,
      config: { ...bot.config, auto_read: !bot.config.auto_read },
    };
    try {
      await invoke("update_bot", { bot: updated });
      onBotUpdated?.(updated);
    } catch (e) {
      console.error("Failed to toggle auto-read:", e);
    }
  }

  /// Speak a specific message (per-reply speaker button). Toggles off.
  async function speakMessage(id: string, text: string) {
    if (speakingMessageId === id) {
      activeSpeech?.stop();
      activeSpeech = null;
      speakingMessageId = null;
      speaking = false;
      return;
    }
    const clean = stripForSpeech(text);
    if (!clean) return;
    activeSpeech?.stop();
    speakingMessageId = id;
    speaking = true;
    try {
      const handle = await speakText(clean, bot?.config?.voice_id || undefined);
      activeSpeech = handle;
      await handle.done;
    } catch (e) {
      console.error("Speak failed:", e);
    } finally {
      if (speakingMessageId === id) speakingMessageId = null;
      activeSpeech = null;
      speaking = false;
    }
  }

  /// Read a reply aloud automatically when the bot has auto-read enabled.
  async function autoReadIfEnabled(text: string) {
    if (!bot?.config?.auto_read) return;
    const clean = stripForSpeech(text);
    if (!clean) return;
    try {
      activeSpeech?.stop();
      speaking = true;
      const handle = await speakText(clean, bot?.config?.voice_id || undefined);
      activeSpeech = handle;
      await handle.done;
    } catch (e) {
      console.error("Auto-read failed:", e);
    } finally {
      activeSpeech = null;
      speaking = false;
    }
  }

  async function speakForVoice(md: string) {
    if (!voiceMode) return;
    const clean = stripForSpeech(md);
    if (!clean) {
      startVoiceLoop();
      return;
    }
    speaking = true;
    try {
      const handle = await speakText(clean, bot?.config?.voice_id || undefined);
      activeSpeech = handle;
      if (!voiceMode) {
        handle.stop();
        return;
      }
      await handle.done;
    } catch (e) {
      console.error("TTS failed:", e);
    } finally {
      activeSpeech = null;
      speaking = false;
      if (voiceMode) startVoiceLoop();
    }
  }
</script>

<svelte:window
  onclick={() => (showThreadDropdown = false, showModelSwitcher = false)}
  onkeydown={handleMessageNavKey}
/>

{#snippet sourcesChips(sources: any[])}
  {#if sources && sources.length > 0}
    <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2.5 space-y-1.5">
      <div class="flex items-center gap-1.5 text-[11px] font-mono text-[var(--text-tertiary)] uppercase tracking-wider">
        <Globe class="size-3 text-[var(--brand-text)]" />
        <span>Sources ({sources.length})</span>
      </div>
      <div class="flex flex-wrap gap-1.5">
        {#each sources as src, idx}
          {@const label = domainOf(src.url)}
          <button
            type="button"
            class="max-w-[220px] h-6 px-2 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] hover:bg-[var(--brand-soft)] hover:border-[var(--brand)]/40 flex items-center gap-1.5 text-[11px] text-[var(--text-secondary)] hover:text-[var(--brand-hover)] transition-colors cursor-pointer"
            title={src.title || src.url}
            onclick={() => openSource(src.url)}
          >
            <span class="size-3.5 rounded bg-[var(--brand-soft)] text-[var(--brand-text)] font-mono flex items-center justify-center text-[11px] shrink-0">{idx + 1}</span>
            <span class="truncate font-mono">{label}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
{/snippet}

<div class="flex flex-col h-full overflow-hidden select-none bg-[var(--surface-0)] text-[var(--text-primary)] font-sans relative">
  <!-- Sleek Top Header Bar (Grok Style) -->
  <header class="h-[46px] px-4 border-b border-[var(--hairline)] bg-[var(--surface-0)] flex items-center justify-between gap-3 z-20 shrink-0">
    <!-- Left: Bot Avatar & Info + Thread Switcher Dropdown -->
    <div class="flex items-center gap-3 min-w-0 flex-1">
      <div class="relative size-8 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline)] p-0.5 shrink-0 shadow-sm">
        <RavenAvatar name={bot.name} imageUrl={bot.avatar_url} style={bot.avatar_style} />
        <span class="absolute bottom-0 right-0 size-2 rounded-full ring-1 ring-[var(--surface-0)] {botStatusTheme.dot}"></span>
      </div>

      <div class="flex items-center gap-2 min-w-0">
        <span class="font-bold text-sm text-[var(--text-primary)] truncate">{bot.name}</span>

        {#if bot?.config?.engine && bot.config.engine !== "native"}
          <span
            class="text-[11px] font-mono py-0.5 px-2 rounded-md bg-[var(--brand-soft)] border border-[var(--brand)]/30 text-[var(--brand-text)] hidden sm:inline-flex items-center gap-1"
            title={t("thread.cliBot")}
          >
            <Cpu class="size-2.5" /> {bot.config.engine}
          </span>
        {/if}

        <!-- Model Quick Switcher (clickable pill, Grok-style) -->
        <div class="relative">
          <button
            type="button"
            class="text-[11px] font-mono py-0.5 px-2 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] truncate cursor-pointer transition-colors hidden sm:inline-flex items-center gap-1 max-w-[220px] {bot?.config?.engine && bot.config.engine !== 'native' ? 'opacity-40' : ''}"
            onclick={(e) => {
              e.stopPropagation();
              openModelSwitcher();
            }}
            aria-expanded={showModelSwitcher}
            aria-haspopup="dialog"
            title={`${t("thread.switchModel")}${currentModelFacts(bot) ? " · " + currentModelFacts(bot) : ""}`}
          >
            <span class="truncate">{getModelDisplayName(bot)}</span>
            <ChevronDown class="size-2.5 shrink-0" />
          </button>

          {#if showModelSwitcher}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="absolute left-0 top-7 z-50 w-[26rem] max-w-[calc(100vw-2rem)] bg-[var(--surface-1)] border border-[var(--hairline)] rounded-2xl shadow-2xl animate-in fade-in zoom-in-95  overflow-hidden"
              onclick={(e) => e.stopPropagation()}
            >
              <span class="block px-3 py-2 text-[11px] font-bold text-[var(--text-tertiary)] uppercase tracking-wider font-mono border-b border-[var(--hairline)]">
                Switch Model · {bot?.config?.model_provider}/{bot?.config?.model_id?.split("/").pop()}
              </span>

              <div class="px-2.5 pt-2 pb-2">
                <ModelPicker
                  providers={switcherRailProviders}
                  provider={switcherRailProvider?.id ?? ""}
                  model={bot?.config?.model_id ?? ""}
                  models={switcherAvailable}
                  configured={switcherConfigured}
                  loading={switcherLoading}
                  error={switcherRailProvider && switcherRailReason(switcherRailProvider)
                    ? `${switcherRailReason(switcherRailProvider)} — fallbacks shown.`
                    : (switcherError && switcherAvailable.length === 0
                        ? "Live list failed — catalog fallbacks shown."
                        : null)}
                  reasonFor={(p) => switcherRailReason(p)}
                  onSelectProvider={selectSwitcherRail}
                  onSelectModel={(id) => switcherRailProvider && switchProvider(switcherRailProvider.id, id)}
                  heightClass="max-h-[18rem]"
                  footerNote={switcherShowingAll
                    ? undefined
                    : `Showing suggested · search for all ${switcherAvailable.length}`}
                />
              </div>

              <!-- Custom model id -->
              <div class="flex items-center gap-1.5 pt-1 border-t border-[var(--hairline)] mt-1">
                <input
                  bind:value={switcherModel}
                  aria-label="Custom model id"
                  placeholder="model id…"
                  class="flex-1 min-w-0 h-6 px-2 rounded-lg bg-[var(--surface-0)] border border-[var(--hairline)] text-[11px] font-mono text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 focus:border-[var(--brand)]/50"
                  onkeydown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      e.stopPropagation();
                      applySwitcherModel();
                    }
                  }}
                />
                <Button
                  size="sm"
                  class="h-6 px-2 text-[11px] bg-[var(--surface-light)] text-[var(--text-on-light)] hover:bg-[var(--surface-light-hover)] cursor-pointer shrink-0"
                  onclick={applySwitcherModel}
                >
                  Apply
                </Button>
              </div>
            </div>
          {/if}
        </div>

        {#if currentThread?.ephemeral}
          <span class="text-[11px] font-mono py-0.5 px-2 rounded-md bg-warning/15 border border-warning/30 text-warning flex items-center gap-1 shrink-0" title={t("thread.tempTip")}>
            <Ghost class="size-3" />
            <span>{t("thread.temp")}</span>
          </span>
        {/if}
      </div>

      <!-- Thread Switcher Dropdown -->
      <div class="relative ml-2 min-w-0 shrink">
        <button
          type="button"
          class="h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] text-xs text-[var(--text-secondary)] flex items-center gap-1.5 cursor-pointer font-medium transition-colors min-w-0"
          onclick={(e) => {
            e.stopPropagation();
            showThreadDropdown = !showThreadDropdown;
          }}
          aria-expanded={showThreadDropdown}
          title={t("thread.switchThread")}
        >
          <MessageSquare class="size-3 text-[var(--brand-text)] shrink-0" />
          <span class="min-w-0 max-w-[130px] truncate text-[11px] font-mono">
            {currentThread?.title || (threads.length > 0 ? "Threads (" + threads.length + ")" : "New Thread")}
          </span>
          <ChevronDown class="size-3 text-[var(--text-tertiary)] shrink-0" />
        </button>

        {#if showThreadDropdown}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute left-0 top-9 z-50 w-64 bg-[var(--surface-1)] border border-[var(--hairline)] rounded-2xl shadow-2xl p-2 space-y-1 animate-in fade-in zoom-in-95 "
            onclick={(e) => e.stopPropagation()}
          >
            <div class="flex items-center justify-between px-2 py-1 border-b border-[var(--hairline)]">
              <span class="text-[11px] font-bold text-[var(--text-tertiary)] uppercase tracking-wider font-mono">{t("thread.chatHistory")}</span>
              <button
                type="button"
                class="text-[11px] text-[var(--brand-text)] hover:text-[var(--brand-hover)] flex items-center gap-1 cursor-pointer"
                onclick={createNewThread}
              >
                <Plus class="size-3" /> New
              </button>
            </div>

            <div class="max-h-60 overflow-y-auto space-y-0.5 py-1">
              {#each threads as thread (thread.id)}
                {@const isSelected = selectedThreadId === thread.id}
                <button
                  type="button"
                  class="w-full text-left px-2.5 py-1.5 rounded-xl text-xs truncate transition-colors cursor-pointer flex items-center justify-between {isSelected
                    ? 'bg-[var(--surface-3)] text-[var(--text-primary)] font-medium border border-[var(--hairline-strong)]'
                    : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                  onclick={() => loadMessages(thread.id)}
                  aria-pressed={isSelected}
                >
                  <span class="truncate">{thread.title || "Untitled"}</span>
                  {#if isSelected}
                    <span class="size-1.5 rounded-full bg-[var(--brand)] shrink-0 ml-2"></span>
                  {/if}
                </button>
              {:else}
                <div class="p-3 text-center text-xs text-[var(--text-muted)]">{t("thread.noPrev")}</div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Quick New Thread Button -->
      <button
        type="button"
        class="h-7 px-2 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] text-xs text-[var(--text-secondary)] flex items-center gap-1 cursor-pointer transition-colors shrink-0"
        onclick={createNewThread}
        title={t("thread.newThread")}
      >
        <Plus class="size-3.5" />
        <span class="hidden md:inline text-[11px]">{t("thread.new")}</span>
      </button>
    </div>

    <!-- Right Header Controls -->
    <div class="flex items-center gap-2 shrink-0">
      <!-- In-thread search -->
      <div class="relative">
        <button
          type="button"
          class="h-7 px-2 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-xs flex items-center gap-1.5 transition-colors cursor-pointer {threadSearchOpen
            ? 'text-[var(--brand-text)] border-[var(--brand)]/40 bg-[var(--brand-soft)]'
            : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          onclick={() => (threadSearchOpen ? closeSearch() : openSearch())}
          aria-expanded={threadSearchOpen}
          aria-label={t("thread.searchInThread")}
          title={t("thread.searchInThread")}
        >
          <Search class="size-3.5" />
          <span class="hidden lg:inline text-[11px]">{t("thread.searchShort")}</span>
        </button>

        {#if threadSearchOpen}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            class="thread-search"
            role="search"
            onkeydown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                closeSearch();
              }
            }}
          >
            <Search class="thread-search__icon size-3.5" aria-hidden="true" />
            <input
              bind:this={threadSearchInput}
              class="thread-search__input"
              type="search"
              value={threadSearchQuery}
              placeholder={t("thread.searchMessages")}
              aria-label={t("thread.searchMessages")}
              oninput={onSearchInput}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  stepSearch(event.shiftKey ? -1 : 1);
                }
              }}
            />
            <span class="thread-search__count" aria-live="polite">
              {#if threadSearchMatches.length}
                {t("thread.searchCount", {
                  current: threadSearchPosition + 1,
                  total: threadSearchMatches.length,
                })}
              {:else if threadSearchQuery.trim()}
                {t("thread.searchNoResults")}
              {/if}
            </span>
            <button
              type="button"
              class="thread-search__button"
              disabled={!threadSearchMatches.length}
              onclick={() => stepSearch(-1)}
              aria-label={t("thread.searchPrevious")}
              title={t("thread.searchPrevious")}
            >
              <ChevronUp class="size-3.5" />
            </button>
            <button
              type="button"
              class="thread-search__button"
              disabled={!threadSearchMatches.length}
              onclick={() => stepSearch(1)}
              aria-label={t("thread.searchNext")}
              title={t("thread.searchNext")}
            >
              <ChevronDown class="size-3.5" />
            </button>
            <button
              type="button"
              class="thread-search__button"
              onclick={closeSearch}
              aria-label={t("thread.searchClose")}
              title={t("thread.searchClose")}
            >
              <X class="size-3.5" />
            </button>
          </div>
        {/if}
      </div>

      <!-- Session Telemetry Pill -->
      <button
        type="button"
        class="h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-xs font-mono flex items-center gap-1.5 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={() => (showCostInfo = !showCostInfo)}
        aria-pressed={showCostInfo}
        title={t("thread.telemetry")}
      >
        <DollarSign class="size-3 text-success" />
        <span>${sessionCost.toFixed(4)}</span>
      </button>

      <!-- Agent Intelligence / Skills Button -->
      <button
        type="button"
        class="h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-xs flex items-center gap-1.5 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={() => (showIntelligence = true)}
        title={t("thread.intelligence")}
      >
        <Brain class="size-3 text-[var(--brand-text)] shrink-0" />
        <span class="hidden xl:inline text-[11px]">{bot.name.split(" ")[0]} Intelligence</span>
      </button>

      <!-- Thread Drawer Toggle Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showThreadDrawer ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => (showThreadDrawer = !showThreadDrawer)}
        aria-pressed={showThreadDrawer}
        aria-label={t("thread.historyToggle")}
        title={t("thread.historyToggle")}
      >
        <History class="size-3.5" />
      </button>

      <!-- Computer / Desktop Panel Button -->
      <button
        type="button"
        class="relative size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showComputer ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => (showComputer = true)}
        aria-pressed={showComputer}
        aria-label={t("thread.computer")}
        title={agentActivity.computerPaused ? t("computer.takenOver") : t("thread.computer")}
      >
        <Monitor class="size-3.5" />
        {#if agentActivity.computerPaused}
          <span class="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-[var(--warning-text)] ring-2 ring-[var(--surface-2)]" aria-hidden="true"></span>
        {/if}
      </button>

      <!-- Channel (context) picker for new threads -->
      {#if channelOptions.length > 0}
        <SimpleSelect
          value={activeChannelId ?? ""}
          options={[
            { value: "", label: t("thread.noChannel") },
            ...channelOptions.map((c) => ({ value: c.id, label: c.name })),
          ]}
          onValueChange={(v) => (activeChannelId = v || null)}
          placeholder={t("thread.noChannel")}
          class="h-7 w-32 rounded-lg text-[11px] font-mono"
        />
      {/if}

      <!-- Channels manager -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showChannels ? 'bg-success/20 text-success border-success/40' : ''}"
        onclick={() => (showChannels = true)}
        aria-pressed={showChannels}
        aria-label={t("thread.channels")}
        title={t("thread.channels")}
      >
        <Hash class="size-3.5" />
      </button>

      <!-- Import a team -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showTeamImport ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => (showTeamImport = true)}
        aria-pressed={showTeamImport}
        aria-label={t("thread.importTeam")}
        title={t("thread.importTeam")}
      >
        <Users class="size-3.5" />
      </button>

      <!-- Routines / Scheduler Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showRoutines ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => (showRoutines = true)}
        aria-pressed={showRoutines}
        aria-label={t("routines.title")}
        title={t("routines.title")}
      >
        <Clock class="size-3.5" />
      </button>

      <!-- Fleet Sync / Backup Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showSync ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => (showSync = true)}
        aria-pressed={showSync}
        aria-label={t("thread.fleetSync")}
        title={t("thread.fleetSync")}
      >
        <Boxes class="size-3.5" />
      </button>

      <!-- Settings Shortcut -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={triggerOpenSettings}
        aria-label={t("thread.settingsKeys")}
        title={t("thread.settingsKeys")}
      >
        <Settings class="size-3.5" />
      </button>

      <!-- Pause / Play Agent -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={togglePause}
        aria-label={bot.status === "paused" ? "Resume agent" : "Pause all agents"}
        aria-pressed={bot.status === "paused"}
        title={bot.status === "paused" ? "Resume agent" : "Pause all agents (kill switch)"}
      >
        {#if bot.status === "paused"}
          <Play class="size-3.5 fill-current text-[var(--brand-text)]" />
        {:else}
          <Pause class="size-3.5" />
        {/if}
      </button>
    </div>
  </header>

  <!-- Telemetry Strip Banner -->
  {#if showCostInfo}
    <div class="px-4 py-2 bg-[var(--surface-0)] border-b border-[var(--hairline)] flex items-center justify-between text-xs text-[var(--text-tertiary)] font-mono">
      <div class="flex items-center gap-6">
        <span>{t("thread.tokens")}: <strong class="text-[var(--text-primary)]">{sessionTokens.toLocaleString()}</strong></span>
        <span>{t("thread.cost")}: <strong class="text-success">${sessionCost.toFixed(4)}</strong></span>
        <span>{t("thread.model")}: <strong class="text-[var(--brand-text)]">{getModelDisplayName(bot)}</strong></span>
      </div>
      <span class="text-[11px] px-2 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] font-mono uppercase">
        {t("thread.enclaveBadge")}
      </span>
    </div>
  {/if}

  <!-- Main Chat Body & Optional Slide-out Thread Drawer -->
  <div class="flex flex-1 overflow-hidden relative">
    <!-- Optional Slide-out Thread History Drawer -->
    {#if showThreadDrawer}
      <div class="w-60 border-r border-[var(--hairline)] bg-[var(--surface-0)] flex flex-col overflow-hidden shrink-0 z-10 animate-in slide-in-from-left duration-200">
        <div class="p-3 border-b border-[var(--hairline)] flex items-center justify-between">
          <span class="text-[11px] font-bold text-[var(--text-tertiary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
            <History class="size-3.5 text-[var(--brand-text)]" />
            {t("thread.historyToggle")}
          </span>
          <button
            type="button"
            class="size-6 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer"
            onclick={createNewThread}
            title={t("thread.newThread")}
          >
            <Plus class="size-3.5" />
          </button>
        </div>

        <!-- Cross-thread search -->
        <div class="p-2 border-b border-[var(--hairline)]">
          <div class="relative">
            <Search class="absolute left-2 top-1/2 -translate-y-1/2 size-3 text-[var(--text-muted)] pointer-events-none" />
            <input
              bind:value={searchQuery}
              aria-label={t("thread.searchThreads")}
              onkeydown={(e) => {
                if (e.key === "Enter") { e.preventDefault(); runSearch(); }
              }}
              placeholder={t("thread.searchThreads")}
              class="w-full h-7 pl-7 pr-2 rounded-lg bg-[var(--surface-1)] border border-[var(--hairline)] text-[11px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 focus:border-[var(--brand)]/50"
            />
          </div>
          {#if searchResults.length > 0}
            <div class="mt-1.5 space-y-1 max-h-48 overflow-y-auto">
              {#each searchResults as hit (hit.message_id)}
                <button
                  type="button"
                  class="w-full text-left px-2 py-1.5 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--brand)]/40 transition-colors cursor-pointer"
                  onclick={() => loadMessages(hit.thread_id)}
                >
                  <div class="flex items-center gap-1 text-[11px] font-mono text-[var(--text-muted)]">
                    <span class="text-[var(--brand-text)] truncate max-w-[100px]">{hit.thread_title || "Thread"}</span>
                    <span class="shrink-0">· {hit.role}</span>
                  </div>
                  <p class="text-[11px] text-[var(--text-secondary)] leading-snug line-clamp-2 mt-0.5">{hit.snippet}</p>
                </button>
              {/each}
            </div>
          {:else if searchPerformed}
            <div class="mt-1.5 text-[11px] text-[var(--text-muted)] text-center">{t("thread.noMatches")}</div>
          {/if}
        </div>

        <div class="flex-1 overflow-y-auto p-2 space-y-1">
          {#each threads as thread (thread.id)}
            {@const isSelected = selectedThreadId === thread.id}
            <button
              type="button"
              class="w-full text-left px-3 py-2 rounded-xl text-xs truncate transition-all block focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 cursor-pointer {isSelected
                ? 'bg-[var(--surface-3)] border border-[var(--hairline-strong)] text-[var(--text-primary)] font-medium shadow-sm'
                : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
              onclick={() => loadMessages(thread.id)}
              aria-pressed={isSelected}
            >
              {thread.title || "Untitled thread"}
            </button>
          {:else}
            <div class="p-4 text-center text-xs text-[var(--text-muted)]">{t("thread.noThreads")}</div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Chat Messages Stream (+ optional Artifact split view) -->
    <div class="flex flex-1 overflow-hidden bg-[var(--surface-0)]">
      <div class="flex flex-col overflow-hidden {openArtifact ? 'w-[54%] shrink-0' : 'flex-1'}">
      <div class="relative flex-1 min-h-0">
      <div bind:this={messagesContainer} onscroll={handleFeedScroll} aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown" class="h-full overflow-y-auto px-3 pt-2 pb-6 {staticEntries ? 'entries-static' : ''}">
        <div class="max-w-[720px] mx-auto">
          {#if hiddenBefore > 0}
            <div class="load-earlier-row">
              <button
                type="button"
                class="load-earlier"
                onclick={() => void loadEarlier()}
              >
                {t("thread.loadEarlier", { count: hiddenBefore })}
              </button>
            </div>
          {/if}
          {#if messages.length === 0 && !sending && !regenerating}
            <ChatEmptyState
              name={bot.name}
              description={bot.description}
              specialty={bot.specialty}
              imageUrl={bot.avatar_url}
              style={bot.avatar_style}
              onPick={(prompt) => void sendMessage(prompt)}
            />
          {/if}
          {#each visibleMessages as message, vi (message.id || message.created_at)}
            {@const mi = visibleStart + vi}
            {@const isUser = isUserMessage(message)}
            {@const isModelError = typeof message.content === "string" && message.content.includes("⚠️ **Model Error:**")}
            {@const rawContent = typeof message.content === "string" ? message.content : message.content?.text || JSON.stringify(message.content)}
            {@const hasChecklist = message.content?.type === "Checklist" || (typeof message.content === "object" && message.content?.items)}
            {@const messageSources = Array.isArray(message.content?.sources) ? message.content.sources : []}
            {@const messageImages = Array.isArray(message.attachments) ? message.attachments.filter((a: any) => a?.is_image && a?.data) : []}
            {@const messageMedia = Array.isArray(message.attachments) ? message.attachments.filter((a: any) => a?.data && !a?.is_image && /^(audio|video)\//i.test(a?.mime_type || "")) : []}
            {@const messageDocs = Array.isArray(message.attachments) ? message.attachments.filter((a: any) => a?.data && !a?.is_image && !/^(audio|video)\//i.test(a?.mime_type || "")) : []}
            {@const toolAudioB64 = message.content?.type === "ToolResult" && message.content?.result?.audio_b64 ? message.content.result.audio_b64 : null}
            {@const grouped = mi > 0 && isUserMessage(messages[mi - 1]) === isUser}
            {@const continuesRun = mi < messages.length - 1 && isUserMessage(messages[mi + 1]) === isUser}
            {@const ghost = !isUser && !hasChecklist && isGhostContent(rawContent)}

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
              {/snippet}

            <ChatMessageRow
              {isUser}
              isError={isModelError}
              rowId={`message-${messageKey(message)}`}
              searchHit={threadSearchOpen && threadSearchMatchSet.has(messageKey(message))}
              searchCurrent={threadSearchOpen && threadCurrentSearchKey === messageKey(message)}
              replyTo={replyQuoteFor(
                messages,
                message.reply_to_id,
                replyAuthor,
                t("thread.replyUnavailable"),
              )}
              onReply={isPersistedMessageId(message.id)
                ? () => startReply(message)
                : undefined}
              onJumpToReply={jumpToMessage}
              reactions={Array.isArray(message.reactions) ? message.reactions : []}
              onReact={isPersistedMessageId(message.id)
                ? (emoji) => void reactToMessage(message, emoji)
                : undefined}
              text={hasChecklist ? (message.content.text || "") : rawContent}
              reasoning={message.content.reasoning ?? null}
              tools={message.content.tools ?? []}
              spans={message.content.spans ?? []}
              time={formatTime(message.created_at)}
              {grouped}
              showMeta={!continuesRun}
              {ghost}
              fullWidthAgent={true}
              onOpenArtifact={(a) => (openArtifact = a)}
            >
              {#snippet errorCard()}
                <!-- Model Configuration Required Card -->
                <div class="rounded-2xl p-4 bg-[var(--danger-soft)] border border-[var(--danger-border)] text-[var(--text-secondary)] space-y-3 shadow-xl">
                  <div class="flex items-center gap-2 text-[var(--danger-text)] font-bold text-xs font-mono">
                    <AlertTriangle class="size-4 shrink-0" />
                    <span>{t("thread.modelNeeded")}</span>
                  </div>

                  <p class="text-xs text-[var(--text-secondary)] leading-relaxed font-sans">
                    {rawContent.replace("⚠️ **Model Error:** ", "")}
                  </p>

                  <div class="pt-1 flex items-center gap-2">
                    <Button
                      size="sm"
                      class="h-8 gap-1.5 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium cursor-pointer"
                      onclick={triggerOpenSettings}
                    >
                      <Key class="size-3.5" />
                      {t("thread.configureApiKey")}
                    </Button>
                  </div>
                </div>
              {/snippet}



              {#snippet userExtras()}
                {@render messageAttachmentsBlock()}
              {/snippet}

              {#snippet aboveBubble()}
                {#if hasChecklist}
                  <!-- Signature Grok "Thought for X steps" Collapsible Accordion -->
                  <details class="group rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] overflow-hidden" open>
                    <summary class="flex items-center justify-between px-3 py-2 text-[11px] font-mono text-[var(--text-tertiary)] cursor-pointer hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] transition-colors">
                      <div class="flex items-center gap-2">
                        <Brain class="size-3.5 text-[var(--brand-text)]" />
                        <span>Reasoning & Task Execution ({message.content.items.length} steps)</span>
                      </div>
                      <ChevronDown class="size-3.5 group-open:rotate-180 transition-transform" />
                    </summary>

                    <div class="p-3 border-t border-[var(--hairline)] space-y-1.5 bg-[var(--surface-0)]">
                      {#each message.content.items as item}
                        <div class="flex items-center gap-2.5 text-xs bg-[var(--surface-2)] p-2.5 rounded-xl border border-[var(--hairline)]">
                          {#if item.status === "Completed"}
                            <CheckCircle2 class="size-4 text-success shrink-0" />
                          {:else if item.status === "Failed"}
                            <XCircle class="size-4 text-danger shrink-0" />
                          {:else if item.status === "InProgress"}
                            <Loader2 class="size-4 text-[var(--brand-text)] animate-spin shrink-0" />
                          {:else}
                            <Circle class="size-4 text-[var(--text-muted)] shrink-0" />
                          {/if}
                          <span class="font-medium text-[var(--text-secondary)]">{item.label}</span>
                          {#if item.result}
                            <span class="text-[var(--text-tertiary)] ml-auto text-[11px] font-mono">{item.result}</span>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  </details>
                {/if}
              {/snippet}

              {#snippet belowBubble()}
                {@render messageAttachmentsBlock()}
                {@render sourcesChips(messageSources)}
              {/snippet}

              {#snippet actions()}
                {#if toolAudioB64}
                  <button type="button" onclick={() => playAudioBase64(toolAudioB64)} class="message-action-button" title={t("thread.playAudio")}>
                    <Volume2 class="size-[17px]" />
                  </button>
                {/if}
                {#if isUser && !sending && !regenerating}
                  <button
                    type="button"
                    class="message-action-button"
                    onclick={() => startEditing(message.id || message.created_at, rawContent)}
                    title={t("thread.editResend")}
                  >
                    <Pencil class="size-[17px]" />
                  </button>
                {/if}
                {#if !isModelError}
                  <button
                    type="button"
                    class="message-action-button"
                    onclick={() => copyMessage(message.id || message.created_at, rawContent)}
                    title={t("thread.copyFull")}
                  >
                    {#if copiedMessageId === (message.id || message.created_at)}
                      <Check class="size-[17px] text-[var(--success-text)]" />
                    {:else}
                      <Copy class="size-[17px]" />
                    {/if}
                  </button>
                {/if}
                {#if !isModelError && !isUser}
                  <button
                    type="button"
                    class="message-action-button {speakingMessageId === (message.id || message.created_at) ? 'text-[var(--text-primary)]' : ''}"
                    onclick={() => speakMessage(message.id || message.created_at, rawContent)}
                    title={speakingMessageId === (message.id || message.created_at) ? "Stop reading" : "Read this reply aloud"}
                  >
                    <Volume2 class="size-[17px]" />
                  </button>
                {/if}
                {#if !isModelError && !isUser && (messages[messages.length - 1]?.id === message.id) && !sending && !regenerating}
                  <button
                    type="button"
                    class="message-action-button"
                    onclick={regenerate}
                    title={t("thread.regenerate")}
                  >
                    <RotateCcw class="size-[17px]" />
                  </button>
                {/if}
              {/snippet}
            </ChatMessageRow>
          {/each}

          {#snippet liveMarkers()}
            {#if actionMarkers.length}
              <!-- OpenBot action markers: quiet per-tool status rows -->
              <div class="chat-action-markers">
                {#each actionMarkers as m (m.id)}
                  <ChatActionMarker
                    name={m.name}
                    done={m.done}
                    args={m.args}
                    startedAt={m.startedAt ?? null}
                    ms={m.ms ?? null}
                  />
                {/each}
              </div>
            {/if}
          {/snippet}

          {#snippet liveExtras()}
            <!-- Live source chips during streaming -->
            {@render sourcesChips(streamingSources)}
            <!-- Live tool images (e.g. screenshots) during streaming -->
            {#if streamingImages.length}
              <div class="flex flex-wrap gap-2 pt-1">
                {#each streamingImages as img, i (i)}
                  <img
                    src={img.data_url}
                    alt={img.name}
                    class="max-h-64 max-w-full rounded-lg object-contain bg-[var(--surface-1)]"
                  />
                {/each}
              </div>
            {/if}
          {/snippet}

          <!-- Live Streaming Assistant Row (OpenBot: activity line, then tail-revealed
               bubble). Once tokens show it reuses ChatMessageRow so the bubble shell,
               ghost handling, smooth height, and entrance match committed rows exactly. -->
          {#if (sending || regenerating) && selectedThreadId}
            {#if reveal.shown}
              <ChatMessageRow
                isUser={false}
                text={reveal.body}
                streamTail={reveal.tail}
                showMeta={false}
                ghost={isGhostContent(reveal.shown)}
                fullWidthAgent={true}
                reasoningThreadId={selectedThreadId}
                reasoningLive={true}
              >
                {#snippet aboveBubble()}{@render liveMarkers()}{/snippet}
                {#snippet belowBubble()}{@render liveExtras()}{/snippet}
              </ChatMessageRow>
            {:else}
              <!-- Pre-first-token: AgentActivity row (32px avatar + shimmer label) -->
              <div class="message-entry flex gap-3.5 justify-start">
                <div class="min-w-0 w-full space-y-1.5">
                  {@render liveMarkers()}
                  <div class="agent-activity-row flex items-center gap-2.5 min-h-[44px]">
                    <div class="size-8 rounded-full overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline)] shrink-0">
                      <RavenAvatar name={bot.name} imageUrl={bot.avatar_url} style={bot.avatar_style} />
                    </div>
                    <span class="agent-activity-label">{t(activityLine)}</span>
                  </div>
                  {@render liveExtras()}
                </div>
              </div>
            {/if}
          {/if}

          <!-- Approval cards: what the bot wants to do + Allow/Deny -->
          {#each pendingApprovals as ap (ap.id)}
            <ApprovalCard
              botName={bot.name}
              toolName={ap.tool_name}
              toolLabel={ap.tool_label}
              args={ap.arguments}
              risk={ap.risk}
              deciding={decidingApproval === ap.id}
              onDecide={(allowed) => decideApproval(ap.id, allowed)}
            />
          {/each}

          <!-- Question cards: the agent needs input to continue -->
          {#each pendingQuestions as q (q.id)}
            <QuestionCard
              botName={bot.name}
              header={q.header}
              question={q.question}
              options={q.options || []}
              allowCustom={q.allow_custom !== false}
              answering={answeringQuestion === q.id}
              onAnswer={(answer) => answerQuestion(q.id, answer)}
            />
          {/each}
        </div>
        </div>
        {#if !stickToLatest}
          <button
            type="button"
            class="jump-latest"
            onclick={jumpToLatest}
            title={unseenMessages > 0
              ? t(unseenMessages === 1 ? "thread.newMessage" : "thread.newMessages", {
                  count: unseenMessages,
                })
              : t("thread.jumpLatest")}
            aria-label={unseenMessages > 0
              ? t(unseenMessages === 1 ? "thread.newMessage" : "thread.newMessages", {
                  count: unseenMessages,
                })
              : t("thread.jumpLatest")}
          >
            <ArrowDown class="size-3.5" />
            {#if unseenMessages > 0}
              <span class="jump-latest__count" aria-hidden="true">{unseenMessages}</span>
            {/if}
            <span>{t("thread.jumpLatest")}</span>
          </button>
        {/if}
      </div>

      <!-- Grok Floating Capsule Composer -->
      <div class="p-4 bg-[var(--surface-0)] shrink-0">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]  p-3 shadow-2xl transition-all duration-200 focus-within:border-[var(--brand)]"
          ondragover={(e) => e.preventDefault()}
          ondrop={handleComposerDrop}
        >
      {#if activeReply}
        <div class="composer-reply">
          <Reply class="composer-reply__icon size-3.5" strokeWidth={1.9} />
          <div class="composer-reply__copy">
            <span class="composer-reply__author">
              {activeReply.author
                ? t("thread.replyingTo", { name: activeReply.author })
                : t("thread.replyingToMessage")}
            </span>
            {#if activeReply.text}
              <span class="composer-reply__text">{activeReply.text}</span>
            {/if}
          </div>
          <button
            type="button"
            class="composer-reply__close"
            onclick={cancelReply}
            aria-label={t("thread.cancelReply")}
            title={t("thread.cancelReply")}
          >
            <X class="size-3.5" />
          </button>
        </div>
      {/if}
      <!-- Pending attachments (images as thumbnails, docs as chips) -->
      {#if pendingAttachments.length}
        <div class="flex flex-wrap gap-1.5 pb-1.5">
          {#each pendingAttachments as att, idx}
            {#if att.isImage}
              <div class="relative size-14 rounded-lg overflow-hidden border border-[var(--hairline-strong)] bg-[var(--surface-2)] shadow-sm">
                <img
                  src={`data:${att.mime};base64,${att.data}`}
                  alt={att.name}
                  class="size-full object-cover"
                />
                <button
                  type="button"
                  class="absolute top-0.5 right-0.5 size-4 rounded-full bg-[var(--scrim)] text-[var(--text-primary)] text-[11px] flex items-center justify-center cursor-pointer hover:bg-danger/80 transition-colors"
                  onclick={() => removePendingAttachment(idx)}
                  aria-label={t("room.removeAttachment")}
                  title={t("room.removeAttachment")}
                >
                  ✕
                </button>
              </div>
            {:else}
              <div class="relative flex items-center gap-1.5 h-14 max-w-[220px] pl-2.5 pr-6 rounded-lg border border-[var(--hairline-strong)] bg-[var(--surface-2)] shadow-sm" title={att.name}>
                <Paperclip class="size-3.5 text-[var(--brand-text)] shrink-0" />
                <div class="min-w-0">
                  <div class="text-[11px] text-[var(--text-secondary)] truncate">{att.name}</div>
                  <div class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{att.mime.split("/").pop()}</div>
                </div>
                <button
                  type="button"
                  class="absolute top-1 right-1 size-4 rounded-full bg-[var(--scrim)] text-[var(--text-primary)] text-[11px] flex items-center justify-center cursor-pointer hover:bg-danger/80 transition-colors"
                  onclick={() => removePendingAttachment(idx)}
                  aria-label={t("room.removeAttachment")}
                  title={t("room.removeAttachment")}
                >
                  ✕
                </button>
              </div>
            {/if}
          {/each}
        </div>
      {/if}

      {#if pendingApprovals.length > 0}
        <div role="alert" class="mb-2 flex items-center gap-2 rounded-xl border border-warning/30 bg-warning/10 px-3 py-2 text-[11px] text-warning">
          <ShieldAlert class="size-3.5 shrink-0" />
          <span>Waiting on your approval — answer above to resume {bot.name}.</span>
        </div>
      {/if}
      {#if pendingQuestions.length > 0}
        <div role="status" class="mb-2 flex items-center gap-2 rounded-xl border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-3 py-2 text-[11px] text-[var(--brand-text)]">
          <span class="size-1.5 rounded-full bg-[var(--brand)] animate-pulse shrink-0"></span>
          <span>{bot.name} is waiting for your answer — reply to the question above.</span>
        </div>
      {/if}
      <textarea
        bind:this={textareaRef}
        bind:value={newMessage}
        aria-label="Message"
        oninput={handleTextareaInput}
        onpaste={handleComposerPaste}
        placeholder={pendingApprovals.length > 0 ? "Answer the approval above first…" : pendingQuestions.length > 0 ? "Answer the question above…" : `Ask anything, run code, or attach images to ${bot.name}...`}
        disabled={pendingApprovals.length > 0 || pendingQuestions.length > 0}
        rows={1}
        class="w-full bg-transparent text-xs sm:text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none min-h-[44px] max-h-40 leading-relaxed font-sans"
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            sendMessage();
          } else if (e.key === "Escape" && editingMessage) {
            e.preventDefault();
            cancelEditing();
          } else if (e.key === "Escape" && replyingTo) {
            e.preventDefault();
            cancelReply();
          }
        }}
      ></textarea>

      <!-- Edit-and-resend banner -->
      {#if editingMessage}
        <div class="flex items-center justify-between pt-2 border-t border-warning/20 mt-1">
          <div class="flex items-center gap-1.5 text-[11px] font-mono text-warning">
            <Pencil class="size-3" />
            <span>{t("thread.editingHint")}</span>
          </div>
          <button
            type="button"
            class="text-[11px] font-mono text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
            onclick={cancelEditing}
            title={t("thread.cancelEdit")}
          >
            Cancel
          </button>
        </div>
      {/if}

          <!-- Action Toolbar Inside Capsule -->
          <div class="flex items-center justify-between pt-2 border-t border-[var(--hairline)] mt-1">
            <!-- Left Tool Toggles -->
            <div class="flex items-center gap-1.5">
              <button
                type="button"
                class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {deepSearchActive
                  ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/50 '
                  : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                onclick={() => (deepSearchActive = !deepSearchActive)}
                aria-pressed={deepSearchActive}
                title={t("home.deepSearch")}
              >
                <Globe class="size-3" />
                <span>DeepSearch</span>
              </button>

              <button
                type="button"
                class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {thinkActive
                  ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/50 '
                  : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                onclick={() => (thinkActive = !thinkActive)}
                aria-pressed={thinkActive}
                title={t("home.think")}
              >
                <Brain class="size-3" />
                <span>{t("home.thinkPill")}</span>
              </button>

              <ContextMeter threadId={selectedThreadId} class="ml-1" />
              <button
                type="button"
                class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center transition-colors cursor-pointer"
                onclick={attachFile}
                aria-label={t("home.attach")}
                title={t("home.attach")}
              >
                <Paperclip class="size-3.5" />
              </button>

              <button
                type="button"
                class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {isListening ? 'text-danger bg-danger/20 border border-danger/50 animate-pulse shadow-sm' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                onclick={toggleVoice}
                aria-pressed={isListening}
                aria-label="Voice input"
                title={isListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
              >
                <Mic class="size-3.5" />
              </button>

              <button
                type="button"
                class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {voiceMode ? 'text-success bg-success/20 border border-success/50 shadow-sm' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'} {speaking ? 'animate-pulse' : ''}"
                onclick={toggleVoiceMode}
                aria-pressed={voiceMode}
                aria-label="Voice mode"
                title={voiceMode ? "Voice mode on — click to stop (hands-free loop)" : "Voice mode: hands-free talk → response spoken aloud"}
              >
                <Volume2 class="size-3.5" />
              </button>

              <button
                type="button"
                class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {bot?.config?.auto_read
                  ? 'bg-success/15 text-success border-success/40'
                  : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                onclick={toggleAutoRead}
                aria-pressed={Boolean(bot?.config?.auto_read)}
                title={bot?.config?.auto_read ? "Auto-read ON — replies are spoken aloud" : "Auto-read: speak every reply aloud"}
              >
                <Volume2 class="size-3" />
                <span>{t("thread.autoRead")}</span>
              </button>

              <button
                type="button"
                class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {tempActive ? 'text-warning bg-warning/15 border border-warning/40' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                onclick={() => (tempActive = !tempActive)}
                aria-pressed={tempActive}
                aria-label="Temporary chat"
                title={tempActive ? "Temporary chat ON — new threads won't feed agent memory" : "Temporary chat: conversations won't feed agent memory"}
              >
                <Ghost class="size-3.5" />
              </button>
            </div>

            <!-- Right Controls: Model Pill & High-Contrast Send Button -->
            <div class="flex items-center gap-2">
              <span class="text-[11px] font-mono text-[var(--text-muted)] px-2 py-0.5 rounded border border-[var(--hairline)] bg-[var(--surface-1)] hidden sm:inline">
                {getModelDisplayName(bot)}
              </span>

              <button
                type="button"
                onclick={() => sendMessage()}
                disabled={(!newMessage.trim() && pendingAttachments.length === 0) || sending}
                aria-label={t("home.send")}
                class="size-8 rounded-full flex items-center justify-center transition-all duration-200 cursor-pointer {(newMessage.trim() || pendingAttachments.length) && !sending
                  ? 'btn-brand text-[var(--text-primary)] hover:scale-105 active:scale-95'
                  : 'bg-[var(--surface-3)] text-[var(--text-muted)] cursor-not-allowed'}"
                title={t("home.send")}
              >
                {#if sending}
                  <Loader2 class="size-3.5 animate-spin" />
                {:else}
                  <ArrowUp class="size-4 stroke-[2.5]" />
                {/if}
              </button>
            </div>
          </div>
        </div>

        <div class="text-center mt-2">
          <span class="text-[11px] text-[var(--text-muted)] font-mono">
            RAVENBOT local enclave active • ⌘K for command palette • ⌘, for settings
          </span>
        </div>
      </div>
      </div>

      <!-- Artifact / Canvas Split Panel -->
      {#if openArtifact}
        <div class="w-[46%] border-l border-[var(--hairline)] shrink-0">
          <ArtifactPanel artifact={openArtifact} onClose={() => (openArtifact = null)} />
        </div>
      {/if}
    </div>
  </div>
</div>

<!-- Intelligence Modal -->
{#if showIntelligence}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-[var(--scrim)]  flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (showIntelligence = false)}
  >
    <div
      class="modal-panel w-full max-w-lg p-6 relative space-y-4"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-center justify-between border-b border-[var(--hairline)] pb-3">
        <div class="flex items-center gap-2">
          <Brain class="size-4 text-[var(--brand-text)]" />
          <span class="font-bold text-sm text-[var(--text-primary)]">{bot.name} Intelligence</span>
        </div>
        <button
          type="button"
          class="size-6 rounded-md hover:bg-[var(--surface-3)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer text-xs font-mono"
          onclick={() => (showIntelligence = false)}
        >
          ✕
        </button>
      </div>
      <AgentIntelligence
        botId={bot.id}
        botName={bot.name}
      />
    </div>
  </div>
{/if}

<!-- Computer / Desktop Panel -->
{#if showComputer}
  <ComputerPanel
    botName={bot.name}
    botId={bot.id}
    hostControl={bot.config?.host_control === true}
    onClose={() => (showComputer = false)}
  />
{/if}

<!-- Channels manager -->
{#if showChannels}
  <ChannelsPanel
    onClose={() => (showChannels = false)}
    onChanged={loadChannelOptions}
  />
{/if}

<!-- Team import -->
{#if showTeamImport}
  <TeamImport
    onClose={() => (showTeamImport = false)}
    onImported={() => { loadChannelOptions(); }}
  />
{/if}

<!-- Fleet Sync / Backup Modal -->
{#if showSync}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-[var(--scrim)]  flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (showSync = false)}
  >
    <div
      class="modal-panel w-full max-w-lg p-6 relative space-y-4"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- SyncPanel's own compact header carries the title; close-only row. -->
      <div class="flex items-center justify-end border-b border-[var(--hairline)] pb-2">
        <button
          type="button"
          class="size-6 rounded-md hover:bg-[var(--surface-3)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer text-xs font-mono"
          onclick={() => (showSync = false)}
        >
          ✕
        </button>
      </div>
      <SyncPanel bot={bot} onBotImported={() => onBotUpdated?.(bot)} />
    </div>
  </div>
{/if}

<!-- Routines / Scheduler Modal -->
{#if showRoutines}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-[var(--scrim)]  flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (showRoutines = false)}
  >
    <div
      class="modal-panel w-full max-w-lg p-6 relative space-y-4"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- RoutinesPanel's own compact header carries the title; this row is
           just the close affordance now (raw "{bot.name} Routines" removed). -->
      <div class="flex items-center justify-end border-b border-[var(--hairline)] pb-2">
        <button
          type="button"
          class="size-6 rounded-md hover:bg-[var(--surface-3)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer text-xs font-mono"
          onclick={() => (showRoutines = false)}
        >
          ✕
        </button>
      </div>
      <RoutinesPanel bot={bot} />
    </div>
  </div>
{/if}

