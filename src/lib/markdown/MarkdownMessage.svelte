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
  import MarkdownBlock from "./MarkdownBlock.svelte";
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
  /**
   * The characters of the body that come after each block.
   *
   * Derived from the token `raw` lengths rather than the rendered text, because
   * the two differ: markup is counted in the raw and absent from the output, so
   * a heading with three `#` in it would otherwise be three characters out.
   */
  const offsets = $derived.by(() => {
    const lengths = tokens.map((t) => t.raw.length);
    // `after` for block i is the sum of everything after it; the last block has
    // nothing after it, so its offset is 0.
    const after: (number | undefined)[] = new Array(tokens.length);
    let rest = 0;
    for (let i = tokens.length - 1; i >= 0; i -= 1) {
      after[i] = rest;
      rest += lengths[i];
    }
    return tailOffsets(lengths, tokens.length ? after[tokens.length - 1] : undefined, trailChars);
  });

  /**
   * Stable identity for settled blocks.
   *
   * A streaming answer re-lexes the whole body on every revealed word. Without
   * a keyed identity the whole subtree would be re-created each time, which is
   * the O(n²) behaviour the old post-render DOM walk had. Keying on type and
   * length means only the block that actually changed is replaced.
   */
  const rendered = $derived.by((): { key: string; token: Token; streaming: boolean }[] => {
    const list: Token[] = tokens;
    const activeIndex = streaming ? streamingBlockIndex(list) : -1;
    return list.map((token: Token, i: number) => ({
      key: `${i}:${token.type}:${token.raw.length}`,
      token,
      streaming: i === activeIndex,
    }));
  });
</script>

<div class="markdown-content {customClass}" data-streaming={streaming || undefined}>
  {#each rendered as item, i (item.key)}
    <MarkdownBlock
      token={item.token}
      streaming={item.streaming}
      after={offsets[i]}
      {onOpenArtifact}
    />
  {/each}
</div>
