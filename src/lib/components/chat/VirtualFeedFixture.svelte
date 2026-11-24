<script lang="ts">
  /**
   * Test-only harness for VirtualTranscript: a real scroll container supplied
   * by the test, plain rows, and a bindable reveal target so the test can ask
   * for a message the way search or a reply jump does.
   */
  import VirtualTranscript from "./VirtualTranscript.svelte";
  import { estimateMessageHeight } from "$lib/chat/virtualRows";

  interface Item {
    id: string;
    text: string;
  }

  let {
    items,
    container,
    initialRows = 8,
    gap = 0,
    target = $bindable(""),
    pin = 0,
    onLayout,
  }: {
    items: Item[];
    container: HTMLElement | null;
    initialRows?: number;
    gap?: number;
    target?: string;
    pin?: number;
    onLayout?: (layout: { tops: Record<string, number>; total: number }) => void;
  } = $props();

  function keyOf(item: Item): string {
    return item.id;
  }
</script>

<VirtualTranscript
  {items}
  {container}
  itemKey={keyOf}
  estimate={estimateMessageHeight}
  {initialRows}
  {gap}
  bind:target
  {pin}
  {onLayout}
>
  {#snippet row(item, index)}
    <div
      class="feed-row-content"
      data-test-index={index}
      data-test-key={item.id}
    >{item.text}</div>
  {/snippet}
</VirtualTranscript>
