<script lang="ts">
  import "../app.css";
  import "katex/dist/katex.min.css";
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
</script>

{@render children?.()}
