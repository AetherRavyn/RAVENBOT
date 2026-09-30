<script lang="ts">
  import { trapFocus } from '$lib/a11y';
  import type { Snippet } from 'svelte';

  interface Props {
    active: boolean;
    children: Snippet;
  }

  let { active, children }: Props = $props();
  let container: HTMLElement;
  let cleanup: (() => void) | null = null;

  $effect(() => {
    if (active && container) {
      cleanup = trapFocus(container);
    } else if (cleanup) {
      cleanup();
      cleanup = null;
    }
    
    return () => {
      cleanup?.();
    };
  });
</script>

<div bind:this={container} role="dialog" aria-modal={active} aria-hidden={!active} tabindex="-1">
  {@render children()}
</div>

<style>
  /* Focus trap styles */
  :global(.focus-trap-sr) {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
  }
</style>
