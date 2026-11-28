<script lang="ts">
  /**
   * The decorative layer behind a conversation transcript.
   *
   * It is deliberately a sibling *behind* the scroll container rather than a
   * background on it: the transcript keeps its normal surface for selection,
   * scrolling and virtualisation, and this layer can never receive a pointer.
   * The scrim is mixed from the active theme's canvas colour, so the same
   * backdrop stays readable in a light theme and a dark one.
   */
  import { backdropSource, type ThreadBackdrop } from "$lib/chat/backdrop";

  interface Props {
    backdrop: ThreadBackdrop | null;
  }

  let { backdrop }: Props = $props();
  let source = $derived(backdropSource(backdrop));
</script>

{#if backdrop && source}
  <div
    class="pointer-events-none absolute inset-0 z-0 overflow-hidden"
    data-thread-backdrop
    data-backdrop-kind={backdrop.kind}
    aria-hidden="true"
  >
    {#if backdrop.kind === "video"}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        class="h-full w-full object-cover"
        src={source}
        autoplay
        muted
        loop
        playsinline
        preload="metadata"
        tabindex="-1"
        disablepictureinpicture
      ></video>
    {:else}
      <img
        class="h-full w-full object-cover"
        src={source}
        alt=""
        draggable="false"
      />
    {/if}
    <div
      class="absolute inset-0"
      data-backdrop-scrim
      style="background: color-mix(in srgb, var(--surface-0) 82%, transparent);"
    ></div>
  </div>
{/if}
