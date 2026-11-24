<script lang="ts">
  /**
   * Render markdown from its token tree.
   *
   * Every element here comes from a `marked` token, never from an HTML string.
   * The consequences worth knowing: raw HTML in a model's output is shown as
   * escaped monospace text rather than executed, a `javascript:` link URL is
   * dropped rather than becoming an anchor, and a streamed answer only
   * re-renders the block currently being written into.
   */
  import { untrack } from "svelte";
  import type { Token } from "marked";
  import MarkdownTokens from "./MarkdownTokens.svelte";
  import { lexBlocks, streamingBlockIndex } from "$lib/markdown/lex";
  import {
    markdownWorker,
    OFF_THREAD_LEX_MIN_CHARS,
  } from "$lib/markdown/offThread";
  import { tailOffsets } from "$lib/chat/reveal";

  interface Props {
    /** The body to render. */
    content: string;
    /** Whether this body is still being written. */
    streaming?: boolean;
    /**
     * How many characters at the end of `content` are still fading in.
     *
     * Measured in characters of the markdown *source*, not the rendered text,
     * because the reveal machinery counts what it has revealed. Blocks whose
     * trailing offset falls inside that window animate; everything before it is
     * already sharp and does not re-render.
     */
    trailChars?: number;
    class?: string;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let {
    content = "",
    streaming = false,
    trailChars = 0,
    class: customClass = "",
    onOpenArtifact,
  }: Props = $props();

  // Streaming bodies stay synchronous: every reveal tick produces a new
  // prefix, and a worker round-trip would lag behind the words. Settled bodies
  // over the threshold go to the worker; a body that is already in the worker
  // cache renders on the first frame. A cold body shows its raw source, escaped
  // by Svelte interpolation and marked busy, until the token tree arrives.
  const firstFrame = untrack(() => {
    if (streaming || content.length < OFF_THREAD_LEX_MIN_CHARS) {
      return {
        tokens: lexBlocks(content, streaming) as Token[],
        body: content,
        pending: false,
      };
    }
    const cached = markdownWorker.cachedLex(content);
    return {
      tokens: (cached ?? []) as Token[],
      body: cached ? content : "",
      pending: !cached,
    };
  });
  let tokens = $state<Token[]>(firstFrame.tokens);
  let tokensBody = firstFrame.body;
  let pending = $state(firstFrame.pending);

  $effect(() => {
    const body = content;
    const isStreaming = streaming;
    if (!isStreaming && body.length >= OFF_THREAD_LEX_MIN_CHARS) {
      const cached = markdownWorker.cachedLex(body);
      if (cached) {
        tokens = cached;
        tokensBody = body;
        pending = false;
        return;
      }
      if (!markdownWorker.available()) {
        tokens = lexBlocks(body, false);
        tokensBody = body;
        pending = false;
        return;
      }
      // If the body just stopped streaming, the tokens already on screen are
      // the same tree the settled lexer will produce. Keep them instead of
      // flashing the raw source while the worker answers.
      const stale = tokensBody !== body || tokens.length === 0;
      pending = stale;
      let active = true;
      void markdownWorker.lex(body).then((result) => {
        if (!active) return;
        pending = false;
        tokens = result ?? lexBlocks(body, false);
        tokensBody = body;
      });
      return () => {
        active = false;
      };
    }
    tokens = lexBlocks(body, isStreaming);
    tokensBody = body;
    pending = false;
  });
</script>

<div
  class="markdown-content {customClass}"
  data-streaming={streaming || undefined}
  data-pending={pending || undefined}
  aria-busy={pending || undefined}
>
  {#if pending}
    <div class="markdown-pending__body">{content}</div>
  {:else}
    <MarkdownTokens tokens={tokens} {streaming} {onOpenArtifact} />
  {/if}
</div>
