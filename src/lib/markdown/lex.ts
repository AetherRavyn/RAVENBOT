/**
 * Markdown → tokens, never tokens → HTML.
 *
 * The renderer used to call `marked.parse()` and inject the string with
 * `{@html}`, then walk the resulting DOM to add copy buttons to every code
 * block and append a tail span to the last text node. Three problems with that:
 *
 *  - It is an injection surface. `marked.parse` passes raw HTML through, and
 *    the call site is a `{@html}` sink. A model that emits `<img onerror=…>` or
 *    a `<script>` is executed in the app window, which has the Tauri IPC
 *    bridge on it.
 *  - Every streamed token re-parsed the whole body and re-walked the whole
 *    subtree, so a long answer did O(n²) DOM work and visibly janked.
 *  - Injecting the stream tail meant reaching into a rendered tree, which is
 *    why the target had to be found by walking to "the last P, LI or H1–H6" —
 *    a heuristic that breaks the moment marked nests differently.
 *
 * Lexing instead gives a token tree. Tokens are data, so the component tree is
 * built from them, raw HTML can be shown as escaped text, and the same input
 * always produces the same output.
 */
import { Lexer, type Token, type TokensList } from "marked";

/** A block token with the `raw` source we need for tail arithmetic. */
export interface Block {
  token: Token;
  /** The exact source text of this block, from the last block's end. */
  raw: string;
}

/**
 * How many lexed bodies to keep.
 *
 * Scroll virtualisation re-renders settled messages constantly, and lexing is
 * the expensive half of rendering, so identical bodies are worth reusing. The
 * bound matters: a long session has thousands of messages and an unbounded map
 * holds every token tree in memory for the life of the window.
 */
const CACHE_LIMIT = 200;

const blockCache = new Map<string, TokensList>();
const inlineCache = new Map<string, Token[]>();

/**
 * Lex a body into block tokens.
 *
 * A streaming body is never cached. Every revealed word is a new prefix, so
 * caching them would hold up to `CACHE_LIMIT` obsolete token trees and evict
 * the settled messages that were actually worth keeping.
 */
export function lexBlocks(body: string, streaming: boolean): TokensList {
  if (!streaming) {
    const hit = blockCache.get(body);
    if (hit) return hit;
  }
  const tokens = Lexer.lex(body, { breaks: true, gfm: true });
  if (!streaming) remember(blockCache, body, tokens);
  return tokens;
}

/** Lex an inline fragment — a table cell, a heading's contents. */
export function lexInline(body: string): Token[] {
  const hit = inlineCache.get(body);
  if (hit) return hit;
  const tokens = Lexer.lexInline(body, { breaks: true, gfm: true });
  remember(inlineCache, body, tokens);
  return tokens;
}

function remember<T>(cache: Map<string, T>, key: string, value: T): void {
  if (cache.size >= CACHE_LIMIT) {
    // Map preserves insertion order, so the first key is the oldest.
    const oldest = cache.keys().next();
    if (!oldest.done) cache.delete(oldest.value);
  }
  cache.set(key, value);
}

/** Clear both caches. Used by tests, and on a locale/theme reset. */
export function clearLexCaches(): void {
  blockCache.clear();
  inlineCache.clear();
}

/**
 * How many lexed bodies are currently retained.
 *
 * Exposed so a test can assert the bound is real. Without it, "is the cache
 * actually bounded" can only be checked by timing, and a test that re-lexes a
 * body twice always hits — whether it was evicted a moment ago or never
 * stored.
 */
export function lexCacheSize(): number {
  return blockCache.size;
}

/**
 * The source length of each block, and the offset each one ends at.
 *
 * The streaming tail is measured in characters of the *body*, so a block needs
 * to know how much body text comes after it. `marked` gives each token its own
 * `raw`, so the offsets accumulate.
 */
export function blockLengths(tokens: readonly Token[]): number[] {
  return tokens.map((token) => token.raw.length);
}

/**
 * The last block that is still growing, or -1.
 *
 * Only one block animates at a time. During a stream the final block is the one
 * being written into, so it is the only one whose contents can change; every
 * earlier block is settled and must not re-render on each word.
 */
export function streamingBlockIndex(tokens: readonly Token[]): number {
  for (let i = tokens.length - 1; i >= 0; i -= 1) {
    const t = tokens[i];
    if (t.type === "space" || t.type === "def") continue;
    return i;
  }
  return -1;
}

/** Whether a token closes a table that is the last thing in a container. */
export function closesFinalTable(token: Token): boolean {
  if (token.type !== "blockquote" && token.type !== "list") return false;
  const inner = (token as { tokens?: Token[] }).tokens ?? [];
  for (let i = inner.length - 1; i >= 0; i -= 1) {
    const t = inner[i];
    if (t.type === "space") continue;
    return t.type === "table";
  }
  return false;
}
