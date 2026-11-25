<script lang="ts">
  /**
   * The floating composer capsule at the foot of a conversation.
   *
   * Everything here is presentational: the owning view keeps the draft, the
   * parked approvals/questions, voice and toggle state, and every action.
   * The textarea element and draft are bound so parent focus/selection and
   * programmatic inserts (mentions, quotes, dictation) keep working.
   */
  import {
    ArrowUp,
    Brain,
    Ghost,
    Globe,
    Loader2,
    Mic,
    Paperclip,
    Pencil,
    Reply,
    ShieldAlert,
    Target,
    Volume2,
    X,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import type { PendingAttachment } from "$lib/attachments";
  import type { ReplyQuote } from "$lib/chat/replies";
  import type { MentionItem, MentionSigil } from "$lib/chat/mentions";
  import ComposerMentionMenu from "./ComposerMentionMenu.svelte";
  import ContextMeter from "./ContextMeter.svelte";

  interface Props {
    botName: string;
    modelLabel: string;
    threadId?: string | null;

    /** Bound so the parent can focus the field and place the caret. */
    draft?: string;
    textarea?: HTMLTextAreaElement | null;

    attachments?: PendingAttachment[];
    activeReply?: ReplyQuote | null;
    editing?: boolean;
    hasPendingApprovals?: boolean;
    hasPendingQuestions?: boolean;

    mentionOpen?: boolean;
    mentionSigil?: MentionSigil;
    mentionItems?: MentionItem[];
    mentionIndex?: number;

    deepSearchActive?: boolean;
    thinkActive?: boolean;
    tempActive?: boolean;
    isListening?: boolean;
    voiceMode?: boolean;
    speaking?: boolean;
    autoRead?: boolean;

    showGoal?: boolean;
    goalActive?: boolean;

    runActive?: boolean;
    canSteer?: boolean;
    sendDisabled?: boolean;
    sendReady?: boolean;

    onInput: (event: Event) => void;
    onPaste: (event: ClipboardEvent) => void;
    onDrop: (event: DragEvent) => void;
    onKeydown: (event: KeyboardEvent) => void;
    onSend: () => void;
    onCancelReply: () => void;
    onRemoveAttachment: (index: number) => void;
    onMentionPick: (item: MentionItem) => void;
    onCancelEdit: () => void;
    onToggleDeepSearch: () => void;
    onToggleThink: () => void;
    onOpenGoal: () => void;
    onAttachFile: () => void;
    onToggleVoice: () => void;
    onToggleVoiceMode: () => void;
    onToggleAutoRead: () => void;
    onToggleTemp: () => void;
  }

  let {
    botName,
    modelLabel,
    threadId = null,
    draft = $bindable(""),
    textarea = $bindable(null),
    attachments = [],
    activeReply = null,
    editing = false,
    hasPendingApprovals = false,
    hasPendingQuestions = false,
    mentionOpen = false,
    mentionSigil = "#",
    mentionItems = [],
    mentionIndex = 0,
    deepSearchActive = false,
    thinkActive = false,
    tempActive = false,
    isListening = false,
    voiceMode = false,
    speaking = false,
    autoRead = false,
    showGoal = false,
    goalActive = false,
    runActive = false,
    canSteer = false,
    sendDisabled = true,
    sendReady = false,
    onInput,
    onPaste,
    onDrop,
    onKeydown,
    onSend,
    onCancelReply,
    onRemoveAttachment,
    onMentionPick,
    onCancelEdit,
    onToggleDeepSearch,
    onToggleThink,
    onOpenGoal,
    onAttachFile,
    onToggleVoice,
    onToggleVoiceMode,
    onToggleAutoRead,
    onToggleTemp,
  }: Props = $props();
</script>

<!-- Grok Floating Capsule Composer -->
<div class="p-4 bg-[var(--surface-0)] shrink-0">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="relative max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]  p-3 shadow-2xl transition-all duration-200 focus-within:border-[var(--brand)]"
    ondragover={(e) => e.preventDefault()}
    ondrop={onDrop}
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
          onclick={() => onCancelReply()}
          aria-label={t("thread.cancelReply")}
          title={t("thread.cancelReply")}
        >
          <X class="size-3.5" />
        </button>
      </div>
    {/if}
    <!-- Pending attachments (images as thumbnails, docs as chips) -->
    {#if attachments.length}
      <div class="flex flex-wrap gap-1.5 pb-1.5">
        {#each attachments as att, idx}
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
                onclick={() => onRemoveAttachment(idx)}
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
                onclick={() => onRemoveAttachment(idx)}
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

    {#if hasPendingApprovals}
      <div role="alert" class="mb-2 flex items-center gap-2 rounded-xl border border-warning/30 bg-warning/10 px-3 py-2 text-[11px] text-warning">
        <ShieldAlert class="size-3.5 shrink-0" />
        <span>Waiting on your approval — answer above to resume {botName}.</span>
      </div>
    {/if}
    {#if hasPendingQuestions}
      <div role="status" class="mb-2 flex items-center gap-2 rounded-xl border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-3 py-2 text-[11px] text-[var(--brand-text)]">
        <span class="size-1.5 rounded-full bg-[var(--brand)] animate-pulse shrink-0"></span>
        <span>{botName} is waiting for your answer — reply to the question above.</span>
      </div>
    {/if}
    {#if mentionOpen}
      <ComposerMentionMenu
        sigil={mentionSigil}
        items={mentionItems}
        activeIndex={mentionIndex}
        onPick={onMentionPick}
      />
    {/if}
    <textarea
      bind:this={textarea}
      bind:value={draft}
      aria-label="Message"
      oninput={onInput}
      onpaste={onPaste}
      placeholder={hasPendingApprovals ? "Answer the approval above first…" : hasPendingQuestions ? "Answer the question above…" : `Ask anything, run code, or attach images to ${botName}...`}
      disabled={hasPendingApprovals || hasPendingQuestions}
      rows={1}
      class="w-full bg-transparent text-xs sm:text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none min-h-[44px] max-h-40 leading-relaxed font-sans"
      onkeydown={onKeydown}
    ></textarea>

    <!-- Edit-and-resend banner -->
    {#if editing}
      <div class="flex items-center justify-between pt-2 border-t border-warning/20 mt-1">
        <div class="flex items-center gap-1.5 text-[11px] font-mono text-warning">
          <Pencil class="size-3" />
          <span>{t("thread.editingHint")}</span>
        </div>
        <button
          type="button"
          class="text-[11px] font-mono text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
          onclick={() => onCancelEdit()}
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
          onclick={() => onToggleDeepSearch()}
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
          onclick={() => onToggleThink()}
          aria-pressed={thinkActive}
          title={t("home.think")}
        >
          <Brain class="size-3" />
          <span>{t("home.thinkPill")}</span>
        </button>

        <ContextMeter threadId={threadId} class="ml-1" />
        {#if showGoal}
          <button
            type="button"
            class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {goalActive
              ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/50'
              : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
            onclick={() => onOpenGoal()}
            aria-pressed={goalActive}
            title="Run a persistent goal in this conversation"
          >
            <Target class="size-3" />
            <span>Goal</span>
          </button>
        {/if}
        <button
          type="button"
          class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center transition-colors cursor-pointer"
          onclick={() => onAttachFile()}
          aria-label={t("home.attach")}
          title={t("home.attach")}
        >
          <Paperclip class="size-3.5" />
        </button>

        <button
          type="button"
          class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {isListening ? 'text-danger bg-danger/20 border border-danger/50 animate-pulse shadow-sm' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
          onclick={() => onToggleVoice()}
          aria-pressed={isListening}
          aria-label="Voice input"
          title={isListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
        >
          <Mic class="size-3.5" />
        </button>

        <button
          type="button"
          class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {voiceMode ? 'text-success bg-success/20 border border-success/50 shadow-sm' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'} {speaking ? 'animate-pulse' : ''}"
          onclick={() => onToggleVoiceMode()}
          aria-pressed={voiceMode}
          aria-label="Voice mode"
          title={voiceMode ? "Voice mode on — click to stop (hands-free loop)" : "Voice mode: hands-free talk → response spoken aloud"}
        >
          <Volume2 class="size-3.5" />
        </button>

        <button
          type="button"
          class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {autoRead
            ? 'bg-success/15 text-success border-success/40'
            : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
          onclick={() => onToggleAutoRead()}
          aria-pressed={autoRead}
          title={autoRead ? "Auto-read ON — replies are spoken aloud" : "Auto-read: speak every reply aloud"}
        >
          <Volume2 class="size-3" />
          <span>{t("thread.autoRead")}</span>
        </button>

        <button
          type="button"
          class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {tempActive ? 'text-warning bg-warning/15 border border-warning/40' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
          onclick={() => onToggleTemp()}
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
          {modelLabel}
        </span>

        <button
          type="button"
          onclick={() => onSend()}
          disabled={sendDisabled}
          aria-label={canSteer ? t("thread.steer") : t("home.send")}
          class="size-8 rounded-full flex items-center justify-center transition-all duration-200 cursor-pointer {sendReady
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
