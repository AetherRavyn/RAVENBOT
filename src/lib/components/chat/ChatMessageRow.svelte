<script lang="ts">
  import { onDestroy, type Snippet } from "svelte";
  import { Check, Copy, Reply, SmilePlus } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { REACTION_PALETTE } from "$lib/chat/reactions";
  import type { ReplyQuote } from "$lib/chat/replies";
  // Shared chat row (OpenBot ChatMessageRow): the single shell both the
  // 1:1 thread and the office channel render — gutter/avatar run headers,
  // bubble variants (user / agent / ghost), error-card slot, and the
  // hover-revealed meta line. "Everything is a prop" on purpose so the two
  // views can never drift apart again.
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import ReasoningPanel from "$lib/components/ReasoningPanel.svelte";
  import TurnTrace from "$lib/components/chat/TurnTrace.svelte";
  import type { ToolTrace, TraceSpan } from "$lib/types";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import type { AvatarMood } from "$lib/avatar";
  import { entrance } from "$lib/chat/entrance";
  import { smoothHeight } from "$lib/chat/smoothHeight";

  interface Props {
    isUser: boolean;
    /** Plain text for user rows; markdown source for agent rows. */
    text: string;
    /** Streaming: newest revealed word, shown with the OpenBot blur-in tail. */
    streamTail?: string;
    /** Pre-formatted timestamp for the meta line (empty hides the line). */
    time?: string;
    /**
     * The agent's reasoning, shown above the answer.
     *
     * Only for agent rows. A user has no reasoning trace, and rendering an empty
     * panel beside every user message would put a border on half the conversation
     * to say nothing.
     */
    reasoning?: string | null;
    /**
     * Tools this turn ran, in order.
     *
     * Persisted with the message, so they survive a reload. Before this, tool
     * calls existed only as a transient line in the browser and every one of
     * them was gone on refresh — answers arrived with no account of how they
     * were reached.
     */
    tools?: ToolTrace[];
    /** Persisted model/tool/memory spans for the turn trace. */
    spans?: TraceSpan[];
    /** Thread to read live reasoning from while this row is the streaming one. */
    reasoningThreadId?: string | null;
    /** True while the agent is still working, so the panel stays open. */
    reasoningLive?: boolean;
    /** Continues the previous run — tightens spacing via [data-grouped]. */
    grouped?: boolean;
    /** Mid-run rows hide the meta line (ThreadView passes !continuesRun). */
    showMeta?: boolean;
    /** Code/table-only agent reply → borderless ghost bubble. */
    ghost?: boolean;
    /** Agent column spans the feed (1:1 thread) instead of the 80% channel cap. */
    fullWidthAgent?: boolean;
    /** hsl(...) author hue → --message-author-color (room). */
    authorColor?: string;
    /** Room: reserve the 24px gutter on agent rows. */
    gutter?: boolean;
    /** Room: whose face to draw in the gutter on the first row of an author run. */
    gutterName?: string;
    /** A custom image for the gutter avatar, if the agent has one. */
    gutterImage?: string | null;
    /** What that agent is doing, so the face reacts. */
    gutterMood?: AvatarMood;
    /** Room: name header row on the first row of an author run. */
    author?: { name: string; specialty?: string } | null;
    /** Room: 32px user avatar on the trailing side of user rows. */
    userAvatar?: string;
    /** Row carries a backend "⚠️ **Model Error:**" reply → render errorCard. */
    isError?: boolean;
    /** The message this row quotes, resolved against the transcript. */
    replyTo?: ReplyQuote | null;
    /** Start a reply to this row; absent hides the Reply control. */
    onReply?: () => void;
    /** Scroll to the quoted message. */
    onJumpToReply?: (id: string) => void;
    /** Persisted emoji reactions, oldest first. */
    reactions?: string[];
    /** Toggle a reaction on this row; absent hides the add control. */
    onReact?: (emoji: string) => void;
    /** Stable DOM id, used by in-thread search to scroll to a hit. */
    rowId?: string;
    /** This row matches the active search query. */
    searchHit?: boolean;
    /** This is the match the search is currently sitting on. */
    searchCurrent?: boolean;
    errorCard?: Snippet;
    actions?: Snippet;
    aboveBubble?: Snippet;
    belowBubble?: Snippet;
    userExtras?: Snippet;
    onOpenArtifact?: (artifact: any) => void;
  }

  let {
    isUser,
    text,
    streamTail = "",
    time = "",
    reasoning = null,
    tools = [],
    spans = [],
    reasoningThreadId = null,
    reasoningLive = false,
    grouped = false,
    showMeta = true,
    ghost = false,
    fullWidthAgent = false,
    authorColor = "",
    gutter = false,
    gutterName = "",
    gutterImage = null,
    gutterMood = "idle",
    author = null,
    userAvatar = "",
    isError = false,
    replyTo = null,
    onReply,
    onJumpToReply,
    reactions = [],
    onReact,
    rowId = "",
    searchHit = false,
    searchCurrent = false,
    errorCard,
    actions,
    aboveBubble,
    belowBubble,
    userExtras,
    onOpenArtifact,
  }: Props = $props();

  // A very long agent report is a document, not a chat bubble. Collapse it to
  // a readable height so it cannot swallow the conversation; the full answer
  // is one click away. Streaming and short replies are never collapsed.
  const COLLAPSE_AFTER = 3000;
  let expanded = $state(false);
  const collapsible = $derived(
    !isUser &&
      !isError &&
      !streamTail &&
      !reasoningLive &&
      text.trim().length > COLLAPSE_AFTER,
  );
  $effect(() => {
    // A regenerated or replaced answer starts collapsed again.
    void text;
    expanded = false;
  });

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  let reactionOpen = $state(false);
  let reactionPicker = $state<HTMLElement | null>(null);

  function chooseReaction(emoji: string) {
    reactionOpen = false;
    onReact?.(emoji);
  }

  // Close the palette on Escape or on a click outside it. A palette that only
  // closes by pressing its own button is a small trap on a long thread.
  $effect(() => {
    if (!reactionOpen) return;
    const onPointer = (event: MouseEvent) => {
      if (!reactionPicker?.contains(event.target as Node)) reactionOpen = false;
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") reactionOpen = false;
    };
    document.addEventListener("click", onPointer, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", onPointer, true);
      document.removeEventListener("keydown", onKey);
    };
  });

  onDestroy(() => {
    if (copyTimer) clearTimeout(copyTimer);
  });

  async function copyMessage() {
    if (!text.trim()) return;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 1600);
    } catch {
      /* Clipboard permission is the user's choice; leave the button at rest. */
    }
  }
