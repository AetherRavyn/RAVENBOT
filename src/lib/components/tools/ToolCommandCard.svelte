<script lang="ts">
  /**
   * A shell call as a terminal card: the command, where it ran, its exit code
   * and its output. The generic trace row still exists for tools without a
   * richer shape; this is the one that answers "what did it actually run?"
   */
  import { t } from "$lib/i18n";
  import { Check, Copy } from "@lucide/svelte";
  import type { ToolTrace } from "$lib/types";
  import { commandCard } from "$lib/chat/toolCards";

  interface Props {
    tool: ToolTrace;
    expanded?: boolean;
  }

  let { tool, expanded = false }: Props = $props();
  const card = $derived(commandCard(tool));
  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyCommand() {
    if (!card?.command) return;
    try {
      await navigator.clipboard.writeText(card.command);
      copied = true;
      if (copiedTimer) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable; the command is still selectable */
    }
  }

  $effect(() => {
    return () => {
      if (copiedTimer) clearTimeout(copiedTimer);
    };
  });
</script>

{#if card}
  <div class="tool-card tool-card--command" data-expanded={expanded}>
    <div class="tool-card__bar">
      <span class="tool-card__prompt" aria-hidden="true">$</span>
      <code class="tool-card__command">{card.command || "shell command"}</code>
      <button
        type="button"
        class="tool-card__icon"
        aria-label={t("a11y.copyCommand")}
        title="Copy command"
        onclick={() => void copyCommand()}
      >
        {#if copied}<Check class="size-3" />{:else}<Copy class="size-3" />{/if}
      </button>
      <span
        class="tool-card__badge"
        data-ok={card.success && (card.exitCode === null || card.exitCode === 0)}
      >
        {#if card.exitCode !== null}
          exit {card.exitCode}
        {:else if card.success}
          ok
        {:else}
          failed
        {/if}
      </span>
    </div>
    {#if card.cwd}
      <div class="tool-card__meta">{card.cwd}</div>
    {/if}
    {#if card.stdout || card.stderr}
      {#if card.stdout}
        <pre class="tool-card__pre" data-kind="stdout">{card.stdout}</pre>
      {/if}
      {#if card.stderr}
        <pre class="tool-card__pre" data-kind="stderr">{card.stderr}</pre>
      {/if}
    {:else}
      <p class="tool-card__empty">No output.</p>
    {/if}
  </div>
{/if}
