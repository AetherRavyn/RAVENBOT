<script lang="ts">
  /**
   * The message body renderer.
   *
   * Keeps the old `{content, streamTail, onOpenArtifact}` shape so
   * `ChatMessageRow` and `ArtifactPanel` do not change, and owns the one thing
   * that is not markdown: pulling a `<think>` block out into a reasoning panel.
   *
   * What it no longer does: call `marked.parse` and inject the result with
   * `{@html}`, then walk the DOM to add copy buttons and append a tail span. The
   * body renders from tokens instead, so raw HTML in a model's answer is shown
   * as text rather than executed, and a `javascript:` link is not a link.
   */
  import MarkdownMessage from "$lib/markdown/MarkdownMessage.svelte";
  import { t } from "$lib/i18n";

  interface Props {
    content: string;
    class?: string;
    /**
     * The trailing word of an in-flight reply, shown with a fade.
     *
     * Kept for the call sites that already compute it: the body is the shown
     * prefix and this is the part still fading.
     */
    streamTail?: string;
    /** Whether the body is still being written. */
    streaming?: boolean;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let {
    content = "",
    class: customClass = "",
    streamTail = "",
    streaming = false,
    onOpenArtifact,
  }: Props = $props();

  /**
   * Separate the reasoning trace from the answer.
   *
   * Native extended thinking and the external engine paths both emit
   * `<think>…</think>`, and a model can leave one open mid-stream. An unclosed
   * block is treated as reasoning rather than answer text, because a
   * half-reasoned chain dumped at the reader is worse than a missing panel.
   */
  function splitThinking(text: string): { thinking: string | null; body: string } {
    const closed = text.match(/<think>([\s\S]*?)<\/think>/i);
    if (closed) {
      return { thinking: closed[1].trim() || null, body: text.replace(closed[0], "") };
    }
    const open = text.match(/<think>([\s\S]*)$/i);
    if (open && open.index !== undefined) {
      return { thinking: open[1].trim() || null, body: text.slice(0, open.index) };
    }
    return { thinking: null, body: text };
  }

  const parts = $derived(splitThinking(content));
  const body = $derived(parts.body);

  /** Characters at the end of the body still fading: the tail word. */
  const trailChars = $derived(streaming && streamTail ? streamTail.length + 1 : 0);
</script>

{#if parts.thinking}
  <details class="think-details" open>
    <summary class="think-summary">
      <span>{t("markdown.reasoning")}</span>
    </summary>
    <pre class="think-body">{parts.thinking}</pre>
  </details>
{/if}

<MarkdownMessage
  content={body}
  {streaming}
  {trailChars}
  class={customClass}
  {onOpenArtifact}
/>

{#if streamTail}
  <span class="stream-tail">{streamTail}</span>
{/if}
