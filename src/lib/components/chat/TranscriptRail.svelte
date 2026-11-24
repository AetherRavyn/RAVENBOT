<script lang="ts">
  import { onDestroy } from "svelte";
  import { t } from "$lib/i18n";
  import {
    fraction,
    viewportBand,
    type RailEntry,
  } from "$lib/chat/scrollRail";

  interface Props {
    container: HTMLElement | null;
    entries?: RailEntry[];
    /**
     * Virtual-transcript offsets, as percentages of the scroll container.
     * When a row is unmounted there is no DOM element to measure, so the
     * virtualizer supplies the same position it uses for its spacer.
     */
    positions?: Record<string, number>;
    onJump: (id: string) => void;
  }

  let { container, entries = [], positions = {}, onJump }: Props = $props();

  interface RailTick extends RailEntry {
    top: number;
  }

  let ticks = $state<RailTick[]>([]);
  let viewportTop = $state(0);
  let viewportHeight = $state(0);
  let visible = $state(false);
  let frame = 0;
  let observer: ResizeObserver | null = null;

  function measure() {
    if (!container) {
      visible = false;
      return;
    }
    const total = container.scrollHeight;
    if (total <= 0) {
      visible = false;
      return;
    }
    const containerRect = container.getBoundingClientRect();
    const next: RailTick[] = [];
    for (const entry of entries) {
      const known = positions[entry.id];
      if (typeof known === "number" && Number.isFinite(known)) {
        next.push({ ...entry, top: Math.min(100, Math.max(0, known)) });
        continue;
      }
      const element = document.getElementById(entry.id);
      if (!element) continue;
      const rect = element.getBoundingClientRect();
      const offset = rect.top - containerRect.top + container.scrollTop;
      next.push({ ...entry, top: fraction(offset, total) * 100 });
    }
    ticks = next;
    const band = viewportBand(
      container.scrollTop,
      container.clientHeight,
      total,
    );
    viewportTop = band.top * 100;
    viewportHeight = Math.max(2, band.height * 100);
    visible = next.length > 0;
  }

  function schedule() {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(measure);
  }

  $effect(() => {
    container;
    entries;
    positions;
    schedule();
    if (!container) return;
    container.addEventListener("scroll", schedule, { passive: true });
    observer = new ResizeObserver(schedule);
    observer.observe(container);
    return () => {
      container.removeEventListener("scroll", schedule);
      observer?.disconnect();
      observer = null;
      cancelAnimationFrame(frame);
    };
  });

  onDestroy(() => {
    cancelAnimationFrame(frame);
    observer?.disconnect();
  });
</script>

{#if visible}
  <div
    class="transcript-rail"
    aria-label={t("thread.scrollRail")}
    role="navigation"
  >
    <div class="transcript-rail-track">
      <div
        class="transcript-rail-viewport"
        style={`top:${viewportTop}%;height:${viewportHeight}%`}
        aria-hidden="true"
      ></div>
      {#each ticks as tick (tick.id)}
        <button
          type="button"
          class="transcript-rail-tick transcript-rail-tick--{tick.kind}"
          style={`top:${tick.top}%`}
          title={tick.label}
          aria-label={tick.label}
          onclick={() => onJump(tick.id)}
        ></button>
      {/each}
    </div>
  </div>
{/if}