</script>

{#snippet quoteCard(quote: ReplyQuote)}
  <button
    type="button"
    class="message-quote"
    onclick={() => onJumpToReply?.(quote.id)}
    aria-label={quote.author ? `Reply to ${quote.author}` : "Jump to quoted message"}
    title={quote.text || quote.author || "Quoted message"}
  >
    {#if quote.author}
      <span class="message-quote__author">{quote.author}</span>
    {/if}
    {#if quote.text}
      <span class="message-quote__text">{quote.text}</span>
    {/if}
  </button>
{/snippet}

<div
  use:entrance
  id={rowId || undefined}
  class="message-entry flex gap-3.5 {isUser ? 'justify-end' : 'justify-start'} group"
  class:message-search-hit={searchHit}
  class:message-search-hit--current={searchCurrent}
  data-grouped={grouped ? "" : undefined}
  style={authorColor ? `--message-author-color: ${authorColor}` : undefined}
>
  {#if !isUser && gutter}
    {#if gutterName}
      <!-- The gutter avatar is the agent's face, and it reacts to what they
           are doing — so a stream of messages from one agent shows one
           attentive face, not a row of static icons. -->
      <RavenAvatar
        name={gutterName}
        mood={gutterMood}
        imageUrl={gutterImage}
        decorative
        class="size-6 rounded-full shrink-0 self-end"
      />
    {:else}
      <div class="w-6 shrink-0" aria-hidden="true"></div>
    {/if}
  {/if}

  <div
    class="min-w-0 space-y-1.5 flex flex-col {isUser
      ? 'items-end flex-1'
      : fullWidthAgent
        ? 'items-start w-full'
        : 'items-start flex-1 max-w-[min(80%,720px)]'}"
  >
    {#if !isUser && author}
      <div class="text-xs px-3 flex items-center gap-1.5">
        <span style="color: var(--message-author-color)">{author.name}</span>
        {#if author.specialty}
          <span class="text-[var(--text-muted)]">· {author.specialty}</span>
        {/if}
      </div>
    {/if}

    {#if isError && errorCard}
      {@render errorCard()}
    {:else if isUser}
      {#if replyTo}
        {@render quoteCard(replyTo)}
      {/if}
      <div class="msg-bubble msg-bubble-user selection:bg-[var(--brand-soft)]">
        {text}
        {@render userExtras?.()}
      </div>
    {:else}
      <div class="space-y-3">
        <!--
          Reasoning sits above the answer, in the agent's column. Order matters:
          the panel is the evidence and the bubble is the claim, so evidence first
          reads as reasoning *toward* the answer rather than a footnote after it.
        -->
        <ReasoningPanel
          threadId={reasoningThreadId || ""}
          agentName={gutterName || ""}
          stored={reasoning}
          live={reasoningLive}
          class={reasoningLive ? "reason--live" : ""}
        />
        <!--
          Thinking, then doing, then the conclusion. The tool list goes between
          them because it *is* the middle of that sentence: it is what turned the
          reasoning into the answer, and burying it after the bubble reads as a
          footnote instead of as the work.
        -->
        <TurnTrace {tools} {spans} {reasoning} />
        {@render aboveBubble?.()}
        {#if replyTo}
          {@render quoteCard(replyTo)}
        {/if}
        <!-- An agent that is still thinking has no answer yet. Rendering the
             empty bubble anyway reserves a block of blank space and makes the
             live lane look broken; the reasoning panel already says what is
             happening. -->
        {#if text.trim() || streamTail.trim()}
          <div use:smoothHeight class={ghost ? "msg-bubble msg-bubble-ghost" : "msg-bubble msg-bubble-agent selection:bg-[var(--brand-soft)]"}>
            <div
              class="message-body"
              class:message-body--collapsed={collapsible && !expanded}
            >
              <MarkdownRenderer content={text} streamTail={streamTail} {onOpenArtifact} />
            </div>
            {#if collapsible}
              <button
                type="button"
                class="message-expand"
                aria-expanded={expanded}
                onclick={() => (expanded = !expanded)}
              >
                {expanded ? t("thread.showLess") : t("thread.showFull")}
              </button>
            {/if}
          </div>
        {/if}
        {@render belowBubble?.()}
      </div>
    {/if}

    {#if (showMeta && time) || text.trim()}
      <div class="message-meta flex items-center gap-0.5 px-3 {isUser ? 'justify-end' : 'justify-start'}">
        {#if showMeta && time}
          <span class="message-time">{time}</span>
        {/if}
        {#if onReply}
          <button
            type="button"
            class="message-action-button"
            aria-label="Reply"
            title="Reply"
            onclick={onReply}
          >
            <Reply class="size-3" strokeWidth={1.8} />
          </button>
        {/if}
        {#if text.trim()}
          <button
            type="button"
            class="message-action-button"
            aria-label={copied ? "Message copied" : "Copy message"}
            title={copied ? "Copied" : "Copy message"}
            onclick={copyMessage}
          >
            {#if copied}
              <Check class="size-3" strokeWidth={2.2} />
            {:else}
              <Copy class="size-3" strokeWidth={1.8} />
            {/if}
          </button>
        {/if}
        {#if onReact}
          <span class="message-reaction-add" bind:this={reactionPicker}>
            <button
              type="button"
              class="message-action-button"
              aria-label="Add reaction"
              title="Add reaction"
              aria-expanded={reactionOpen}
              onclick={() => (reactionOpen = !reactionOpen)}
            >
              <SmilePlus class="size-3" strokeWidth={1.8} />
            </button>
            {#if reactionOpen}
              <span class="message-reaction-palette" role="group" aria-label="Choose a reaction">
                {#each REACTION_PALETTE as emoji (emoji)}
                  <button
                    type="button"
                    class="message-reaction-choice"
                    aria-label={`React with ${emoji}`}
                    onclick={() => chooseReaction(emoji)}
                  >
                    <span aria-hidden="true">{emoji}</span>
                  </button>
                {/each}
              </span>
            {/if}
          </span>
        {/if}
        {@render actions?.()}
      </div>
    {/if}

    {#if reactions.length > 0}
      <div class="message-reactions" aria-label="Reactions">
        {#each reactions as emoji (emoji)}
          <button
            type="button"
            class="message-reaction-chip"
            aria-label={`Remove ${emoji} reaction`}
            aria-pressed="true"
            title={`Remove ${emoji} reaction`}
            onclick={() => onReact?.(emoji)}
          >
            <span aria-hidden="true">{emoji}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if isUser && userAvatar}
    <div class="size-8 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--hairline-strong)] shrink-0 mt-1 shadow-sm">
      <img src={userAvatar} alt="You" class="size-full object-cover" />
    </div>
  {/if}
</div>
