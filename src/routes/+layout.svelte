<script lang="ts">
  import "../app.css";
  import "katex/dist/katex.min.css";
  import type { Snippet } from "svelte";
  import ImageLightbox from "$lib/components/chat/ImageLightbox.svelte";

  let { children }: { children: Snippet } = $props();
</script>

{@render children?.()}
<ImageLightbox />
