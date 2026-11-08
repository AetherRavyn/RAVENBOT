<script lang="ts">
  /**
   * A markdown table, inside a horizontal scroll container.
   *
   * The scroll wrapper is written here rather than applied afterwards, which
   * matters for two reasons: a wide table no longer widens the message column
   * and squeezes the conversation, and the container is focusable, so a
   * keyboard user can scroll it — an `overflow-x: auto` div that cannot be
   * reached by Tab is a scroll region nobody but a mouse can use.
   */
  import { onDestroy } from "svelte";
  import type { Tokens } from "marked";
  import MarkdownInline from "./MarkdownInline.svelte";
  import { lexInline } from "$lib/markdown/lex";
  import { t } from "$lib/i18n";
  import { tableToCsv, tableToMarkdown } from "$lib/markdown/tableExport";
  import { Check, Copy } from "@lucide/svelte";

  interface Props {
    table: Tokens.Table;
    streaming?: boolean;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let { table, streaming = false, onOpenArtifact }: Props = $props();

  const aligns = $derived(
    table.align.map((a) => a ?? "left"),
  );

  type TableFormat = "markdown" | "csv";
  let copied = $state<TableFormat | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | null = null;

  onDestroy(() => {
    if (copiedTimer) clearTimeout(copiedTimer);
  });

  function exportTable(format: TableFormat) {
    const source = {
      headers: table.header.map((cell) => cell.text),
      rows: table.rows.map((row) => row.map((cell) => cell.text)),
    };
    const text = format === "csv" ? tableToCsv(source) : tableToMarkdown(source);
    void navigator.clipboard?.writeText(text).then(
      () => {
        copied = format;
        if (copiedTimer) clearTimeout(copiedTimer);
        copiedTimer = setTimeout(() => (copied = null), 1600);
      },
      () => {
        /* Clipboard access can be denied; the table remains selectable. */
      },
    );
  }
</script>

<div class="md-table-card">
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<section class="md-table-scroll" aria-label={t("markdown.table")} tabindex="0">
  <table
    class="md-table"
    class:md-table--pair={table.header.length === 2}
    class:md-table--multi={table.header.length >= 3}
    style={`--md-cols:${table.header.length}`}
  >
    <thead>
      <tr>
        {#each table.header as cell, i (i)}
          <th style="text-align: {aligns[i] ?? 'left'}">
            <MarkdownInline tokens={lexInline(cell.text)} {onOpenArtifact} />
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each table.rows as row, ri (ri)}
        <tr>
          {#each row as cell, ci (ci)}
            <td style="text-align: {aligns[ci] ?? 'left'}">
              <MarkdownInline tokens={lexInline(cell.text)} {streaming} {onOpenArtifact} />
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</section>
{#if !streaming}
  <div class="md-table-actions">
    <button
      type="button"
      class="md-table-action"
      aria-label="Copy table as Markdown"
      onclick={() => exportTable("markdown")}
    >
      {#if copied === "markdown"}
        <Check class="size-3" />
        Copied
      {:else}
        <Copy class="size-3" />
        Markdown
      {/if}
    </button>
    <button
      type="button"
      class="md-table-action"
      aria-label="Copy table as CSV"
      onclick={() => exportTable("csv")}
    >
      {#if copied === "csv"}
        <Check class="size-3" />
        Copied
      {:else}
        <Copy class="size-3" />
        CSV
      {/if}
    </button>
  </div>
{/if}
</div>
