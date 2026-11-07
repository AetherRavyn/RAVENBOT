<script lang="ts">
  /**
   * A single-image viewer for markdown images and message attachments.
   *
   * It is mounted once at the app root and listens for `ravenbot:image`
   * events, so any renderer can open an image without owning modal state.
   * The native <dialog> supplies focus trapping and Escape handling.
   */
  import { onMount } from "svelte";
  import { ChevronLeft, ChevronRight, Download, X } from "@lucide/svelte";

  interface GalleryImage {
    src: string;
    alt: string;
  }

  let dialog = $state<HTMLDialogElement | null>(null);
  let closeButton = $state<HTMLButtonElement | null>(null);
  let gallery = $state<GalleryImage[]>([]);
  let index = $state(0);
  const current = $derived(gallery[index] ?? { src: "", alt: "" });

  function open(event: Event) {
    const detail = (
      event as CustomEvent<{
        src?: string;
        alt?: string;
        images?: GalleryImage[];
        index?: number;
      }>
    ).detail;
    if (!detail) return;
    const images =
      detail.images && detail.images.length > 0
        ? detail.images.filter((image) => Boolean(image?.src))
        : detail.src
          ? [{ src: detail.src, alt: detail.alt || "" }]
          : [];
    if (images.length === 0) return;
    gallery = images;
    index = Math.min(images.length - 1, Math.max(0, Number(detail.index) || 0));
    if (!dialog?.open) dialog?.showModal();
    closeButton?.focus({ preventScroll: true });
  }

  function close() {
    if (dialog?.open) dialog.close();
    gallery = [];
    index = 0;
  }

  function go(delta: number) {
    if (gallery.length < 2) return;
    index = (index + delta + gallery.length) % gallery.length;
  }

  function download() {
    if (!current.src) return;
    const link = document.createElement("a");
    link.href = current.src;
    link.download = current.alt || "image";
    link.rel = "noopener";
    link.click();
  }

  onMount(() => {
    window.addEventListener("ravenbot:image", open);
    return () => window.removeEventListener("ravenbot:image", open);
  });
</script>

<dialog
  bind:this={dialog}
  class="image-lightbox"
  aria-label={current.alt || "Image preview"}
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  onclick={(event) => {
    if (event.target === dialog) close();
  }}
  onkeydown={(event) => {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      go(-1);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      go(1);
    }
  }}
>
  <div class="image-lightbox-body">
    <div class="image-lightbox-actions">
      {#if gallery.length > 1}
        <button
          type="button"
          class="image-lightbox-button"
          aria-label="Previous image"
          onclick={() => go(-1)}
        >
          <ChevronLeft class="size-4" />
        </button>
        <button
          type="button"
          class="image-lightbox-button"
          aria-label="Next image"
          onclick={() => go(1)}
        >
          <ChevronRight class="size-4" />
        </button>
      {/if}
      <button type="button" class="image-lightbox-button" aria-label="Download image" onclick={download}>
        <Download class="size-4" />
      </button>
      <button
        bind:this={closeButton}
        type="button"
        class="image-lightbox-button"
        aria-label="Close image"
        onclick={close}
      >
        <X class="size-4" />
      </button>
    </div>
    <img src={current.src} alt={current.alt} />
    {#if current.alt}
      <p class="image-lightbox-caption">{current.alt}</p>
    {/if}
    {#if gallery.length > 1}
      <span class="image-lightbox-count">{index + 1} / {gallery.length}</span>
    {/if}
  </div>
</dialog>
