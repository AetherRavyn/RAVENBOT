<script lang="ts">
  /**
   * Render a list of block tokens.
   *
   * This exists because a container's children are already lexed. `marked`
   * hands back a blockquote's inner blocks and a list item's inner blocks as
   * token arrays, and the obvious way to render those is to pass the
   * container's `raw` source to `MarkdownMessage` and let it lex again — which
   * is what this used to do, and it recursed until the stack ran out.
   *
   * `raw` is the block's own source *including its markers*: `"> quote"` for a
   * blockquote, `"- one\n"` for a list item. Lexing either produces the same
   * block again, so `MarkdownMessage` re-entered `MarkdownBlock`, which handed
   * back the same `raw`, forever. A blockquote or a list in a model reply was
   * enough to take the whole message view down with a `RangeError`, and both are
   * among the most common things a model writes.
   *
   * Rendering the tokens that are already there removes the re-lex entirely, and
   * with it the recursion.
   */
  import type { Token } from "marked";
  import MarkdownBlock from "./MarkdownBlock.svelte";
  import { streamingBlockIndex } from "$lib/markdown/lex";
  import { tailOffsets } from "$lib/chat/reveal";

  interface Props {
    tokens: Token[];
    /**
     * The *body* is still being written.
     *
     * Propagated so a container's last child can animate, but the child is only
     * treated as growing when this is true — a settled blockquote must not
     * re-render on every streamed word elsewhere in the message.
     */
    streaming?: boolean;
    /** Body characters that come after this whole group. */
    trailChars?: number;
    /** Extra class for the wrapper, for a nested block to narrow its own. */
    class?: string;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let {
    tokens,
    streaming = false,
    trailChars = 0,
    class: customClass = "",
    onOpenArtifact,
  }: Props = $props();

  /**
   * Body characters that come after each block.
   *
   * Derived from the token `raw` lengths rather than the rendered text, because
   * the two differ: markup is counted in the raw and absent from the output, so
   * a heading with three `#` in it would otherwise be three characters out.
   */
  const offsets = $derived.by(() => {
    const lengths = tokens.map((t) => t.raw?.length ?? 0);
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
    const activeIndex = streaming ? streamingBlockIndex(tokens) : -1;
    return tokens.map((token: Token, i: number) => ({
      key: `${i}:${token.type}:${token.raw?.length ?? 0}`,
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
