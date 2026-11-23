<script lang="ts">
  /**
   * The in-flight delegation edge, shown where the reader is about to type.
   *
   * A handoff is not the same thing as a tool call: one agent has asked another
   * agent to work, and the answer comes back later. The row names the target
   * and the instruction, so "asked Sam" never looks like "the lead went quiet".
   */
  import { t } from "$lib/i18n";
  import { Check, Loader2, X } from "@lucide/svelte";
  import type { Handoff } from "$lib/handoffs.svelte";

  interface Props {
    items: Handoff[];
    nameFor: (botId: string) => string;
    onOpen?: (handoff: Handoff) => void;
  }

  let { items, nameFor, onOpen }: Props = $props();

  function stateOf(handoff: Handoff): "working" | "replied" | "failed" {
    if (handoff.error) return "failed";
    return handoff.done ? "replied" : "working";
  }

  function stateLabel(handoff: Handoff): string {
    const state = stateOf(handoff);
    if (state === "failed") return t("room.awaitingFailed");
    if (state === "replied") return t("room.awaitingReplied");
    return t("room.awaitingWorking");
  }
</script>

{#if items.length > 0}
  <section class="awaiting-replies" aria-live="polite" aria-label={t("room.awaitingTitle")}>
    <header class="awaiting-replies__head">
      <Loader2 class="size-3.5 animate-spin text-[var(--brand-text)]" />
      <span class="awaiting-replies__title">{t("room.awaitingTitle")}</span>
      <span class="awaiting-replies__count">{items.length}</span>
    </header>
    <ul class="awaiting-replies__list">
      {#each items as handoff (handoff.key)}
        {@const state = stateOf(handoff)}
        <li class="awaiting-reply" data-state={state}>
          <span class="awaiting-reply__icon" aria-hidden="true">
            {#if state === "failed"}
              <X class="size-3.5" strokeWidth={2.2} />
            {:else if state === "replied"}
              <Check class="size-3.5" strokeWidth={2.2} />
            {:else}
              <Loader2 class="size-3.5 animate-spin" />
            {/if}
          </span>
          <span class="awaiting-reply__copy">
            <span class="awaiting-reply__name">{nameFor(handoff.toBotId) || handoff.toBotName}</span>
            <span class="awaiting-reply__instruction" title={handoff.error || handoff.instruction}>
              {handoff.error || handoff.instruction}
            </span>
          </span>
          <span class="awaiting-reply__state">{stateLabel(handoff)}</span>
          {#if handoff.childThreadId && onOpen}
            <button
              type="button"
              class="awaiting-reply__open"
              onclick={() => onOpen(handoff)}
            >
              {t("room.openThread")}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
