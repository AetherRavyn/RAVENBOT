<script lang="ts">
  /**
   * Test-only fixture: mount a transcript through the same window bounds the
   * office uses. Keeping this out of the app route means the regression gate
   * exercises the real row shell without needing Tauri IPC or a database.
   */
  import ChatMessageRow from "./ChatMessageRow.svelte";
  import { transcriptWindow } from "$lib/chat/messageWindow";

  interface Message {
    id: string;
    role: string;
    text: string;
  }

  let { messages, size = 30 }: { messages: Message[]; size?: number } = $props();

  const visible = $derived(
    messages.slice(transcriptWindow(messages.length, size).start),
  );
</script>

{#each visible as message (message.id)}
  <ChatMessageRow
    isUser={message.role === "user"}
    text={message.text}
    fullWidthAgent={true}
    showMeta={false}
  />
{/each}
