<script lang="ts">
  /**
   * Every tool the turn ran, in order, above the answer it produced.
   *
   * This list did not exist. The schema had `ToolCall` and `ToolResult` message
   * variants since the beginning and nothing ever wrote one — so tool calls
   * lived only as a transient line in the browser, and a reload left answers
   * with no account of how they were reached: "fixed three issues" with no
   * files, no errors, no indication that any of it happened.
   *
   * Sits between the reasoning and the bubble, so the reading order is the
   * causal order: what it was thinking, what it did, what it concluded.
   *
   * Each entry is collapsed by default for one reason — most calls are noise
   * (`memory_search` six times) and the two that matter are the ones that
   * failed or took a second. Expanding is per-entry and remembered for the
   * session, because re-opening the same entry on every render is what makes a
   * disclosure useless.
   */
  import { t } from "$lib/i18n";
  import type { ToolTrace } from "$lib/types";
  import { summarizeArgs, renderToolValue } from "$lib/chat/toolSummary";
  import { ChevronRight, Check, X, Timer } from "@lucide/svelte";

  interface Props {
    tools: ToolTrace[];
    class?: string;
  }

  let { tools, class: cls = "" }: Props = $props();

  function failed(tool: ToolTrace): boolean {
    return !!tool.is_error;
  }

  /** Milliseconds as human time. `1.2s` beats `1200ms` when scanning a list. */
  function duration(tool: ToolTrace): string | null {
    const ms = tool.duration_ms;
    if (ms == null || ms <= 0) return null;
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
  }

  /** Per-entry expansion, remembered so it survives a re-render. */
  const open = $state<Record<number, boolean>>({});

  function toggle(i: number) {
    open[i] = !open[i];
  }
</script>

{#if tools.length}
  <div class={"tools " + cls} aria-label={t("tools.title")}>
    {#each tools as tool, i (i)}
      {@const bad = failed(tool)}
      {@const ms = duration(tool)}
      <div class="tools__item" data-failed={bad}>
        <button
          type="button"
          class="tools__head"
          aria-expanded={open[i] ?? false}
          onclick={() => toggle(i)}
        >
          <ChevronRight class="tools__chev size-3 shrink-0" strokeWidth={2} />
          {#if bad}
            <X class="tools__state size-3 shrink-0" strokeWidth={2.5} />
          {:else}
            <Check class="tools__state size-3 shrink-0" strokeWidth={2.5} />
          {/if}
          <span class="tools__name">{tool.name}</span>
          {#if summarizeArgs(tool.arguments)}
            <span class="tools__args">{summarizeArgs(tool.arguments)}</span>
          {/if}
          {#if ms}
            <span class="tools__ms">
              <Timer class="size-2.5 shrink-0" strokeWidth={2} />
              {ms}
            </span>
          {/if}
        </button>

        {#if open[i] ?? false}
          <div class="tools__body">
            {#if summarizeArgs(tool.arguments)}
              <div class="tools__block">
                <span class="tools__label">{t("tools.input")}</span>
                <pre class="tools__pre">{renderToolValue(tool.arguments)}</pre>
              </div>
            {/if}
            {#if renderToolValue(tool.result)}
              <div class="tools__block">
                <span class="tools__label" data-failed={bad}>
                  {bad ? t("tools.failed") : t("tools.output")}
                </span>
                <pre class="tools__pre">{renderToolValue(tool.result)}</pre>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}
