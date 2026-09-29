<script lang="ts">
  /**
   * One markdown token, rendered as real elements.
   *
   * A switch on token type rather than a string of HTML: each case owns its own
   * element, so a `code` block gets a component with copy and artifact buttons
   * instead of a DOM walk that injects them afterwards, and a `table` gets a
   * scroll container because the element is written here rather than
   * retrofitted.
   */
  import type { Token, Tokens } from "marked";
  import MarkdownTokens from "./MarkdownTokens.svelte";
  import MarkdownInline from "./MarkdownInline.svelte";
  import MarkdownCode from "./MarkdownCode.svelte";
  import MarkdownTable from "./MarkdownTable.svelte";
  import { lexInline, closesFinalTable } from "$lib/markdown/lex";
  import { tailInside } from "$lib/chat/reveal";
  import { safeHref } from "$lib/markdown/safeUrl";

  interface Props {
    token: Token;
    /** This block is the one currently being written. */
    streaming?: boolean;
    /** Body characters that come after this block. */
    after?: number;
    onOpenArtifact?: (code: string, lang: string) => void;
  }

  let { token, streaming = false, after = 0, onOpenArtifact }: Props = $props();

  /** Clamp a heading depth to the six real levels. */
  function headingLevel(depth: number): 1 | 2 | 3 | 4 | 5 | 6 {
    if (depth <= 1) return 1;
    if (depth > 6) return 6;
    return depth as 2 | 3 | 4 | 5 | 6;
  }

  /** Child tokens of a container, for nested rendering. */
  function inner(t: Token): Token[] {
    const nested = (t as { tokens?: Token[] }).tokens;
    return Array.isArray(nested) ? nested : [];
  }

  /** Where a container's children start, so they can measure their own offsets. */
  function childAfter(t: Token): number {
    return tailInside(t.raw, inner(t), after) ?? after;
  }

  //
  // A note on comments in this file, since the fix is not obvious. An HTML
  // comment in a Svelte template is a *node*. Inside a rendered branch that
  // makes the whitespace around it significant, so Svelte emits it as a real
  // text node and the message gains a space between every inline element —
  // `**bold**` rendering as "bold ", and copied text carrying the extra space.
  // Svelte has no comment form that leaves nothing behind (`{/* */}` is a parse
  // error), so the explanations for the branches live here, in the script,
  // rather than in the markup.

  // The `space` and `def` branch below is empty on purpose. Those are the blank
  // lines between blocks and the link reference definitions: structure the lexer
  // reports and the reader must not see, so nothing is emitted for them.
  //
  // Which leaves a renderer-shaped question for each remaining branch, and the
  // answers are here rather than in the markup for the reason in the header.
  //
  // `html` is shown, not executed. A model that emits a tag gets the tag as text
  // the reader can see, which is also the only honest way to show a snippet the
  // model meant to describe rather than render.
  //
  // Anything unrecognised falls through to its own source, rendered as inline
  // text — so an unknown token is escaped by being text, not by being dropped.

  /** Whether a container's last child is a table closing the whole block. */
  const childStreaming = $derived(streaming && !closesFinalTable(token));
</script>

<!-- svelte-ignore block_empty -->
{#if token.type === "space" || token.type === "def"}
{:else if token.type === "heading"}
  {@const level = headingLevel((token as Tokens.Heading).depth)}
  {#if level === 1}
    <h1 data-level="1"><MarkdownInline tokens={(token as Tokens.Heading).tokens} after={childAfter(token)} streaming={streaming} {onOpenArtifact} /></h1>
  {:else if level === 2}
    <h2 data-level="2"><MarkdownInline tokens={(token as Tokens.Heading).tokens} after={childAfter(token)} streaming={streaming} {onOpenArtifact} /></h2>
  {:else if level === 3}
    <h3 data-level="3"><MarkdownInline tokens={(token as Tokens.Heading).tokens} after={childAfter(token)} streaming={streaming} {onOpenArtifact} /></h3>
  {:else if level === 4}
    <h4 data-level="4"><MarkdownInline tokens={(token as Tokens.Heading).tokens} after={childAfter(token)} streaming={streaming} {onOpenArtifact} /></h4>
  {:else if level === 5}
    <h5 data-level="5"><MarkdownInline tokens={(token as Tokens.Heading).tokens} after={childAfter(token)} streaming={streaming} {onOpenArtifact} /></h5>
  {:else}
    <h6 data-level="6"><MarkdownInline tokens={(token as Tokens.Heading).tokens} after={childAfter(token)} streaming={streaming} {onOpenArtifact} /></h6>
  {/if}
{:else if token.type === "paragraph"}
  <p>
    <MarkdownInline
      tokens={(token as Tokens.Paragraph).tokens}
      after={childAfter(token)}
      {streaming}
      {onOpenArtifact}
    />
  </p>
{:else if token.type === "text"}
  {#if (token as Tokens.Text).tokens}
    <MarkdownInline
      tokens={(token as Tokens.Text).tokens!}
      after={childAfter(token)}
      {streaming}
      {onOpenArtifact}
    />
  {:else}
    <MarkdownInline inlineSource={(token as Tokens.Text).text} after={after} {streaming} {onOpenArtifact} />
  {/if}
{:else if token.type === "blockquote"}
  <blockquote>
    <MarkdownTokens tokens={inner(token)} streaming={childStreaming} {onOpenArtifact} />
  </blockquote>
{:else if token.type === "list"}
  {@const list = token as Tokens.List}
  {#if list.ordered}
    <ol start={list.start === "" ? undefined : list.start}>
      {#each list.items as item, i (i)}
        <li class={item.task ? "md-task" : undefined}>
          {#if item.task}
            <input type="checkbox" checked={item.checked === true} disabled aria-label={item.text} />
          {/if}
          {#if item.tokens.some((c) => c.type !== "checkbox")}
            <MarkdownTokens
              tokens={item.tokens ?? []}
              streaming={childStreaming && i === list.items.length - 1}
              {onOpenArtifact}
            />
          {:else}
            <span>{item.text}</span>
          {/if}
        </li>
      {/each}
    </ol>
  {:else}
    <ul>
      {#each list.items as item, i (i)}
        <li class={item.task ? "md-task" : undefined}>
          {#if item.task}
            <input type="checkbox" checked={item.checked === true} disabled aria-label={item.text} />
          {/if}
          {#if item.tokens.some((c) => c.type !== "checkbox")}
            <MarkdownTokens
              tokens={item.tokens ?? []}
              streaming={childStreaming && i === list.items.length - 1}
              {onOpenArtifact}
            />
          {:else}
            <span>{item.text}</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
{:else if token.type === "code"}
  {@const code = token as Tokens.Code}
  <MarkdownCode
    code={code.text}
    language={(code.lang ?? "").trim().split(/\s+/)[0] ?? ""}
    {streaming}
    {onOpenArtifact}
  />
{:else if token.type === "table"}
  <MarkdownTable table={token as Tokens.Table} {streaming} {onOpenArtifact} />
{:else if token.type === "hr"}
  <hr />
{:else if token.type === "html"}
  <span class="md-raw-html">{(token as Tokens.HTML).raw}</span>
{:else if token.type === "br"}
  <br />
{:else}
  <MarkdownInline inlineSource={token.raw} {after} {streaming} {onOpenArtifact} />
{/if}
