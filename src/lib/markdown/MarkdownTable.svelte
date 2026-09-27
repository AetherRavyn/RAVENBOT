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
  import type { Tokens } from "marked";
  import MarkdownInline from "./MarkdownInline.svelte";
  import { lexInline } from "$lib/markdown/lex";
  import { t } from "$lib/i18n";

  interface Props {
    table: Tokens.Table;
    streaming?: boolean;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let { table, streaming = false, onOpenArtifact }: Props = $props();

  const aligns = $derived(
    table.align.map((a) => a ?? "left"),
  );
</script>

<!--
  A scrollable region must be reachable by keyboard or it is not scrollable for
  anyone not using a mouse (WCAG 2.1.1, and the "scrollable region must have
  keyboard access" technique). `tabindex="0"` on a labelled region is the
  recommended fix, and svelte's noninteractive-tabindex rule does not know
  that, hence the suppression.
-->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<section class="md-table-scroll" aria-label={t("markdown.table")} tabindex="0">
  <table class="md-table">
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
