<script lang="ts">
  /**
   * A compact context-window meter for the composer.
   *
   * It shows the estimated share of the model window in use and lets the user
   * ask for a compaction on the next turn. Compaction itself is per-run state,
   * so the request is queued in the runtime rather than faked here.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { notify } from "$lib/toast";
  import { contextUsage, type ContextUsage } from "$lib/chat/contextUsage.svelte";
  import { t } from "$lib/i18n";

  interface Props {
    threadId?: string | null;
    /** An office shows the latest node's context instead of one thread's. */
    usage?: ContextUsage | null;
    class?: string;
  }

  let { threadId = null, usage: explicitUsage = null, class: customClass = "" }: Props = $props();

  const usage = $derived(explicitUsage ?? contextUsage.forThread(threadId));
  const percent = $derived(
    usage && usage.windowTokens > 0
      ? Math.min(100, Math.max(0, Math.round((usage.usedTokens / usage.windowTokens) * 100)))
      : 0,
  );
  const level = $derived(percent >= 95 ? "danger" : percent >= 80 ? "warning" : "normal");
  const breakdown = $derived(
    [
      [t("composer.contextSystem"), usage?.systemTokens],
      [t("composer.contextTools"), usage?.toolTokens],
      [t("composer.contextMemory"), usage?.memoryTokens],
      [t("composer.contextMessages"), usage?.messageTokens],
    ]
      .filter((entry): entry is [string, number] => typeof entry[1] === "number")
      .map(([label, value]) => `${label} ${format(value)}`)
      .join(" · "),
  );
  const compaction = $derived(
    usage?.compacted && typeof usage.beforeTokens === "number"
      ? t("composer.contextCompactedFrom", {
          before: format(usage.beforeTokens),
          after: format(usage.usedTokens),
        })
      : "",
  );
  const title = $derived(
    [
      `${t("composer.contextTitle")} · ${format(usage?.usedTokens ?? 0)} / ${format(usage?.windowTokens ?? 0)} tokens`,
      breakdown,
      compaction,
      t("composer.contextCompact"),
    ]
      .filter(Boolean)
      .join("\n"),
  );

  function compact() {
    if (!threadId) return;
    void invoke("compact_thread_context", { threadId })
      .then(() => notify(t("composer.contextCompactQueued"), "success"))
      .catch((error) => notify(`Could not request compaction: ${String(error)}`, "error"));
  }

  function format(value: number): string {
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}m`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
    return String(value);
  }
</script>

{#if usage}
  <span class="context-meter-wrap">
    <button
      type="button"
      class="context-meter {customClass}"
      data-level={level}
      title={title}
      aria-label={`${t("composer.contextTitle")}: ${percent}%`}
      onclick={compact}
    >
      <span class="context-meter-track" aria-hidden="true">
        <span class="context-meter-fill" style={`width:${percent}%`}></span>
      </span>
      <span class="context-meter-label">{percent}%</span>
      {#if usage.compacted}<span class="context-meter-badge">{t("composer.contextCompacted")}</span>{/if}
    </button>
    {#if breakdown || compaction}
      <span class="context-meter-details" role="tooltip">
        {#if breakdown}<span>{breakdown}</span>{/if}
        {#if compaction}<span>{compaction}</span>{/if}
      </span>
    {/if}
  </span>
{/if}
