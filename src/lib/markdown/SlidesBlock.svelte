<script lang="ts">
  /**
   * Render a fenced `slides` block as a compact deck.
   *
   * Slides are ordinary Markdown separated by a line containing only `---`.
   * The reader gets one slide at a time with keyboard/button navigation; the
   * stage scrolls internally so a long slide never widens or stretches the
   * message column.
   */
  import { onMount } from "svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { ChevronLeft, ChevronRight, Maximize2 } from "@lucide/svelte";

  interface Props {
    code: string;
    streaming?: boolean;
  }

  let { code, streaming = false }: Props = $props();

  const slides = $derived(
    code
      .split(/^\s*---\s*$/m)
      .map((slide) => slide.trim())
      .filter((slide) => slide.length > 0),
  );
  let index = $state(0);
  let stage = $state<HTMLDivElement | null>(null);
  let isFullscreen = $state(false);

  $effect(() => {
    if (index >= slides.length) {
      index = Math.max(0, slides.length - 1);
    }
  });

  function go(delta: number) {
    if (slides.length === 0) return;
    index = (index + delta + slides.length) % slides.length;
  }

  function keys(event: KeyboardEvent) {
    if (event.key === "ArrowLeft" || event.key === "PageUp") {
      event.preventDefault();
      go(-1);
    } else if (event.key === "ArrowRight" || event.key === "PageDown") {
      event.preventDefault();
      go(1);
    } else if (event.key === "Home") {
      event.preventDefault();
      index = 0;
    } else if (event.key === "End") {
      event.preventDefault();
      index = Math.max(0, slides.length - 1);
    }
  }

  async function present() {
    try {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
      } else if (stage) {
        await stage.requestFullscreen();
      }
    } catch {
      /* fullscreen can be denied by the desktop shell; the deck still works */
    }
  }

  onMount(() => {
    const sync = () => (isFullscreen = document.fullscreenElement === stage);
    document.addEventListener("fullscreenchange", sync);
    return () => document.removeEventListener("fullscreenchange", sync);
  });

</script>

{#if slides.length === 0}
  <figure class="md-slides md-slides--empty">
    <figcaption>{streaming ? "Building slides…" : "This slide deck is empty."}</figcaption>
  </figure>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <figure
    class="md-slides"
    aria-label="Slide deck"
    tabindex="0"
    onkeydown={keys}
    aria-keyshortcuts="ArrowLeft ArrowRight Home End"
  >
    <div
      bind:this={stage}
      class="md-slides-stage"
      role="group"
      aria-roledescription="carousel"
      aria-label="Slide {index + 1} of {slides.length}"
    >
      <div class="md-slides-page">
        <MarkdownRenderer content={slides[index]} />
      </div>
    </div>

    <div class="md-slides-nav">
      <button
        type="button"
        class="md-slides-button"
        aria-label="Previous slide"
        disabled={slides.length < 2}
        onclick={() => go(-1)}
      >
        <ChevronLeft class="size-3.5" />
      </button>
      <span class="md-slides-count" aria-live="polite">{index + 1} / {slides.length}</span>
      <button
        type="button"
        class="md-slides-button"
        aria-label="Next slide"
        disabled={slides.length < 2}
        onclick={() => go(1)}
      >
        <ChevronRight class="size-3.5" />
      </button>
      <button
        type="button"
        class="md-slides-button md-slides-present"
        aria-label={isFullscreen ? "Exit presentation" : "Present fullscreen"}
        title={isFullscreen ? "Exit presentation" : "Present fullscreen"}
        onclick={present}
      >
        <Maximize2 class="size-3.5" />
      </button>
      {#if slides.length > 1}
        <div class="md-slides-dots" aria-hidden="true">
          {#each slides as _, dot (dot)}
            <span class="md-slides-dot" class:md-slides-dot--active={dot === index}></span>
          {/each}
        </div>
      {/if}
    </div>
    <div class="md-slides-progress" aria-hidden="true">
      <span style={`width:${slides.length ? ((index + 1) / slides.length) * 100 : 0}%`}></span>
    </div>
  </figure>
{/if}
