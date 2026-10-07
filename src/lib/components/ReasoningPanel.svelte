<script lang="ts">
  /**
   * An agent's reasoning, live, beside its answer.
   *
   * Two things are on screen at once that used to be one indistinguishable
   * stream: what the agent is *thinking* and what it is *saying*. They are shown
   * separately because they answer different questions, and because the failure
   * mode of merging them is that the private trace gets cleared away at the next
   * tool round and reappears as markup in the middle of the user's paragraph.
   *
   * Behaviour that earns its place:
   *
   *  - **Open while thinking, collapsed once answered.** Someone watching an agent
   *    work wants the reasoning; someone reading an answer wants the answer. Making
   *    them click to see what happened, every time, is the wrong default.
   *  - **The user can override it.** An explicit open or close sticks until the
   *    next run, because being overridden by a panel that reopens itself on every
   *    message is worse than not having the control.
   *  - **Follows its own tail, not the page.** The panel scrolls independently so a
   *    long trace does not fight the conversation for scroll position.
   *  - **Nothing is animated per token.** A character-by-character caret on
   *    reasoning is unreadable and repaints the whole panel on every chunk; a
   *    marker on the live round says the same thing for one repaint.
   */
  import { t } from "$lib/i18n";
  import {
    reasoning,
    parseStoredReasoning,
    type ReasoningRound,
  } from "$lib/chat/reasoning.svelte";
  import { Brain, ChevronRight } from "@lucide/svelte";

  interface Props {
    /** Thread whose reasoning to show. */
    threadId: string;
    /** The agent's name, for the heading. */
    agentName?: string;
    /**
     * Persisted reasoning, for a finished message.
     *
     * Preferred over the live store when present: it is the version that
     * survived a reload, and a reloaded conversation should not show an empty
     * panel next to a message that plainly had reasoning.
     */
    stored?: string | null;
    /** True while this agent is still working. */
    live?: boolean;
    class?: string;
  }

  let { threadId, agentName = "", stored = null, live = false, class: cls = "" }: Props = $props();

  const liveTrace = $derived(reasoning.byThread[threadId]);
  const storedRounds = $derived(parseStoredReasoning(stored));

  /**
   * Live wins over stored while the run is in flight, because the stored copy is
   * from the *previous* run and showing it beside a live one is just confusing.
   */
  const rounds: ReasoningRound[] = $derived(
    live && liveTrace ? liveTrace.rounds : storedRounds.length ? storedRounds : liveTrace?.rounds ?? [],
  );
  const thinking = $derived(live && (liveTrace?.rounds.some((r) => r.live) ?? false));

  /** `null` means "no opinion yet", so the default can follow the run. */
  let override: boolean | null = $state(null);
  const open = $derived(override ?? (thinking || rounds.length <= 1));

  let body = $state<HTMLDivElement | null>(null);
  const roundCount = $derived(rounds.length);

  // Follow the tail while thinking, but only then: scrolling a settled trace
  // because a chunk arrived would move a panel the reader had already scrolled.
  $effect(() => {
    void roundCount;
    void thinking;
    if (!thinking || !body) return;
    body.scrollTop = body.scrollHeight;
  });

  function toggle() {
    override = !open;
  }

  const summary = $derived(
    thinking
      ? t("reasoning.live")
      : rounds.length > 1
        ? t("reasoning.rounds", { count: rounds.length })
        : t("reasoning.title"),
  );
</script>

{#if rounds.length}
  <section class="reason {cls}" data-open={open} aria-label={t("reasoning.title")}>
    <button
      type="button"
      class="reason__head"
      aria-expanded={open}
      onclick={toggle}
    >
      <Brain class="reason__icon size-3.5 shrink-0" strokeWidth={1.75} />
      <span class="reason__title">{summary}</span>
      {#if agentName}
        <span class="reason__who">{agentName}</span>
      {/if}
      {#if thinking}
        <!-- A marker, not a per-token caret: the caret repaints the whole panel
             on every chunk and makes the text unreadable while it moves. -->
        <span class="reason__pulse" aria-hidden="true"></span>
      {/if}
      <ChevronRight class="reason__chev size-3.5 shrink-0" strokeWidth={2} />
    </button>

    {#if open}
      <div class="reason__body" bind:this={body}>
        {#each rounds as round, i (i)}
          {#if i > 0}
            <div class="reason__sep">
              <span>{t("reasoning.afterTools")}</span>
            </div>
          {/if}
          <p class="reason__text">{round.text}</p>
        {/each}
        {#if thinking}
          <p class="reason__more" aria-live="polite">{t("reasoning.working")}</p>
        {/if}
      </div>
    {/if}
  </section>
{/if}
