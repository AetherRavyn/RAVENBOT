<script lang="ts">
  /**
   * A compact image gallery. One tile per image, responsive columns, and a
   * click opens the shared lightbox at that image so the reader can step
   * through the rest of the message without closing it.
   */
  interface GalleryImage {
    src: string;
    alt: string;
  }

  interface Props {
    images: GalleryImage[];
    label?: string;
  }

  let { images, label = "Image gallery" }: Props = $props();

  function open(index: number) {
    window.dispatchEvent(
      new CustomEvent("ravenbot:image", {
        detail: { images, index, src: images[index]?.src, alt: images[index]?.alt },
      }),
    );
  }
</script>

<div
  class="image-gallery"
  class:image-gallery--single={images.length === 1}
  role="group"
  aria-label={label}
>
  {#each images as image, index (index)}
    <button
      type="button"
      class="image-gallery-item"
      aria-label={`Open image ${index + 1} of ${images.length}`}
      onclick={() => open(index)}
    >
      <img src={image.src} alt={image.alt} loading="lazy" decoding="async" />
    </button>
  {/each}
</div>
