<script lang="ts">
  import type { Snippet } from "svelte";
  // Shared chat row (OpenBot ChatMessageRow): the single shell both the
  // 1:1 thread and the office channel render — gutter/avatar run headers,
  // bubble variants (user / agent / ghost), error-card slot, and the
  // hover-revealed meta line. "Everything is a prop" on purpose so the two
  // views can never drift apart again.
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import ReasoningPanel from "$lib/components/ReasoningPanel.svelte";
  import ToolTraceList from "$lib/components/ToolTraceList.svelte";
  import type { ToolTrace } from "$lib/types";
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
    errorCard,
    actions,
    aboveBubble,
    belowBubble,
    userExtras,
    onOpenArtifact,
  }: Props = $props();
</script>

<div
  use:entrance
  class="message-entry flex gap-3.5 {isUser ? 'justify-end' : 'justify-start'} group"
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

  <div class="min-w-0 space-y-1.5 {isUser ? '' : fullWidthAgent ? 'w-full' : 'max-w-[min(80%,720px)]'}">
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
        />
        <!--
          Thinking, then doing, then the conclusion. The tool list goes between
          them because it *is* the middle of that sentence: it is what turned the
          reasoning into the answer, and burying it after the bubble reads as a
          footnote instead of as the work.
        -->
        <ToolTraceList {tools} />
        {@render aboveBubble?.()}
        <div use:smoothHeight class={ghost ? "msg-bubble msg-bubble-ghost" : "msg-bubble msg-bubble-agent selection:bg-[var(--brand-soft)]"}>
          <MarkdownRenderer content={text} streamTail={streamTail} {onOpenArtifact} />
        </div>
        {@render belowBubble?.()}
      </div>
    {/if}

    {#if showMeta && time}
      <div class="message-meta flex items-center gap-0.5 px-3 {isUser ? 'justify-end' : 'justify-start'}">
        <span class="message-time">{time}</span>
        {@render actions?.()}
      </div>
    {/if}
  </div>

  {#if isUser && userAvatar}
    <div class="size-8 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--hairline-strong)] shrink-0 mt-1 shadow-sm">
      <img src={userAvatar} alt="You" class="size-full object-cover" />
    </div>
  {/if}
</div>
