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

