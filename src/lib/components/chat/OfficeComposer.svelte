<script lang="ts">
  /**
   * The office composer capsule.
   *
   * The room owns the draft, the queue, the mention state and every decision
   * these controls trigger. This component only lays the capsule out: the plan
   * summary and in-flight handoffs above the input, the reply/attachment chips,
   * the mention menu, the textarea and the send toolbar.
   *
   * Draft and textarea ref are bindable because the room focuses and rewrites
   * them from actions that live outside the composer (reply, quote, voice,
   * mention insertion), exactly as it did when this markup was inline.
   */
  import { t } from "$lib/i18n";
  import {
    ArrowDown,
    ArrowUp,
    ListOrdered,
    Loader2,
    Mic,
    Paperclip,
    Pause,
    Play,
    Reply,
    X,
  } from "@lucide/svelte";
  import ComposerMentionMenu from "./ComposerMentionMenu.svelte";
  import ContextMeter from "./ContextMeter.svelte";
  import OfficeTaskCard from "./OfficeTaskCard.svelte";
  import AwaitingReplies from "./AwaitingReplies.svelte";
  import type { BoardNode } from "./OfficeBoard.svelte";
  import type { Handoff } from "$lib/handoffs.svelte";
  import type { PendingAttachment } from "$lib/attachments";
  import type { MentionItem, MentionSigil } from "$lib/chat/mentions";
  import type { ReplyQuote } from "$lib/chat/replies";
  import type { ContextUsage } from "$lib/chat/contextUsage.svelte";

  interface OfficeQueueItem {
    id: string;
    content: string;
    visible: string;
    attachment_names: string[];
    state: "pending" | "running" | "failed";
    error?: string | null;
    created_at: string;
    started_at?: string | null;
  }

  interface Props {
    nodes: BoardNode[];
    handoffs: Handoff[];
    queueItems: OfficeQueueItem[];
    queuePaused: boolean;
    queueBusy: boolean;
    nameFor: (botId: string) => string;
    onOpenNode: (node: BoardNode) => void;
    onOpenHandoff: (handoff: Handoff) => void;
    onToggleQueue: () => void;
    onRunQueueNext: (item: OfficeQueueItem) => void;
    canMoveQueueItem: (item: OfficeQueueItem, delta: -1 | 1) => boolean;
    onMoveQueueItem: (item: OfficeQueueItem, delta: -1 | 1) => void;
    onRemoveQueueItem: (item: OfficeQueueItem) => void;
    reply: ReplyQuote | null;
    onCancelReply: () => void;
    attachments: PendingAttachment[];
    onRemoveAttachment: (index: number) => void;
    addressee: { id: string; label: string } | null;
    onClearAddressee: () => void;
    mentionOpen: boolean;
    mentionSigil: MentionSigil;
    mentionItems: MentionItem[];
    mentionIndex: number;
    onPickMention: (item: MentionItem) => void;
    draft?: string;
    composerRef?: HTMLTextAreaElement | null;
    onInput: (event: Event) => void;
    onKeydown: (event: KeyboardEvent) => void;
    sending: boolean;
    memberCount: number;
    roomName: string;
    contextUsage: ContextUsage | null;
    isListening: boolean;
    onAttach: () => void;
    onToggleVoice: () => void;
    onSend: () => void;
  }

  let {
    nodes,
    handoffs,
    queueItems,
    queuePaused,
    queueBusy,
    nameFor,
    onOpenNode,
    onOpenHandoff,
    onToggleQueue,
    onRunQueueNext,
    canMoveQueueItem,
    onMoveQueueItem,
    onRemoveQueueItem,
    reply,
    onCancelReply,
    attachments,
    onRemoveAttachment,
    addressee,
    onClearAddressee,
    mentionOpen,
    mentionSigil,
    mentionItems,
    mentionIndex,
    onPickMention,
    draft = $bindable(""),
    composerRef = $bindable(null),
    onInput,
    onKeydown,
    sending,
    memberCount,
    roomName,
    contextUsage,
    isListening,
    onAttach,
    onToggleVoice,
    onSend,
  }: Props = $props();
</script>

