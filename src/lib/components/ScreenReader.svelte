<script lang="ts">
  interface Props {
    message: string;
    priority?: 'polite' | 'assertive';
    clearAfter?: number;
  }

  let { message, priority = 'polite', clearAfter = 1000 }: Props = $props();
  // svelte-ignore state_referenced_locally
  let currentMessage = $state(message);

  $effect(() => {
    currentMessage = message;
    
    if (clearAfter > 0 && message) {
      const timeout = setTimeout(() => {
        currentMessage = '';
      }, clearAfter);
      
      return () => clearTimeout(timeout);
    }
  });
</script>

<div
  role="status"
  aria-live={priority}
  aria-atomic="true"
  class="sr-only"
>
  {currentMessage}
</div>

<style>
  :global(.sr-only) {
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
