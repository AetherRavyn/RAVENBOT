<script lang="ts">
  /** Search results and file listings as a compact result card. */
  import { ListFilter, Search } from "@lucide/svelte";
  import type { ToolTrace } from "$lib/types";
  import { searchCard } from "$lib/chat/toolCards";

  interface Props {
    tool: ToolTrace;
    expanded?: boolean;
  }

  let { tool, expanded = false }: Props = $props();
  const card = $derived(searchCard(tool));
</script>

{#if card}
  <div class="tool-card tool-card--search" data-expanded={expanded}>
    <div class="tool-card__bar">
      {#if card.entries}
        <ListFilter class="size-3.5 shrink-0 text-[var(--brand-text)]" strokeWidth={2} />
      {:else}
        <Search class="size-3.5 shrink-0 text-[var(--brand-text)]" strokeWidth={2} />
      {/if}
      <span class="tool-card__title">
        {#if card.pattern}
          <code>{card.pattern}</code>
        {:else}
          Listing
        {/if}
      </span>
      <span class="tool-card__badge">{card.count} result{card.count === 1 ? "" : "s"}</span>
      {#if card.path}<span class="tool-card__meta">{card.path}</span>{/if}
      {#if card.truncated}
        <span class="tool-card__badge" data-warn="true">truncated</span>
      {/if}
    </div>
    {#if card.matches.length}
      <div class="tool-card__matches" data-expanded={expanded}>
        {#each card.matches as match, index (index)}
          <div class="tool-card__match">{match}</div>
        {/each}
      </div>
    {:else}
      <p class="tool-card__empty">No matches.</p>
    {/if}
  </div>
{/if}
