<script lang="ts">
  /**
   * A file read as a code window with a real line-number gutter.
   *
   * The trace keeps the selected offset/limit window, so showing those numbers
   * is what makes the card auditable: a reader can tell a whole-file read from
   * a five-line excerpt without opening the file.
   */
  import { FileText } from "@lucide/svelte";
  import type { ToolTrace } from "$lib/types";
  import { readCard } from "$lib/chat/toolCards";

  interface Props {
    tool: ToolTrace;
    expanded?: boolean;
  }

  let { tool, expanded = false }: Props = $props();
  const card = $derived(readCard(tool));
  const lines = $derived(card ? card.content.split("\n") : []);
  const lastLine = $derived(
    card ? card.offset + Math.max(0, lines.length - 1) : 0,
  );
</script>

{#if card}
  <div class="tool-card tool-card--read" data-expanded={expanded}>
    <div class="tool-card__bar">
      <FileText class="size-3.5 shrink-0 text-[var(--brand-text)]" strokeWidth={2} />
      <code class="tool-card__path" title={card.path}>{card.path || "file"}</code>
      <span class="tool-card__badge">{lines.length} line{lines.length === 1 ? "" : "s"}</span>
      {#if card.totalLines}
        <span class="tool-card__meta">lines {card.offset}–{lastLine} of {card.totalLines}</span>
      {/if}
      {#if card.truncated}
        <span class="tool-card__badge" data-warn="true">truncated</span>
      {/if}
    </div>
    {#if lines.length === 1 && lines[0] === ""}
      <p class="tool-card__empty">The file is empty.</p>
    {:else}
      <div class="tool-card__code" data-expanded={expanded}>
        {#each lines as line, index (index)}
          <div class="tool-card__line">
            <span class="tool-card__line-no" aria-hidden="true">{card.offset + index}</span>
            <span class="tool-card__line-text">{line || " "}</span>
          </div>
        {/each}
      </div>
    {/if}
    {#if card.nextOffset !== null}
      <p class="tool-card__meta">More lines are available from offset {card.nextOffset}.</p>
    {/if}
  </div>
{/if}
