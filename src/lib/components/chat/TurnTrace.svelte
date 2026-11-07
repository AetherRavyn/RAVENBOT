<script lang="ts">
  /**
   * A turn's tool work as one disclosure: a one-line summary, a proportional
   * timeline, and the existing per-tool detail list.
   *
   * The raw tool list was already useful but sat between reasoning and the
   * answer as an undifferentiated stack. The summary answers "how much work was
   * this?" before the reader opens anything, and the timeline shows which call
   * dominated the turn. Model/memory spans can be added to the same surface
   * when the runtime emits them.
   */
  import { t } from "$lib/i18n";
  import type { ToolTrace, TraceSpan } from "$lib/types";
  import ToolTraceList from "$lib/components/ToolTraceList.svelte";
  import TraceWaterfall from "$lib/components/chat/TraceWaterfall.svelte";
  import { Activity, Check, ChevronRight, X } from "@lucide/svelte";

  interface Props {
    tools: ToolTrace[];
    spans?: TraceSpan[];
    reasoning?: string | null;
    class?: string;
  }

  let { tools, spans = [], reasoning = null, class: customClass = "" }: Props = $props();

  let open = $state(false);
  let traceOpen = $state(false);

  function duration(ms: number | null | undefined): string | null {
    if (ms == null || ms <= 0) return null;
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
  }

  const timeline = $derived.by(() => {
    if (spans.length > 0) {
      return spans.map((span, index) => ({
        id: span.id || `span-${index}`,
        kind: span.kind,
        label: span.label,
        started: Math.max(0, span.started_ms),
        duration: Math.max(0, span.duration_ms),
        failed: span.failed === true,
        detail: span.detail ?? null,
      }));
    }
    let cursor = 0;
    return tools.map((tool, index) => {
      const ms = Math.max(0, tool.duration_ms ?? 0);
      const entry = {
        id: `tool-${index}`,
        kind: "tool",
        label: tool.name,
        started: cursor,
        duration: ms,
        failed: tool.is_error === true,
        detail: null,
      };
      cursor += ms;
      return entry;
    });
  });

  const failed = $derived(timeline.filter((span) => span.failed).length);
  const endMs = $derived(Math.max(1, ...timeline.map((span) => span.started + span.duration)));
  const totalMs = $derived(timeline.reduce((sum, span) => sum + span.duration, 0));
  const stepCount = $derived(
    spans.length > 0 ? spans.length : tools.length + (reasoning ? 1 : 0),
  );
  const summary = $derived(
    duration(totalMs)
      ? t("trace.summary", { count: stepCount, duration: duration(totalMs)! })
      : t("trace.summaryNoTime", { count: stepCount }),
  );

  /** Each span's start and width relative to the whole measured turn. */
  const bars = $derived.by(() =>
    timeline.map((span) => ({
      span,
      left: (span.started / endMs) * 100,
      width: Math.max(span.duration > 0 ? 1.5 : 0, (span.duration / endMs) * 100),
    })),
  );
</script>

{#if stepCount > 0}
  <div class={"turn-trace " + customClass}>
    <button
      type="button"
      class="turn-trace__head"
      aria-expanded={open}
      title={open ? t("trace.hide") : t("trace.show")}
      onclick={() => (open = !open)}
    >
      <Activity class="size-3.5 shrink-0" strokeWidth={2} />
      <span class="turn-trace__summary">{summary}</span>
      {#if failed > 0}
        <span class="turn-trace__failed">
          <X class="size-2.5 shrink-0" strokeWidth={2.5} />
          {t("trace.failed", { count: failed })}
        </span>
      {:else}
        <span class="turn-trace__ok">
          <Check class="size-2.5 shrink-0" strokeWidth={2.5} />
        </span>
      {/if}
      <ChevronRight class={"turn-trace__chev size-3 shrink-0 " + (open ? "turn-trace__chev--open" : "")} strokeWidth={2} />
    </button>

    {#if open}
      <div class="turn-trace__body">
        {#if reasoning}
          <div class="turn-trace__reasoning">
            <span class="turn-trace__reasoning-label">{t("trace.reasoning")}</span>
            <span class="turn-trace__reasoning-text">{reasoning.length > 180 ? `${reasoning.slice(0, 180)}…` : reasoning}</span>
          </div>
        {/if}
        {#if tools.length > 0}
          <div class="turn-trace__timeline-head">
            <span>{t("trace.waterfall")}</span>
            <button type="button" class="turn-trace__open" onclick={() => (traceOpen = true)}>
              {t("trace.open")}
            </button>
          </div>
          <div class="turn-trace__timeline" aria-label={t("trace.waterfall")}>
            {#each bars as bar (bar.span.id)}
              <div class="turn-trace__bar-row" data-failed={bar.span.failed} data-kind={bar.span.kind}>
                <span
                  class="turn-trace__bar-label"
                  title={bar.span.detail || bar.span.label}
                >
                  <span class="turn-trace__bar-name">{bar.span.label}</span>
                  {#if bar.span.detail}
                    <span class="turn-trace__bar-detail">{bar.span.detail}</span>
                  {/if}
                </span>
                <span class="turn-trace__bar-track">
                  <span
                    class="turn-trace__bar"
                    style={`left:${bar.left}%;width:${bar.width}%`}
                  ></span>
                </span>
                <span class="turn-trace__bar-ms">{duration(bar.span.duration) ?? "—"}</span>
              </div>
            {/each}
          </div>
          <ToolTraceList {tools} />
        {/if}
      </div>
    {/if}
  </div>
  <TraceWaterfall bars={bars} open={traceOpen} onClose={() => (traceOpen = false)} />
{/if}
