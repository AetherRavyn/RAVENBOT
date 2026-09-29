<script lang="ts">
  /**
   * Inline markdown: strong, emphasis, strikethrough, code spans, links,
   * images, and the text between them.
   *
   * The old renderer emitted these through `marked`'s HTML output. Here each
   * one is an element built from a token, which is what makes a `javascript:`
   * link refusable and lets text inside the streaming trail be wrapped
   * individually.
   */
  import type { Token, Tokens } from "marked";
  // Svelte 5 resolves a component by filename, so this is the recursive
  // reference rather than an import cycle.
  import MarkdownInline from "./MarkdownInline.svelte";
  import { lexInline } from "$lib/markdown/lex";
  import { tailInside, splitTrail, trailReach, type RevealChunk } from "$lib/chat/reveal";
  import { safeHref, safeImageSrc, isExternal } from "$lib/markdown/safeUrl";
  import { openExternal } from "$lib/external";

  interface Props {
    /** Inline tokens, or a raw string to lex. */
    tokens?: Token[];
    inlineSource?: string;
    /** Body characters that come after this run of text. */
    after?: number;
    /** Steps still fading, for the text-fade treatment. */
    trail?: RevealChunk[];
    streaming?: boolean;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let { tokens, inlineSource, after = 0, trail = [], streaming = false, onOpenArtifact }: Props = $props();

  const resolved = $derived(tokens ?? (inlineSource ? lexInline(inlineSource) : []));

  //
  // A note on comments in this file, since the fix is not obvious. An HTML
  // comment in a Svelte template is a *node*. Inside a rendered branch that
  // makes the whitespace around it significant, so Svelte emits it as a real
  // space text node and the message gains a space between every inline element —
  // `**bold**` rendering as "bold ", and copied text carrying the extra space.
  // Svelte has no comment form that leaves nothing behind (`{/* */}` is a parse
  // error), so the explanations for the branches live here, in the script,
  // rather than in the markup.
  //
  // A link whose URL `safeHref` refuses is shown as text rather than dropped,
  // so the reader can still see and copy what the model wrote.
  //
  // Each trail chunk is a fresh element, so the CSS fade-in runs once per chunk
  // with no timer and no reactive clock. A step still fading when the next one
  // appears is a different element with its own animation, so it never jumps to
  // full opacity.
  //
  /** Wrap the tail of a text run in per-step fading spans. */
  function withTrail(text: string, offset: number) {
    const { prefix, chunks } = splitTrail(text, trail, offset);
    return { prefix, chunks };
  }
</script>

{#each resolved as token, i (i)}
  {@const offset = tailInside(token.raw, [], after) ?? after}
  {#if token.type === "text"}
    {@const t = token as Tokens.Text}
    {#if t.tokens}
      <MarkdownInline tokens={t.tokens} after={offset} {trail} {streaming} {onOpenArtifact} />
    {:else}
      {@const parts = withTrail(t.text, offset)}
      {#if parts.prefix}<span>{parts.prefix}</span>{/if}{#each parts.chunks as chunk, ci (ci)}<span class="md-fade">{chunk.text}</span>{/each}
    {/if}
  {:else if token.type === "strong"}
    <strong><MarkdownInline tokens={(token as Tokens.Strong).tokens} after={offset} {trail} {streaming} {onOpenArtifact} /></strong>
  {:else if token.type === "em"}
    <em><MarkdownInline tokens={(token as Tokens.Em).tokens} after={offset} {trail} {streaming} {onOpenArtifact} /></em>
  {:else if token.type === "del"}
    <del><MarkdownInline tokens={(token as Tokens.Del).tokens} after={offset} {trail} {streaming} {onOpenArtifact} /></del>
  {:else if token.type === "codespan"}
    <code>{(token as Tokens.Codespan).text}</code>
  {:else if token.type === "link"}
    {@const link = token as Tokens.Link}
    {@const href = safeHref(link.href)}
    {#if href}
      <a
        {href}
        title={link.title || undefined}
        rel={isExternal(href) ? "noopener noreferrer nofollow" : undefined}
        target={isExternal(href) ? "_blank" : undefined}
        onclick={(e) => {
          if (!isExternal(href)) return;
          e.preventDefault();
          openExternal(href);
        }}>{link.text}</a>
    {:else}
      <span class="md-link-refused" title="This link was not made clickable">{link.text}</span>
    {/if}
  {:else if token.type === "image"}
    {@const img = token as Tokens.Image}
    {@const src = safeImageSrc(img.href)}
    {#if src}
      <img class="md-image" {src} alt={img.text || ""} title={img.title || undefined} loading="lazy" />
    {:else}
      <span class="md-link-refused">{img.text || img.href}</span>
    {/if}
  {:else if token.type === "br"}
    <br />
  {:else if token.type === "escape"}
    {(token as Tokens.Escape).text}
  {:else if token.type === "html"}
    <span class="md-raw-html">{(token as Tokens.HTML).raw}</span>
  {:else}
    {token.raw}
  {/if}
{/each}