<div class="p-4 bg-[var(--surface-0)] shrink-0">
  <div class="relative max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]  p-3 shadow-2xl transition-all duration-200 focus-within:border-[var(--brand)]">
    {#if nodes.length > 0}
      <OfficeTaskCard
        nodes={nodes}
        nameFor={nameFor}
        onOpen={onOpenNode}
      />
    {/if}
    <AwaitingReplies
      items={handoffs}
      nameFor={nameFor}
      onOpen={onOpenHandoff}
    />
    {#if queueItems.length > 0 || queuePaused}
      <div class="office-queue" aria-label={t("room.queueTitle", { count: queueItems.length })}>
        <div class="office-queue-head">
          <span class="office-queue-title">
            <ListOrdered class="size-3.5" />
            {t("room.queueTitle", { count: queueItems.length })}
            {#if queuePaused}<span class="office-queue-paused">{t("room.queuePaused")}</span>{/if}
          </span>
          <button
            type="button"
            class="office-queue-toggle"
            disabled={queueBusy}
            onclick={() => void onToggleQueue()}
          >
            {#if queuePaused}<Play class="size-3" />{:else}<Pause class="size-3" />{/if}
            {queuePaused ? t("room.queueResume") : t("room.queuePause")}
          </button>
        </div>
        {#if queueItems.length === 0}
          <p class="office-queue-empty">{t("room.queueEmpty")}</p>
        {:else}
          {#each queueItems as item, index (item.id)}
            <div class="office-queue-item" data-state={item.state}>
              <span class="office-queue-index">{index + 1}</span>
              {#if item.state === "pending"}
                <span class="office-queue-move">
                  <button
                    type="button"
                    class="office-queue-move-button"
                    aria-label={t("room.queueRunNext")}
                    title={t("room.queueRunNext")}
                    disabled={queueBusy}
                    onclick={() => void onRunQueueNext(item)}
                  >
                    <Play class="size-3" />
                  </button>
                  <button
                    type="button"
                    class="office-queue-move-button"
                    aria-label={t("room.queueMoveUp")}
                    title={t("room.queueMoveUp")}
                    disabled={!canMoveQueueItem(item, -1)}
                    onclick={() => void onMoveQueueItem(item, -1)}
                  >
                    <ArrowUp class="size-3" />
                  </button>
                  <button
                    type="button"
                    class="office-queue-move-button"
                    aria-label={t("room.queueMoveDown")}
                    title={t("room.queueMoveDown")}
                    disabled={!canMoveQueueItem(item, 1)}
                    onclick={() => void onMoveQueueItem(item, 1)}
                  >
                    <ArrowDown class="size-3" />
                  </button>
                </span>
              {/if}
              <span class="office-queue-text" title={item.visible || item.content}>
                {item.visible || item.content}
              </span>
              {#if item.attachment_names?.length}
                <span class="office-queue-attachments" title={item.attachment_names.join(", ")}>
                  <Paperclip class="size-3" />{item.attachment_names.length}
                </span>
              {/if}
              {#if item.state === "running"}
                <span class="office-queue-state">
                  <Loader2 class="size-3 animate-spin" />{t("room.queueRunning")}
                </span>
              {:else if item.state === "failed"}
                <span class="office-queue-state office-queue-state--failed" title={item.error || ""}>
                  {t("room.queueFailed")}
                </span>
                <button
                  type="button"
                  class="office-queue-remove"
                  aria-label={t("room.queueCancel")}
                  title={t("room.queueCancel")}
                  disabled={queueBusy}
                  onclick={() => void onRemoveQueueItem(item)}
                >✕</button>
              {:else}
                <button
                  type="button"
                  class="office-queue-remove"
                  aria-label={t("room.queueCancel")}
                  title={t("room.queueCancel")}
                  disabled={queueBusy}
                  onclick={() => void onRemoveQueueItem(item)}
                >✕</button>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    {/if}
    {#if reply}
      <div class="composer-reply">
        <Reply class="composer-reply__icon size-3.5" strokeWidth={1.9} />
        <div class="composer-reply__copy">
          <span class="composer-reply__author">
            {reply.author
              ? t("thread.replyingTo", { name: reply.author })
              : t("thread.replyingToMessage")}
          </span>
          {#if reply.text}
            <span class="composer-reply__text">{reply.text}</span>
          {/if}
        </div>
        <button
          type="button"
          class="composer-reply__close"
          onclick={onCancelReply}
          aria-label={t("thread.cancelReply")}
          title={t("thread.cancelReply")}
        >
          <X class="size-3.5" />
        </button>
      </div>
    {/if}
    {#if attachments.length}
      <div class="flex flex-wrap gap-1.5 pb-2">
        {#each attachments as att, idx}
          {#if att.isImage}
            <div class="relative size-12 rounded-lg overflow-hidden border border-[var(--hairline-strong)] bg-[var(--surface-2)]">
              <img src={`data:${att.mime};base64,${att.data}`} alt={att.name} class="size-full object-cover" />
              <button type="button" class="absolute top-0.5 right-0.5 size-4 rounded-full bg-[var(--scrim)] text-[var(--text-primary)] text-[11px] flex items-center justify-center cursor-pointer hover:bg-danger/80" onclick={() => onRemoveAttachment(idx)} title={t("room.removeAttachment")}>✕</button>
            </div>
          {:else}
            <div class="relative flex items-center gap-1.5 h-12 max-w-[200px] pl-2.5 pr-6 rounded-lg border border-[var(--hairline-strong)] bg-[var(--surface-2)]" title={att.name}>
              <Paperclip class="size-3.5 text-[var(--brand-text)] shrink-0" />
              <div class="min-w-0">
                <div class="text-[11px] text-[var(--text-secondary)] truncate">{att.name}</div>
                <div class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{att.mime.split("/").pop()}</div>
              </div>
              <button type="button" class="absolute top-1 right-1 size-4 rounded-full bg-[var(--scrim)] text-[var(--text-primary)] text-[11px] flex items-center justify-center cursor-pointer hover:bg-danger/80" onclick={() => onRemoveAttachment(idx)} aria-label={t("room.removeAttachment")} title={t("room.removeAttachment")}>✕</button>
            </div>
          {/if}
        {/each}
      </div>
    {/if}
    {#if addressee}
      <div class="composer-addressee" role="status">
        <span class="composer-addressee__sigil">@</span>
        <span class="composer-addressee__label">{t("room.addressedTo", { name: addressee.label })}</span>
        <button
          type="button"
          class="composer-addressee__clear"
          aria-label={t("room.clearAddressee")}
          title={t("room.clearAddressee")}
          onclick={onClearAddressee}
        >✕</button>
      </div>
    {/if}
    {#if mentionOpen}
      <ComposerMentionMenu
        sigil={mentionSigil}
        items={mentionItems}
        activeIndex={mentionIndex}
        onPick={onPickMention}
      />
    {/if}
    <textarea
      bind:this={composerRef}
      bind:value={draft}
      aria-label="Message"
      placeholder={memberCount === 0
        ? t("room.hireFirstPh")
        : t("room.taskPh", { name: roomName, n: memberCount })}
      rows={1}
      class="w-full bg-transparent text-xs sm:text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none min-h-[44px] max-h-40 leading-relaxed font-sans"
      oninput={onInput}
      onkeydown={onKeydown}
    ></textarea>

    <!-- Action Toolbar Inside Capsule -->
    <div class="flex items-center justify-between pt-2 border-t border-[var(--hairline)] mt-1">
      <div class="flex items-center gap-1.5">
        <button
          type="button"
          class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center transition-colors cursor-pointer"
          onclick={onAttach}
          title={t("home.attach")}
        >
          <Paperclip class="size-3.5" />
        </button>

        <button
          type="button"
          class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {isListening ? 'text-danger bg-danger/20 border border-danger/50 animate-pulse shadow-sm' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
          onclick={onToggleVoice}
          title={isListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
        >
          <Mic class="size-3.5" />
        </button>

        <ContextMeter usage={contextUsage} class="ml-1" />
        <span class="text-[11px] font-mono text-[var(--text-muted)] ml-1">
          {memberCount} Parallel Agents Assigned
        </span>
      </div>

      <button
        type="button"
        onclick={() => onSend()}
        disabled={((!draft.trim() && attachments.length === 0) || sending || memberCount === 0)}
        class="h-8 px-4 rounded-full flex items-center gap-1.5 transition-all duration-200 cursor-pointer font-semibold text-xs {(draft.trim() || attachments.length) && !sending && memberCount > 0
          ? 'btn-brand text-[var(--text-primary)] hover:scale-[1.03] active:scale-95'
          : 'bg-[var(--surface-3)] text-[var(--text-muted)] cursor-not-allowed'}"
      >
        {#if sending}
          <Loader2 class="size-3.5 animate-spin" />
          <span>{t("activity.working")}</span>
        {:else}
          <ArrowUp class="size-3.5 stroke-[2.5]" />
          <span>{t("room.dispatch")}</span>
        {/if}
      </button>
    </div>
  </div>
</div>
