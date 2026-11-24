<script lang="ts">
  /**
   * File edits as diffs, not a one-line "file_edit completed".
   *
   * The trace is already clipped, so a very large patch may be incomplete. The
   * card says what it knows: file names, addition/deletion counts, and the
   * actual hunks the trace retained. A missing diff is reported instead of
   * pretending the file list is the diff.
   */
  import { FileCode2 } from "@lucide/svelte";
  import type { ToolTrace } from "$lib/types";
  import DiffView from "$lib/components/workspace/DiffView.svelte";
  import { diffCard } from "$lib/chat/toolCards";

  interface Props {
    tool: ToolTrace;
    expanded?: boolean;
  }

  let { tool, expanded = false }: Props = $props();
  const card = $derived(diffCard(tool));
</script>

{#if card}
  <div class="tool-card tool-card--diff" data-expanded={expanded}>
    <div class="tool-card__bar">
      <FileCode2 class="size-3.5 shrink-0 text-[var(--brand-text)]" strokeWidth={2} />
      <span class="tool-card__title">
        {card.files.length} file{card.files.length === 1 ? "" : "s"} changed
      </span>
      <span class="tool-card__add">+{card.added}</span>
      <span class="tool-card__del">−{card.deleted}</span>
      {#if card.dryRun}
        <span class="tool-card__badge">dry run</span>
      {/if}
    </div>
    <div class="tool-card__files">
      {#each card.files as file, index (file.path + index)}
        <div class="tool-card__file">
          <div class="tool-card__file-head">
            <code title={file.absolutePath || file.path}>{file.path || file.absolutePath || "file"}</code>
            <span class="tool-card__add">+{file.added}</span>
            <span class="tool-card__del">−{file.deleted}</span>
          </div>
          {#if file.diff}
            <div class="tool-card__diff" data-expanded={expanded}>
              <DiffView diff={file.diff} />
            </div>
          {:else}
            <p class="tool-card__empty">The trace did not retain this file's diff.</p>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}
