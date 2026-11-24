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
  import TranscriptRail from "$lib/components/chat/TranscriptRail.svelte";
  import VirtualTranscript from "$lib/components/chat/VirtualTranscript.svelte";
  import MessageSelectionToolbar from "$lib/components/chat/MessageSelectionToolbar.svelte";
  import { quoteAsMarkdown, type SelectionQuote } from "$lib/chat/selection";
  import ChatEmptyState from "$lib/components/chat/ChatEmptyState.svelte";
  import DiffView from "$lib/components/workspace/DiffView.svelte";
  import { matchingMessageIds, messageKey, stepMatchIndex } from "$lib/chat/search";
  import { agentActivity } from "$lib/agentActivity.svelte";
  import {
    MESSAGE_FOCUS_EVENT,
    THREAD_FOCUS_EVENT,
    takePendingMessageFocus,
    takePendingThreadFocus,
    type MessageFocusTarget,
    type ThreadFocusTarget,
  } from "$lib/chat/messageFocus";
  import { isPersistedMessageId, toggleReaction as nextReactions } from "$lib/chat/reactions";
  import { replyQuoteFor } from "$lib/chat/replies";
  import { messageDayKey, messageDayLabel } from "$lib/chat/dayMarkers";
  import { isEditableTarget, navigateMessage } from "$lib/chat/navigation";
  import { anchoredScrollTop, transcriptWindow } from "$lib/chat/messageWindow";
  import { estimateMessageHeight } from "$lib/chat/virtualRows";
  import { prewarmMarkdownMessages } from "$lib/markdown/offThread";
  import {
    mergeOlderMessages,
    oldestMessageId,
    MESSAGE_PAGE_SIZE,
  } from "$lib/chat/messagePaging";
  import {
    mergeReplyTargets,
    replyTargetsToLoad,
  } from "$lib/chat/replyResolution";
  import { drafts, threadDraftKey } from "$lib/chat/drafts";
  import {
    applyMention,
    filterMentionItems,
    mentionAtCaret,
    type MentionItem,
    type MentionSigil,
    type MentionToken,
  } from "$lib/chat/mentions";
  import ComposerMentionMenu from "$lib/components/chat/ComposerMentionMenu.svelte";
  import ApprovalCard from "$lib/components/chat/ApprovalCard.svelte";
  import InteractionBlockCard from "$lib/components/chat/InteractionBlock.svelte";
  import type {
    InteractionBlock as InteractionBlockPayload,
    QuestionKind,
  } from "$lib/chat/blocks";
  import ContextMeter from "$lib/components/chat/ContextMeter.svelte";
  import ImageGallery from "$lib/components/chat/ImageGallery.svelte";
  import ImageGeneration from "$lib/components/chat/ImageGeneration.svelte";
  import FileAttachmentCard from "$lib/components/chat/FileAttachmentCard.svelte";
  import { contextUsage } from "$lib/chat/contextUsage.svelte";
  import { canSteerRun } from "$lib/chat/steering";
  import ModelPicker from "$lib/components/ModelPicker.svelte";
  import { makeArtifact } from "$lib/artifact";
  import { artifactStore } from "$lib/artifacts.svelte";
  import { sidePanel } from "$lib/workspace/sidePanel.svelte";
  import RoutinesPanel from "$lib/components/RoutinesPanel.svelte";
  import SyncPanel from "$lib/components/SyncPanel.svelte";
  import ComputerPanel from "$lib/components/ComputerPanel.svelte";
  import ChannelsPanel from "$lib/components/ChannelsPanel.svelte";
  import TeamImport from "$lib/components/TeamImport.svelte";
  import { getDiceBearUrl, isUserMessage } from "$lib/utils";
  import { notify } from "$lib/toast";
  import { announceAttentionChanged } from "$lib/attention.svelte";
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
    Target,
    GitBranch,
    Share2,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    onBotUpdated?: (bot: any) => void;
  }

  let { bot, onBotUpdated }: Props = $props();

  let threads = $state<any[]>([]);
  let selectedThreadId = $state<string | null>(null);
  let messages = $state<any[]>([]);
  /** The persistent 1:1 goal attached to the selected thread. */
  let threadGoal = $state<any | null>(null);
  let goalOpen = $state(false);
  let goalObjective = $state("");
  let goalOutcome = $state("");
  let goalGate = $state("");
  let goalRounds = $state(8);
  let goalTokens = $state(0);
  let goalBusy = $state(false);
  let goalError = $state("");
  let goalObjectiveRef = $state<HTMLTextAreaElement | null>(null);

  interface ThreadShareRecord {
    id: string;
    thread_id: string;
    created_at: string;
    expires_at: string;
    revoked_at: string | null;
    view_count: number;
    last_viewed_at: string | null;
  }

  /** Read-only public links for the selected conversation. */
  let shareOpen = $state(false);
  let shareBusy = $state(false);
  let shareError = $state("");
  let shareTtl = $state(24);
  let shareLink = $state<{ id: string; url: string; token: string; expires_at: string } | null>(null);
  let shareList = $state<ThreadShareRecord[]>([]);
  let shareStatus = $state<{ running: boolean; url: string; allow_lan: boolean; tls: boolean } | null>(null);
  let shareCopied = $state(false);
  let shareDialogEl = $state<HTMLElement | null>(null);

  /** Conversation-scoped Git worktree isolation and its review. */
  let worktree = $state<any | null>(null);
  let worktreeReview = $state<any | null>(null);
  let worktreeBusy = $state(false);
  let worktreeError = $state("");
  let worktreeReviewEl = $state<HTMLElement | null>(null);
  /** The first thread load, so a focus request can wait for it instead of racing it. */
  let initialLoad: Promise<void> | null = null;
  let loadedBotId: string | null = null;
  let loadingBotId: string | null = null;
  let loadError = $state<string | null>(null);
  let loadSeq = 0;

  $effect(() => {
    if (!selectedThreadId) return;
    const current = messages.filter((message) => message.thread_id === selectedThreadId);
    if (current.length) contextUsage.hydrate(selectedThreadId, current);
  });

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
  let hasEarlier = $state(false);
  let loadingEarlier = $state(false);
  let windowEnd = $state(0);
  const windowBounds = $derived(
    transcriptWindow(messages.length, messageWindow, windowEnd),
  );
  const visibleStart = $derived(windowBounds.start);
  const visibleEnd = $derived(windowBounds.end);
  const visibleMessages = $derived(messages.slice(visibleStart, visibleEnd));
  const hiddenBefore = $derived(visibleStart);

  // Prepare long bodies in the loaded window while the cheap rows paint. The
  // worker dedupes and caps its own batch; this is a warming hint, not work the
  // renderer waits for.
  $effect(() => {
    prewarmMarkdownMessages(visibleMessages);
  });

  // The row virtualizer mounts only the viewport plus overscan. `virtualTarget`
  // is a one-shot request to mount a message before a search hit or a reply
  // jump scrolls to it; the component clears it after handling.
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

  async function loadEarlier() {
    if (loadingEarlier || (hiddenBefore <= 0 && !hasEarlier)) return;
    const el = messagesContainer;
    const previousTop = el?.scrollTop ?? 0;
    const previousHeight = el?.scrollHeight ?? 0;
    loadingEarlier = true;
    try {
      if (hiddenBefore > 0) {
        messageWindow += MESSAGE_WINDOW_STEP;
      } else if (selectedThreadId) {
        const page: any = await invoke("list_messages_page", {
          threadId: selectedThreadId,
          beforeId: oldestMessageId(messages),
          limit: MESSAGE_PAGE_SIZE,
        });
        const older = Array.isArray(page?.messages) ? page.messages : [];
        hasEarlier = !!page?.has_more;
        if (older.length === 0) return;
        messages = mergeOlderMessages(messages, older);
        messageWindow += older.length;
        void resolveReplyTargets(selectedThreadId, messages);
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
    // The row may not be mounted: the virtualizer re-windows around this key
    // and scrolls once the row exists.
    virtualTarget = key;
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

  async function openSearch() {
    threadSearchOpen = true;
    // Search covers the whole thread, not only the cursor-loaded tail. Load
    // the full transcript on demand; normal scrolling stays paginated.
    if (hasEarlier && selectedThreadId) {
      try {
        messages = await invoke("list_messages", { threadId: selectedThreadId });
        hasEarlier = false;
      } catch {
        /* search still works over the loaded pages */
      }
    }
    await tick();
    threadSearchInput?.focus();
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

  function messageById(id: string) {
    return messages.find((message) => messageKey(message) === id) ?? null;
  }

  /** Reply to the whole message that owns the selected passage. */
  function replyFromSelection(messageId: string) {
    const message = messageById(messageId);
    if (message) startReply(message);
  }

  /**
   * Quote a passage: the source becomes the reply target and the passage enters
   * the draft as a Markdown blockquote. Sending keeps both, so the model sees
   * the exact words the user highlighted and the transcript can jump back.
   */
  function quoteFromSelection(selection: SelectionQuote) {
    const message = messageById(selection.messageId);
    if (!message) return;
    startReply(message);
    newMessage = `${quoteAsMarkdown(selection.text)}\n\n${newMessage}`.trimStart();
    void tick().then(() => textareaRef?.focus());
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

  async function focusThreadTarget(target: ThreadFocusTarget) {
    if (target.botId !== bot.id) return;
    if (target.threadId !== selectedThreadId) {
      await loadMessages(target.threadId);
    }
  }

  async function handleThreadFocus(event: Event) {
    const target = (event as CustomEvent<ThreadFocusTarget>).detail;
    if (!target || target.botId !== bot.id) return;
    ensureBotLoaded(bot.id);
    if (initialLoad) await initialLoad;
    // An initial load owns the hand-off while it is running; if it consumed
    // this request, there is nothing left to do here.
    if (!takePendingThreadFocus(bot.id)) return;
    await focusThreadTarget(target);
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
    if (!botId || loadedBotId === botId || loadingBotId === botId) return;
    if (loadedBotId) {
      drafts.saveNow(threadDraftKey(loadedBotId, selectedThreadId), newMessage);
    }
    loadingBotId = botId;
    loadError = null;
    threads = [];
    selectedThreadId = null;
    messages = [];
    streamingText = "";
    streamingSources = [];
    streamingImages = [];
    // Drafts, attachments and parked decisions belong to the previous
    // conversation. Carrying them across a bot switch sent the wrong text or
    // file into the wrong agent; the new conversation starts with its own
    // saved draft.
    newMessage = drafts.load(threadDraftKey(botId, null));
    pendingAttachments = [];
    pendingApprovals = [];
    pendingQuestions = [];
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
        const threadTarget = takePendingThreadFocus(botId);
        if (target) {
          await loadMessages(target.threadId);
          if (seq !== loadSeq) return;
          await tick();
          await revealMessage(target.messageId);
        } else if (threadTarget) {
          await loadMessages(threadTarget.threadId);
        } else if (threads.length > 0) {
          await loadMessages(threads[0].id);
        }
        if (seq === loadSeq) loadedBotId = botId;
      } catch (e) {
        // Do not mark the bot loaded: the user can switch back and retry, and
        // the next effect will run rather than showing a permanent empty state.
        if (seq === loadSeq) {
          loadedBotId = null;
          loadError = String(e);
        }
        console.error("Failed to load threads:", e);
      } finally {
        if (seq === loadSeq) {
          initialLoad = null;
          loadingBotId = null;
        }
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
  let mentionOpen = $state(false);
  let mentionSigil = $state<MentionSigil>("#");
  let mentionToken = $state<MentionToken | null>(null);
  let mentionItems = $state<MentionItem[]>([]);
  let mentionIndex = $state(0);
  let fileMentions = $state<MentionItem[]>([]);
  let skillMentions = $state<MentionItem[]>([]);
  let mentionSourcesBotId: string | null = null;
  let sending = $state(false);
  let pendingMessageId = $state<string | null>(null);
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
      announceAttentionChanged();
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
    kind?: QuestionKind | null; payload?: InteractionBlockPayload | null;
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
      announceAttentionChanged();
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
  let railEntries = $derived(
    visibleMessages
      .filter((message) => isUserMessage(message))
      .map((message) => {
        const text =
          typeof message.content === "string"
            ? message.content
            : message.content?.text || "";
        return {
          id: `message-${messageKey(message)}`,
          kind: "user" as const,
          label: text.trim().slice(0, 80) || t("thread.you"),
        };
      }),
  );

  function jumpToRail(id: string) {
    void revealMessage(id.replace(/^message-/, ""));
  }
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
  import { sessionRole } from "$lib/sessionRole.svelte";

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
  const liveImageGeneration = $derived(
    [...actionMarkers].reverse().find((marker) => marker.name === "image_gen" && !marker.done) ??
      null,
  );
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
  // Temporary (ephemeral) chat mode: new threads skip agent-memory persistence
  let tempActive = $state(false);
  // Edit-and-resend: editing a prior user turn removes everything after it
  let editingMessage = $state<{ id: string; original: string } | null>(null);
  // Hands-free voice mode: STT → send → TTS loop
  let voiceMode = $state(false);
  let speaking = $state(false);
  let unlisten: UnlistenFn | null = null;
  /** Refreshes the goal banner and transcript after a background goal round. */
  let goalUnlisten: UnlistenFn | null = null;

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
    window.addEventListener(THREAD_FOCUS_EVENT, handleThreadFocus);
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
          case "steer": {
            const id = String(payload?.message_id || "");
            const tid = String(payload?.thread_id || "");
            const content = String(payload?.content || "");
            if (!id || (tid && tid !== selectedThreadId)) break;
            if (messages.some((message) => String(message.id) === id)) break;
            const saved = {
              id,
              thread_id: selectedThreadId,
              role: "user",
              content,
              attachments: [],
              reply_to_id: null,
              reactions: [],
              created_at: new Date().toISOString(),
            };
            const optimistic = messages.findIndex(
              (message) =>
                String(message.id).startsWith("temp-steer-") && message.content === content,
            );
            messages =
              optimistic >= 0
                ? messages.map((message, index) => (index === optimistic ? saved : message))
                : [...messages, saved];
            scrollToBottom();
            break;
          }
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
              void loadThreadWorktree(selectedThreadId);
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
              if (nextStatus === "idle" && selectedThreadId) {
                // A run can create or fail to create an isolated checkout
                // before it ever reaches a `done` event.
                void loadThreadWorktree(selectedThreadId);
              }
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
      goalUnlisten = await listen<any>("thread-goal", (event) => {
        const tid = String(event.payload?.thread_id || "");
        if (!tid || tid !== selectedThreadId) return;
        void loadThreadGoal(tid);
        void loadThreadWorktree(tid);
        void loadMessages(tid);
      });
    } catch (e) {
      console.error("Failed to attach stream listener:", e);
    }
  });

  onDestroy(() => {
    unlisten?.();
    unlisten = null;
    goalUnlisten?.();
    goalUnlisten = null;
    window.removeEventListener("local-skill-try", handleTryLocalSkill);
    window.removeEventListener(MESSAGE_FOCUS_EVENT, handleMessageFocus);
    window.removeEventListener(THREAD_FOCUS_EVENT, handleThreadFocus);
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
    if (botId) {
      ensureBotLoaded(botId);
      void loadMentionSources(botId);
    }
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
      if (nearTop && (hiddenBefore > 0 || hasEarlier)) void loadEarlier();
      return;
    }
    stickToLatest = el.scrollHeight - el.scrollTop - el.clientHeight <= 80;
    if (stickToLatest) unseenMessages = 0;
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
      messagesContainer,
      event.key === "ArrowUp" ? -1 : 1,
      !prefersReducedMotion(),
      revealAdjacentFromBoundary,
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
    const jump = windowEnd !== 0 || messageWindow !== MESSAGE_WINDOW;
    windowEnd = 0;
    // Returning to the live end also releases any earlier batches the reader
    // loaded while paging through history.
    messageWindow = MESSAGE_WINDOW;
    if (force || jump) virtualPin += 1;
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }

  async function openShareDialog() {
    shareOpen = true;
    shareError = "";
    shareLink = null;
    shareCopied = false;
    await loadShareData();
  }

  async function loadShareData() {
    if (!selectedThreadId) return;
    try {
      const [shares, status] = await Promise.all([
        invoke<ThreadShareRecord[]>("list_thread_shares", {
          threadId: selectedThreadId,
        }),
        invoke<{ running: boolean; url: string; allow_lan: boolean; tls: boolean }>(
          "get_remote_status",
        ),
      ]);
      shareList = Array.isArray(shares) ? shares : [];
      shareStatus = status;
    } catch (error: any) {
      shareError = error?.toString() || "Could not load share links";
    }
  }

  async function createShareLink() {
    if (!selectedThreadId || shareBusy || !shareStatus?.running) return;
    shareBusy = true;
    shareError = "";
    shareCopied = false;
    try {
      shareLink = await invoke("create_thread_share", {
        threadId: selectedThreadId,
        ttlHours: shareTtl,
      });
      await loadShareData();
    } catch (error: any) {
      shareError = error?.toString() || "Could not create the share link";
    } finally {
      shareBusy = false;
    }
  }

  async function revokeShare(share: ThreadShareRecord) {
    shareError = "";
    try {
      await invoke("revoke_thread_share", { shareId: share.id });
      if (shareLink?.id === share.id) shareLink = null;
      await loadShareData();
    } catch (error: any) {
      shareError = error?.toString() || "Could not revoke the share link";
    }
  }

  async function copyShareLink() {
    if (!shareLink) return;
    try {
      await navigator.clipboard.writeText(shareLink.url);
      shareCopied = true;
      setTimeout(() => (shareCopied = false), 1500);
    } catch (error) {
      shareError = "The clipboard is unavailable. Select the link and copy it manually.";
    }
  }

  function shareIsActive(share: ThreadShareRecord): boolean {
    return !share.revoked_at && new Date(share.expires_at).getTime() > Date.now();
  }

  function formatShareTime(value: string | null): string {
    if (!value) return "never";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  async function loadMessages(threadId: string) {
    const changed = threadId !== selectedThreadId;
    if (changed) {
      replyingTo = null;
      worktreeReview = null;
      worktreeError = "";
      if (bot?.id) {
        drafts.saveNow(threadDraftKey(bot.id, selectedThreadId), newMessage);
      }
    }
    stickToLatest = true;
    unseenMessages = 0;
    seenMessageCount = 0;
    messageWindow = MESSAGE_WINDOW;
    windowEnd = 0;
    virtualTarget = "";
    railPositions = {};
    selectedThreadId = threadId;
    void loadThreadGoal(threadId);
    void loadThreadWorktree(threadId);
    if (changed && bot?.id) {
      newMessage = drafts.load(threadDraftKey(bot.id, threadId));
      void tick().then(() => {
        if (!textareaRef) return;
        textareaRef.style.height = "auto";
        textareaRef.style.height = Math.min(textareaRef.scrollHeight, 160) + "px";
      });
    }
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
    replyTargets = [];
    try {
      hasEarlier = false;
      const page: any = await invoke("list_messages_page", {
        threadId,
        beforeId: null,
        limit: MESSAGE_PAGE_SIZE,
      });
      messages = Array.isArray(page?.messages) ? page.messages : [];
      hasEarlier = !!page?.has_more;
      messageWindow = Math.max(MESSAGE_WINDOW, messages.length);
      windowEnd = 0;
      void resolveReplyTargets(threadId, messages);
      await tick();
      staticEntries = false;
      scrollToBottom(true);
    } catch (e) {
      staticEntries = false;
      console.error("Failed to load messages:", e);
    }
  }

  // Focus the field a keyboard user came to fill. Svelte rejects a bare
  // `autofocus`, and a modal that opens with focus left behind the scrim is
  // not actually usable without a pointer.
  $effect(() => {
    if (!goalOpen || !goalObjectiveRef) return;
    void tick().then(() => goalObjectiveRef?.focus());
  });

  $effect(() => {
    if (!shareOpen) return;
    void tick().then(() => shareDialogEl?.focus());
  });

  $effect(() => {
    if (!worktreeReview) return;
    void tick().then(() => worktreeReviewEl?.focus());
  });

  async function loadThreadGoal(threadId: string | null) {
    if (!threadId) {
      threadGoal = null;
      return;
    }
    try {
      const goal = await invoke("get_thread_goal", { threadId });
      // A fast thread switch must not let an older response win.
      if (threadId === selectedThreadId) threadGoal = goal;
    } catch (error) {
      console.debug("thread goal unavailable:", error);
      if (threadId === selectedThreadId) threadGoal = null;
    }
  }

  function openGoalDialog() {
    goalError = "";
    goalObjective = threadGoal?.goal?.objective || "";
    goalOutcome = threadGoal?.goal?.contract?.outcome || "";
    goalGate = threadGoal?.gates?.[0]?.command || "";
    goalRounds = threadGoal?.goal?.max_rounds || 8;
    goalTokens = threadGoal?.goal?.contract?.max_tokens || 0;
    goalOpen = true;
  }

  async function startThreadGoal() {
    const threadId = selectedThreadId;
    if (!threadId) {
      goalError = "Start a conversation thread first.";
      return;
    }
    goalBusy = true;
    goalError = "";
    try {
      const goal = await invoke("start_thread_goal", {
        threadId,
        objective: goalObjective,
        outcome: goalOutcome || null,
        gate: goalGate || null,
        maxRounds: Number(goalRounds) || 8,
        maxTokens: Number(goalTokens) || 0,
      });
      if (threadId === selectedThreadId) {
        threadGoal = goal;
        goalOpen = false;
      }
    } catch (error) {
      if (threadId === selectedThreadId) goalError = String(error);
    } finally {
      goalBusy = false;
    }
  }

  async function threadGoalAction(action: "pause" | "cancel") {
    const threadId = selectedThreadId;
    if (!threadId) return;
    goalBusy = true;
    goalError = "";
    try {
      const goal = await invoke(`${action}_thread_goal`, { threadId });
      if (threadId === selectedThreadId) threadGoal = goal;
    } catch (error) {
      if (threadId === selectedThreadId) goalError = String(error);
    } finally {
      goalBusy = false;
    }
  }

  /**
   * Resume a paused/blocked goal.
   *
   * The banner resumes with the saved budget; the dialog resumes with the
   * values in its inputs, so a user who hit the token ceiling can raise it
   * before the next round spends anything.
   */
  async function resumeThreadGoal(withBudget = false) {
    const threadId = selectedThreadId;
    if (!threadId) return;
    goalBusy = true;
    goalError = "";
    try {
      const args: Record<string, unknown> = { threadId };
      if (withBudget) {
        args.maxRounds = Number(goalRounds) || 8;
        args.maxTokens = Number(goalTokens) || 0;
      }
      const goal = await invoke("resume_thread_goal", args);
      if (threadId === selectedThreadId) {
        threadGoal = goal;
        goalOpen = false;
      }
    } catch (error) {
      if (threadId === selectedThreadId) goalError = String(error);
    } finally {
      goalBusy = false;
    }
  }

  async function loadThreadWorktree(threadId: string | null) {
    if (!threadId) {
      worktree = null;
      return;
    }
    try {
      const value = await invoke("get_thread_worktree", { threadId });
      if (threadId === selectedThreadId) worktree = value;
    } catch (error) {
      console.debug("thread worktree unavailable:", error);
      if (threadId === selectedThreadId) worktree = null;
    }
  }

  async function toggleWorktreeIsolation() {
    const threadId = selectedThreadId;
    if (!threadId || worktreeBusy) return;
    worktreeBusy = true;
    worktreeError = "";
    try {
      const value = await invoke("set_thread_worktree_isolation", {
        threadId,
        enabled: !worktree?.enabled,
      });
      if (threadId === selectedThreadId) worktree = value;
    } catch (error) {
      if (threadId === selectedThreadId) worktreeError = String(error);
    } finally {
      worktreeBusy = false;
    }
  }

  async function openWorktreeReview() {
    const threadId = selectedThreadId;
    if (!threadId || worktreeBusy) return;
    worktreeBusy = true;
    worktreeError = "";
    try {
      const review = await invoke("review_thread_worktree", { threadId });
      if (threadId === selectedThreadId) worktreeReview = review;
    } catch (error) {
      if (threadId === selectedThreadId) worktreeError = String(error);
    } finally {
      worktreeBusy = false;
    }
  }

  async function applyWorktree() {
    const threadId = selectedThreadId;
    if (!threadId || worktreeBusy) return;
    worktreeBusy = true;
    worktreeError = "";
    try {
      const review = await invoke("apply_thread_worktree", { threadId });
      if (threadId === selectedThreadId) worktreeReview = review;
      await loadThreadWorktree(threadId);
    } catch (error) {
      if (threadId === selectedThreadId) worktreeError = String(error);
    } finally {
      worktreeBusy = false;
    }
  }

  async function discardWorktree() {
    const threadId = selectedThreadId;
    if (!threadId || worktreeBusy) return;
    worktreeBusy = true;
    worktreeError = "";
    try {
      const review = await invoke("discard_thread_worktree", { threadId });
      if (threadId === selectedThreadId) worktreeReview = review;
      await loadThreadWorktree(threadId);
    } catch (error) {
      if (threadId === selectedThreadId) worktreeError = String(error);
    } finally {
      worktreeBusy = false;
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
      threadGoal = null;
      worktree = null;
      worktreeReview = null;
      worktreeError = "";
      void loadThreadWorktree((thread as any).id);
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
  const runActive = $derived(sending || (!!activeRunId && !pausedRunId));
  const canSteer = $derived(
    canSteerRun({
      running: runActive,
      streamingTool,
      activeRunId,
      text: newMessage,
      attachmentCount: pendingAttachments.length,
    }),
  );

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

  /**
   * Stop the active run. Pause is resumable; stop is final. Without this a
   * hung provider leaves the composer locked until the IPC promise settles.
   */
  async function stopActiveRun() {
    if (!activeRunId) return;
    try {
      await invoke("cancel_run", { runId: activeRunId });
      activeRunId = null;
      pausedRunId = null;
      streamingText = "";
      streamingTool = null;
      notify("Stopped the active run.", "success");
    } catch (e) {
      notify(`Could not stop the run: ${String(e)}`, "error");
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

  async function steerActiveRun(text: string) {
    if (!selectedThreadId || !streamingTool || !activeRunId) return;
    const tempId = "temp-steer-" + Date.now();
    const tempMessage = {
      id: tempId,
      thread_id: selectedThreadId,
      role: "user",
      content: text,
      attachments: [],
      reply_to_id: null,
      reactions: [],
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempMessage];
    pendingMessageId = tempId;
    newMessage = "";
    try {
      const saved: any = await invoke("steer_thread", {
        threadId: selectedThreadId,
        content: text,
      });
      const replaced = messages.map((message) =>
        String(message.id) === tempId ? saved : message,
      );
      messages = replaced.filter(
        (message, index) =>
          replaced.findIndex((other) => String(other.id) === String(message.id)) === index,
      );
      notify(t("thread.steerQueued"), "success");
    } catch (error: any) {
      messages = messages.filter((message) => String(message.id) !== tempId);
      newMessage = text;
      notify(`${t("thread.steerTooLate")} ${String(error)}`, "error");
    } finally {
      pendingMessageId = null;
      scrollToBottom(true);
    }
  }

  async function sendMessage(textToSend?: string) {
    if (sessionRole.role === "viewer") return;
    const typed = (textToSend || newMessage).trim();
    const hasAttachments = pendingAttachments.length > 0;
    if (!typed && !hasAttachments) return;
    if (sending || (activeRunId && !pausedRunId)) {
      // The active run accepts a steer only while a tool is executing. The
      // backend enforces the same rule; this branch just avoids a pointless
      // request when the composer is between tools.
      if (typed && !hasAttachments && streamingTool && activeRunId) {
        await steerActiveRun(typed);
      } else if (hasAttachments) {
        notify(t("thread.steerTextOnly"), "info");
      }
      return;
    }
    const rawText = typed || "(shared files)";
    const replyToId = replyingTo?.id ?? null;
    const draftBotId = bot?.id ?? null;

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
      pendingMessageId = tempUserMsg.id;
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
      if (draftBotId) {
        drafts.clear(threadDraftKey(draftBotId, null));
        drafts.clear(threadDraftKey(draftBotId, selectedThreadId));
      }

      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
      pendingMessageId = null;
    } catch (e: any) {
      console.error("Failed to send message:", e);
      pendingMessageId = null;
      notify(`Message not sent: ${String(e)}`, "error");
      // A failed send must not eat the draft. Put the text back and persist it
      // under the conversation that is actually open now.
      newMessage = rawText;
      if (draftBotId) {
        drafts.saveNow(threadDraftKey(draftBotId, selectedThreadId), rawText);
      }
      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
    } finally {
      pendingMessageId = null;
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

  async function loadMentionSources(botId: string) {
    if (!botId || mentionSourcesBotId === botId) return;
    mentionSourcesBotId = botId;
    try {
      const files = (await invoke<any[]>("list_file_changes", { botId, limit: 200 })) || [];
      let paths = [...new Set(files.map((file) => String(file.path || "")).filter(Boolean))];
      if (paths.length === 0) {
        const recent = (await invoke<any[]>("list_recent_file_changes", { limit: 200 })) || [];
        paths = [...new Set(recent.map((file) => String(file.path || "")).filter(Boolean))];
      }
      fileMentions = paths.slice(0, 100).map((path) => ({
        id: path,
        label: path,
        detail: "file",
        kind: "file" as const,
      }));
    } catch {
      fileMentions = [];
    }
    try {
      const skills = (await invoke<any[]>("list_all_skills")) || [];
      skillMentions = skills
        .slice(0, 100)
        .map((skill: any) => ({
          id: String(skill.id || ""),
          label: String(skill.id || ""),
          detail: String(skill.description || ""),
          kind: "skill" as const,
        }))
        .filter((item: MentionItem) => item.id);
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
    if (!token || token.sigil === "@") {
      closeMentions();
      return;
    }
    const source = token.sigil === "#" ? fileMentions : skillMentions;
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

  function chooseMention(item: MentionItem) {
    if (!mentionToken) return;
    const applied = applyMention(newMessage, mentionToken, item.label);
    newMessage = applied.text;
    closeMentions();
    if (bot?.id) drafts.save(threadDraftKey(bot.id, selectedThreadId), applied.text);
    void tick().then(() => {
      textareaRef?.focus();
      textareaRef?.setSelectionRange(applied.caret, applied.caret);
    });
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

  function handleTextareaInput(e: Event) {
    const target = e.target as HTMLTextAreaElement;
    target.style.height = "auto";
    target.style.height = Math.min(target.scrollHeight, 160) + "px";
    if (bot?.id) {
      drafts.save(threadDraftKey(bot.id, selectedThreadId), target.value);
    }
    updateMentions(target.value, target.selectionStart ?? target.value.length);
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

<div class="flex flex-col h-full overflow-hidden bg-[var(--surface-0)] text-[var(--text-primary)] font-sans relative">
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

            <div class="border-t border-[var(--hairline)] px-2 pt-2 pb-1 space-y-1">
              <button
                type="button"
                class="w-full flex items-center gap-2 rounded-xl px-2 py-1.5 text-xs transition-colors cursor-pointer disabled:opacity-50 {worktree?.enabled
                  ? 'text-[var(--brand-text)]'
                  : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                onclick={() => void toggleWorktreeIsolation()}
                disabled={!selectedThreadId || worktreeBusy}
                aria-pressed={Boolean(worktree?.enabled)}
                title="Requires a clean Git repository with one project folder"
              >
                <GitBranch class="size-3.5 shrink-0" />
                <span class="min-w-0 flex-1 text-left">Isolated Git worktree</span>
                <span class="font-mono text-[11px]">{worktree?.enabled ? "On" : "Off"}</span>
              </button>
              <p class="px-2 text-[11px] leading-snug text-[var(--text-muted)]">
                Runs write to a separate checkout until you review and apply them. Requires a clean Git repository; ignored local files are copied for reading, not applied.
              </p>
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
          onclick={() => (threadSearchOpen ? closeSearch() : void openSearch())}
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

      {#if selectedThreadId && sessionRole.role === "owner"}
        <!-- Read-only public conversation link -->
        <button
          type="button"
          class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {shareOpen ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
          onclick={() => void openShareDialog()}
          aria-pressed={shareOpen}
          aria-label="Share conversation"
          title="Share conversation"
        >
          <Share2 class="size-3.5" />
        </button>
      {/if}

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

      {#if activeRunId}
        <button
          type="button"
          class="size-7 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] flex items-center justify-center text-[var(--danger-text)] hover:bg-[var(--danger-soft-hover)] transition-colors cursor-pointer"
          onclick={stopActiveRun}
          aria-label="Stop the active run"
          title="Stop the active run"
        >
          <XCircle class="size-3.5" />
        </button>
      {/if}

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

    <!-- Chat Messages Stream -->
    <div class="flex flex-1 overflow-hidden bg-[var(--surface-0)]">
      <div class="flex flex-1 flex-col overflow-hidden">
      <div class="relative flex-1 min-h-0">
      <div bind:this={messagesContainer} onscroll={handleFeedScroll} aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown" class="virtual-scroll h-full overflow-y-auto px-3 pt-2 pb-6 {staticEntries ? 'entries-static' : ''}">
        <div class="max-w-[720px] mx-auto">
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
          <VirtualTranscript
            items={visibleMessages}
            container={messagesContainer}
            itemKey={messageKey}
            estimate={estimateMessageHeight}
            resetKey={selectedThreadId ?? ""}
            bind:target={virtualTarget}
            pin={virtualPin}
            onLayout={handleVirtualLayout}
          >
            {#snippet row(message, index)}
            {@const mi = visibleStart + index}
            {@const dayKey = messageDayKey(message.created_at)}
            {@const previousDayKey = mi > 0 ? messageDayKey(messages[mi - 1]?.created_at) : ""}
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

            {#if dayKey && dayKey !== previousDayKey}
              <div
                class="flex items-center gap-3 px-1 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]"
                role="separator"
                aria-label={messageDayLabel(message.created_at)}
              >
                <span class="h-px flex-1 bg-[var(--hairline)]" aria-hidden="true"></span>
                <span>{messageDayLabel(message.created_at)}</span>
                <span class="h-px flex-1 bg-[var(--hairline)]" aria-hidden="true"></span>
              </div>
            {/if}

            <ChatMessageRow
              {isUser}
              isError={isModelError}
              rowId={`message-${messageKey(message)}`}
              messageId={isPersistedMessageId(message.id) ? message.id : undefined}
              searchHit={threadSearchOpen && threadSearchMatchSet.has(messageKey(message))}
              searchCurrent={threadSearchOpen && threadCurrentSearchKey === messageKey(message)}
              replyTo={replyQuoteFor(
                messagesForReplies,
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
              pending={message.id === pendingMessageId}
              {grouped}
              showMeta={!continuesRun}
              {ghost}
              fullWidthAgent={true}
              onOpenArtifact={(code, lang) => {
                artifactStore.open(makeArtifact(code, lang));
                sidePanel.select("artifacts", sidePanel.scope());
              }}
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
            {/snippet}
          </VirtualTranscript>

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
                {#if liveImageGeneration}
                  <ImageGeneration
                    state="generating"
                    prompt={String((liveImageGeneration.args as any)?.prompt ?? "")}
                    size={String((liveImageGeneration.args as any)?.size ?? "")}
                  />
                {/if}
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
            <InteractionBlockCard
              botName={bot.name}
              header={q.header}
              question={q.question}
              options={q.options || []}
              allowCustom={q.allow_custom !== false}
              kind={q.kind || "question"}
              payload={q.payload || null}
              answering={answeringQuestion === q.id}
              onAnswer={(answer) => answerQuestion(q.id, answer)}
            />
          {/each}
        </div>
        </div>
        <MessageSelectionToolbar
          container={messagesContainer}
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
        <TranscriptRail
          container={messagesContainer}
          entries={railEntries}
          positions={railPositions}
          onJump={jumpToRail}
        />
      </div>

      {#if worktree?.active}
        <div class="px-4 pb-1 shrink-0">
          <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto flex items-center gap-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] px-3 py-2">
            <GitBranch class="size-3.5 shrink-0 text-[var(--brand-text)]" />
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2 text-xs font-medium text-[var(--text-primary)]">
                <span>Isolated worktree</span>
                <span class="shrink-0 rounded border border-[var(--hairline)] px-1.5 py-0.5 font-mono text-[11px] text-[var(--text-tertiary)]">
                  not applied
                </span>
              </div>
              <p class="truncate text-[11px] text-[var(--text-muted)]">
                Runs in this conversation write to a Git checkout. Review and apply them to the project.
              </p>
            </div>
            <button
              type="button"
              class="h-7 px-2.5 rounded-lg border border-[var(--hairline)] text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer disabled:opacity-50 flex items-center gap-1.5"
              onclick={() => void openWorktreeReview()}
              disabled={worktreeBusy}
            >
              {#if worktreeBusy}<Loader2 class="size-3 animate-spin" />{/if}
              Review
            </button>
          </div>
        </div>
      {/if}
      {#if worktreeError && !worktreeReview}
        <div class="px-4 pb-1 shrink-0">
          <p class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-1.5 text-[11px] text-[var(--danger)]">
            {worktreeError}
          </p>
        </div>
      {/if}

      {#if threadGoal?.goal}
        <div class="px-4 pb-1 shrink-0">
          <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto flex items-center gap-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] px-3 py-2">
            <Target class="size-3.5 shrink-0 text-[var(--brand-text)]" />
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2 text-xs font-medium text-[var(--text-primary)]">
                <span class="truncate">{threadGoal.goal.objective}</span>
                <span class="shrink-0 rounded border border-[var(--hairline)] px-1.5 py-0.5 font-mono text-[11px] text-[var(--text-tertiary)]">
                  {threadGoal.goal.status} · {threadGoal.goal.rounds_used}/{threadGoal.goal.max_rounds}{threadGoal.goal.contract?.max_tokens > 0
                    ? ` · ${Number(threadGoal.goal.tokens_used || 0).toLocaleString()}/${Number(threadGoal.goal.contract.max_tokens).toLocaleString()} tok`
                    : ""}
                </span>
              </div>
              {#if threadGoal.goal.last_reason}
                <p class="truncate text-[11px] text-[var(--text-muted)]">{threadGoal.goal.last_reason}</p>
              {/if}
            </div>
            {#if threadGoal.goal.status === "active"}
              <button type="button" class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center" onclick={() => void threadGoalAction("pause")} title="Pause goal" aria-label="Pause goal" disabled={goalBusy}>
                <Pause class="size-3.5" />
              </button>
            {:else if threadGoal.goal.status === "paused" || threadGoal.goal.status === "blocked"}
              <button type="button" class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center" onclick={() => void resumeThreadGoal(false)} title="Resume goal" aria-label="Resume goal" disabled={goalBusy}>
                <Play class="size-3.5" />
              </button>
            {/if}
            {#if !["done", "cancelled", "failed"].includes(threadGoal.goal.status)}
              <button type="button" class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--danger)] hover:bg-[var(--surface-3)] flex items-center justify-center" onclick={() => void threadGoalAction("cancel")} title="Stop goal" aria-label="Stop goal" disabled={goalBusy}>
                <X class="size-3.5" />
              </button>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Grok Floating Capsule Composer -->
      <div class="p-4 bg-[var(--surface-0)] shrink-0">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="relative max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]  p-3 shadow-2xl transition-all duration-200 focus-within:border-[var(--brand)]"
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
      {#if mentionOpen}
        <ComposerMentionMenu
          sigil={mentionSigil}
          items={mentionItems}
          activeIndex={mentionIndex}
          onPick={chooseMention}
        />
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
          if (handleMentionKeydown(e)) return;
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
              {#if selectedThreadId}
                <button
                  type="button"
                  class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {threadGoal
                    ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/50'
                    : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                  onclick={openGoalDialog}
                  aria-pressed={Boolean(threadGoal)}
                  title="Run a persistent goal in this conversation"
                >
                  <Target class="size-3" />
                  <span>Goal</span>
                </button>
              {/if}
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
                disabled={(!newMessage.trim() && pendingAttachments.length === 0) || (runActive && !canSteer)}
                aria-label={canSteer ? t("thread.steer") : t("home.send")}
                class="size-8 rounded-full flex items-center justify-center transition-all duration-200 cursor-pointer {(newMessage.trim() || pendingAttachments.length) && (!runActive || canSteer)
                  ? 'btn-brand text-[var(--text-primary)] hover:scale-105 active:scale-95'
                  : 'bg-[var(--surface-3)] text-[var(--text-muted)] cursor-not-allowed'}"
                title={canSteer ? t("thread.steerTitle") : t("home.send")}
              >
                {#if runActive && !canSteer}
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

<!-- Persistent 1:1 Goal Modal -->
{#if shareOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-[var(--scrim)] flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (shareOpen = false)}
  >
    <div
      class="modal-panel w-full max-w-xl p-5 space-y-3 max-h-[86vh] flex flex-col"
      role="dialog"
      aria-modal="true"
      aria-labelledby="thread-share-title"
      tabindex="-1"
      bind:this={shareDialogEl}
      onclick={(event) => event.stopPropagation()}
      onkeydown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          shareOpen = false;
        }
      }}
    >
      <div class="flex items-start justify-between gap-3 border-b border-[var(--hairline)] pb-2 shrink-0">
        <div class="min-w-0">
          <h2 id="thread-share-title" class="text-sm font-semibold text-[var(--text-primary)] flex items-center gap-1.5">
            <Share2 class="size-4 text-[var(--brand-text)]" />
            Share conversation
          </h2>
          <p class="mt-0.5 text-[11px] text-[var(--text-muted)]">
            Read-only snapshot link. Anyone with the link can read those messages until it expires or is revoked.
            Tool calls, reasoning, attachments, approvals and questions are never included.
          </p>
        </div>
        <button
          type="button"
          class="size-6 rounded-md hover:bg-[var(--surface-3)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer text-xs font-mono shrink-0"
          onclick={() => (shareOpen = false)}
          aria-label="Close share dialog"
        >
          ✕
        </button>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto space-y-3 pr-1">
        <p class="text-[11px] text-[var(--text-tertiary)] leading-relaxed">
          The link is served by the local Remote access listener at
          <code class="font-mono text-[var(--text-secondary)]">{shareStatus?.url ?? "…"}</code>.
          Messages added after the link is created are not included. If Remote access is bound to
          loopback, the link only works on this machine or through a tunnel.
        </p>

        {#if !shareStatus}
          <p class="text-[11px] text-[var(--text-muted)]">Checking Remote access…</p>
        {:else if !shareStatus.running}
          <div class="rounded-lg border border-[var(--warning-border)] bg-[var(--warning-soft)] px-3 py-2 text-[11px] text-[var(--warning-text)]" role="alert">
            Remote access is not running. Enable it in Settings → Remote access, then reopen this dialog.
          </div>
        {:else}
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-[11px] text-[var(--text-muted)]">Link lifetime</span>
            {#each [{ hours: 24, label: "24 hours" }, { hours: 24 * 7, label: "7 days" }, { hours: 24 * 30, label: "30 days" }] as option}
              <button
                type="button"
                aria-pressed={shareTtl === option.hours}
                class="h-6 px-2 rounded-md border text-[11px] font-mono cursor-pointer transition-colors {shareTtl === option.hours ? 'bg-[var(--brand-soft)] border-[var(--brand)]/40 text-[var(--brand-text)]' : 'bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
                onclick={() => (shareTtl = option.hours)}
              >
                {option.label}
              </button>
            {/each}
            <Button
              size="sm"
              class="ml-auto h-7 px-3 text-[11px] gap-1"
              disabled={shareBusy}
              onclick={() => void createShareLink()}
            >
              {#if shareBusy}<Loader2 class="size-3 animate-spin" />{/if}
              Create link
            </Button>
          </div>

          {#if shareLink}
            <div class="rounded-lg border border-success/30 bg-success/10 p-2.5 space-y-1.5">
              <p class="text-[11px] text-success">Copy this link now — it is not shown again.</p>
              <div class="flex items-center gap-2">
                <code class="min-w-0 flex-1 break-all font-mono text-[11px] text-[var(--text-primary)]">{shareLink.url}</code>
                <button
                  type="button"
                  class="size-6 shrink-0 rounded-md border border-success/40 bg-success/15 text-success flex items-center justify-center cursor-pointer"
                  onclick={() => void copyShareLink()}
                  aria-label="Copy share link"
                  title="Copy share link"
                >
                  {#if shareCopied}<Check class="size-3" />{:else}<Copy class="size-3" />{/if}
                </button>
              </div>
              <p class="text-[11px] text-[var(--text-muted)]">Expires {formatShareTime(shareLink.expires_at)}.</p>
            </div>
          {/if}
        {/if}

        {#if shareError}
          <p class="rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger)] whitespace-pre-wrap" role="alert">
            {shareError}
          </p>
        {/if}

        <div class="space-y-1.5">
          <h3 class="text-[11px] font-bold text-[var(--text-secondary)]">Existing links</h3>
          {#if shareList.length === 0}
            <p class="text-[11px] text-[var(--text-muted)]">No links have been created for this conversation.</p>
          {:else}
            {#each shareList as share (share.id)}
              {@const active = shareIsActive(share)}
              <div class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] p-2 space-y-1">
                <div class="flex items-center gap-2">
                  <span class="text-[11px] font-mono {active ? 'text-success' : 'text-[var(--text-muted)]'}">
                    {share.revoked_at ? "revoked" : active ? "active" : "expired"}
                  </span>
                  <span class="text-[11px] text-[var(--text-muted)]">created {formatShareTime(share.created_at)}</span>
                  <span class="ml-auto text-[11px] text-[var(--text-muted)]">{share.view_count} view{share.view_count === 1 ? "" : "s"}</span>
                  {#if active}
                    <button
                      type="button"
                      class="text-[11px] text-[var(--danger)] hover:underline cursor-pointer"
                      onclick={() => void revokeShare(share)}
                    >
                      Revoke
                    </button>
                  {/if}
                </div>
                <div class="flex flex-wrap gap-x-3 text-[11px] text-[var(--text-muted)]">
                  <span>expires {formatShareTime(share.expires_at)}</span>
                  {#if share.last_viewed_at}<span>last viewed {formatShareTime(share.last_viewed_at)}</span>{/if}
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

{#if goalOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-[var(--scrim)] flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (goalOpen = false)}
  >
    <div
      class="modal-panel w-full max-w-lg p-5 space-y-3 max-h-[85vh] overflow-y-auto"
      onclick={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-label="Persistent goal"
      tabindex="-1"
    >
      <div class="flex items-start justify-between gap-3 border-b border-[var(--hairline)] pb-2">
        <div>
          <h2 class="text-sm font-semibold text-[var(--text-primary)] flex items-center gap-1.5">
            <Target class="size-4 text-[var(--brand-text)]" />
            Persistent goal
          </h2>
          <p class="mt-0.5 text-[11px] text-[var(--text-muted)]">
            The agent keeps planning, acting and verifying in this thread until the goal is done or the round budget ends.
          </p>
        </div>
        <button
          type="button"
          class="size-6 rounded-md hover:bg-[var(--surface-3)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer text-xs font-mono"
          onclick={() => (goalOpen = false)}
          aria-label="Close goal dialog"
        >
          ✕
        </button>
      </div>

      {#if threadGoal?.goal}
        <div class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] p-3 space-y-1">
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[var(--text-primary)]">Current goal</span>
            <span class="font-mono text-[11px] text-[var(--text-tertiary)]">
              {threadGoal.goal.status} · {threadGoal.goal.rounds_used}/{threadGoal.goal.max_rounds}
            </span>
          </div>
          <p class="text-xs text-[var(--text-secondary)]">{threadGoal.goal.objective}</p>
          {#if threadGoal.goal.last_reason}
            <p class="text-[11px] text-[var(--text-muted)]">{threadGoal.goal.last_reason}</p>
          {/if}
          {#if threadGoal.goal.status === "paused" || threadGoal.goal.status === "blocked"}
            <div class="grid grid-cols-2 gap-3 pt-1">
              <div>
                <label class="block text-[11px] font-medium text-[var(--text-secondary)]" for="thread-goal-resume-rounds">
                  Rounds on resume
                </label>
                <input
                  id="thread-goal-resume-rounds"
                  bind:value={goalRounds}
                  type="number"
                  min="1"
                  max="64"
                  class="mt-1 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] focus:outline-none focus:border-[var(--brand)]/50"
                />
              </div>
              <div>
                <label class="block text-[11px] font-medium text-[var(--text-secondary)]" for="thread-goal-resume-tokens">
                  Token budget (0 = unlimited)
                </label>
                <input
                  id="thread-goal-resume-tokens"
                  bind:value={goalTokens}
                  type="number"
                  min="0"
                  step="1000"
                  class="mt-1 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] focus:outline-none focus:border-[var(--brand)]/50"
                />
              </div>
            </div>
          {/if}
          <div class="flex gap-2 pt-1">
            {#if threadGoal.goal.status === "active"}
              <Button size="sm" variant="outline" disabled={goalBusy} onclick={() => void threadGoalAction("pause")}>Pause</Button>
            {:else if threadGoal.goal.status === "paused" || threadGoal.goal.status === "blocked"}
              <Button size="sm" variant="outline" disabled={goalBusy} onclick={() => void resumeThreadGoal(true)}>Resume</Button>
            {/if}
            {#if !["done", "cancelled", "failed"].includes(threadGoal.goal.status)}
              <Button size="sm" variant="outline" disabled={goalBusy} onclick={() => void threadGoalAction("cancel")}>Stop</Button>
            {/if}
          </div>
        </div>
      {/if}

      {#if !threadGoal?.goal || ["done", "cancelled", "failed"].includes(threadGoal.goal.status)}
        <div class="space-y-2">
          <label class="block text-[11px] font-medium text-[var(--text-secondary)]" for="thread-goal-objective">
            What should the agent accomplish?
          </label>
          <textarea
            id="thread-goal-objective"
            bind:this={goalObjectiveRef}
            bind:value={goalObjective}
            rows={3}
            maxlength={4000}
            placeholder="Finish the feature, run its tests, and leave the workspace in a working state."
            class="w-full resize-none rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--brand)]/50"
          ></textarea>

          <label class="block text-[11px] font-medium text-[var(--text-secondary)]" for="thread-goal-outcome">
            Evidence that counts as done (optional)
          </label>
          <input
            id="thread-goal-outcome"
            bind:value={goalOutcome}
            placeholder="The test suite passes and the change is recorded."
            class="w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--brand)]/50"
          />

          <label class="block text-[11px] font-medium text-[var(--text-secondary)]" for="thread-goal-gate">
            Verification command (optional)
          </label>
          <input
            id="thread-goal-gate"
            bind:value={goalGate}
            placeholder="python3 -m pytest -q tests"
            spellcheck="false"
            class="w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 font-mono text-xs text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--brand)]/50"
          />

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-[11px] font-medium text-[var(--text-secondary)]" for="thread-goal-rounds">
                Automatic rounds
              </label>
              <input
                id="thread-goal-rounds"
                bind:value={goalRounds}
                type="number"
                min="1"
                max="64"
                class="mt-1 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] focus:outline-none focus:border-[var(--brand)]/50"
              />
            </div>
            <div>
              <label class="block text-[11px] font-medium text-[var(--text-secondary)]" for="thread-goal-tokens">
                Token budget (0 = unlimited)
              </label>
              <input
                id="thread-goal-tokens"
                bind:value={goalTokens}
                type="number"
                min="0"
                step="1000"
                class="mt-1 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] focus:outline-none focus:border-[var(--brand)]/50"
              />
            </div>
          </div>
        </div>
      {/if}

      {#if goalError}
        <p class="rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger)]" role="alert">
          {goalError}
        </p>
      {/if}

      <div class="flex items-center justify-end gap-2 border-t border-[var(--hairline)] pt-3">
        <Button size="sm" variant="ghost" onclick={() => (goalOpen = false)}>Cancel</Button>
        {#if !threadGoal?.goal || ["done", "cancelled", "failed"].includes(threadGoal.goal.status)}
          <Button size="sm" disabled={goalBusy || !goalObjective.trim()} onclick={() => void startThreadGoal()}>
            {goalBusy ? "Starting…" : "Start goal"}
          </Button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- Isolated Worktree Review Modal -->
{#if worktreeReview}
  {@const review = worktreeReview}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-[var(--scrim)] flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (worktreeReview = null)}
  >
    <div
      class="modal-panel w-full max-w-3xl p-5 space-y-3 max-h-[86vh] flex flex-col"
      role="dialog"
      aria-modal="true"
      aria-labelledby="worktree-review-title"
      tabindex="-1"
      bind:this={worktreeReviewEl}
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-start justify-between gap-3 border-b border-[var(--hairline)] pb-2 shrink-0">
        <div class="min-w-0">
          <h2 id="worktree-review-title" class="text-sm font-semibold text-[var(--text-primary)] flex items-center gap-1.5">
            <GitBranch class="size-4 text-[var(--brand-text)]" />
            Isolated worktree
          </h2>
          <p class="mt-0.5 text-[11px] text-[var(--text-muted)]">
            {review.note || (review.status === "active"
              ? "These changes are not in your project yet."
              : "This worktree is finished.")}
          </p>
        </div>
        <button
          type="button"
          class="size-6 rounded-md hover:bg-[var(--surface-3)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer text-xs font-mono shrink-0"
          onclick={() => (worktreeReview = null)}
          aria-label="Close worktree review"
        >
          ✕
        </button>
      </div>

      {#if review.files.length}
        <div class="rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 max-h-28 overflow-auto shrink-0">
          {#each review.files as file}
            <p class="font-mono text-[11px] text-[var(--text-secondary)] break-all">{file}</p>
          {/each}
        </div>
      {/if}

      {#if review.diff}
        <div class="min-h-0 flex-1 overflow-auto rounded-lg border border-[var(--hairline)] bg-[var(--surface-0)]">
          <DiffView diff={review.diff} />
        </div>
        {#if review.truncated}
          <p class="shrink-0 text-[11px] text-[var(--warning-text)]">
            The preview is truncated; Apply uses the complete change.
          </p>
        {/if}
      {:else}
        <p class="text-xs text-[var(--text-muted)]">
          {review.note || "The isolated checkout has no changes."}
        </p>
      {/if}

      {#if worktreeError}
        <p class="shrink-0 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger)]" role="alert">
          {worktreeError}
        </p>
      {/if}

      <div class="flex items-center justify-end gap-2 border-t border-[var(--hairline)] pt-3 shrink-0">
        <Button size="sm" variant="ghost" onclick={() => (worktreeReview = null)}>Close</Button>
        {#if review.status === "active"}
          <Button size="sm" variant="outline" disabled={worktreeBusy} onclick={() => void discardWorktree()}>
            Discard
          </Button>
          <Button size="sm" disabled={worktreeBusy} onclick={() => void applyWorktree()}>
            {worktreeBusy ? "Working…" : "Apply to project"}
          </Button>
        {/if}
      </div>
    </div>
  </div>
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

