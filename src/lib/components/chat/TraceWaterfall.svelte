<script lang="ts">
  /**
   * The full-screen view of a turn's trace. The inline TurnTrace keeps the
   * compact timeline; this dialog gives a long turn enough width to compare
   * spans without squeezing labels into the message column.
   */
  import { t } from "$lib/i18n";
  import { X } from "@lucide/svelte";

  export interface TraceBar {
    span: {
      id: string;
      kind: string;
      label: string;
      duration: number;
      failed: boolean;
      detail?: string | null;
    };
    left: number;
    width: number;
  }

  interface Props {
    bars: TraceBar[];
    open: boolean;
    onClose: () => void;
  }

  let { bars, open, onClose }: Props = $props();
  let dialog = $state<HTMLDialogElement | null>(null);

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });

  function duration(ms: number): string {
    if (ms <= 0) return "—";
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
  }
</script>

<dialog
  bind:this={dialog}
  class="trace-waterfall"
  aria-label={t("trace.title")}
  oncancel={(event) => {
    event.preventDefault();
    onClose();
  }}
  onclick={(event) => {
    if (event.target === dialog) onClose();
  }}
>
  <div class="trace-waterfall__panel">
    <header class="trace-waterfall__header">
      <div>
        <h2>{t("trace.title")}</h2>
        <p>{t("trace.waterfall")}</p>
      </div>
      <button type="button" class="trace-waterfall__close" aria-label={t("trace.close")} onclick={onClose}>
        <X class="size-4" />
      </button>
    </header>

    <div class="trace-waterfall__body">
      {#each bars as bar (bar.span.id)}
        <div class="trace-waterfall__row" data-failed={bar.span.failed} data-kind={bar.span.kind}>
          <span class="trace-waterfall__label" title={bar.span.detail || bar.span.label}>
            <span class="trace-waterfall__name">{bar.span.label}</span>
            {#if bar.span.detail}
              <span class="trace-waterfall__detail">{bar.span.detail}</span>
            {/if}
          </span>
          <span class="trace-waterfall__track">
            <span class="trace-waterfall__bar" style={`left:${bar.left}%;width:${bar.width}%`}></span>
          </span>
          <span class="trace-waterfall__ms">{duration(bar.span.duration)}</span>
        </div>
      {/each}
    </div>
  </div>
</dialog>
