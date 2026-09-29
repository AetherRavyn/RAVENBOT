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
  import type { Token } from "marked";
  import MarkdownTokens from "./MarkdownTokens.svelte";
  import { lexBlocks, streamingBlockIndex } from "$lib/markdown/lex";
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

  const tokens = $derived(lexBlocks(content, streaming));
</script>

<div class="markdown-content {customClass}" data-streaming={streaming || undefined}>
  <MarkdownTokens tokens={tokens} {streaming} {onOpenArtifact} />
</div>
