<script lang="ts">
  /**
   * Render a Mermaid diagram.
   *
   * Mermaid is heavy (~1 MB gzipped), so it is loaded on demand: the first
   * diagram on a page pays the load cost, and subsequent ones reuse the
   * cached module. A diagram that fails to parse shows its source as text
   * rather than taking the whole message down.
   */
  import { onMount } from "svelte";

  interface Props {
    code: string;
  }

  let { code }: Props = $props();

  let svg = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);

  onMount(() => {
    let cancelled = false;
    (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({ startOnLoad: false, theme: "dark", securityLevel: "strict" });
        const id = `mermaid-${Math.random().toString(36).slice(2)}`;
        const { svg: rendered } = await mermaid.render(id, code);
        if (!cancelled) {
          svg = rendered;
          loading = false;
        }
      } catch (e) {
        if (!cancelled) {
          error = e instanceof Error ? e.message : String(e);
          loading = false;
        }
      }
    })();
    return () => { cancelled = true; };
  });
</script>

<div class="md-mermaid">
  {#if loading}
    <div class="md-mermaid-loading">Rendering diagram…</div>
  {:else if error}
    <pre class="md-mermaid-error">{code}</pre>
    <p class="md-mermaid-errtext">{error}</p>
  {:else if svg}
    {@html svg}
  {/if}
</div>
