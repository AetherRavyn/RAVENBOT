/**
 * Markdown worker entry.
 *
 * It runs the same pure lexer and highlighter the synchronous fallback uses,
 * so a token tree produced here is indistinguishable from one produced on the
 * UI thread. The only job of this file is to receive a request, run the pure
 * function, bound a small cache, and post the result back.
 */
import type { Token as MarkedToken } from "marked";
import type { Token as HighlightToken } from "$lib/highlight";
import { lexBlocks } from "./lex";
import { hl, hlLanguage } from "$lib/highlight";
import type {
  MarkdownWorkerRequest,
  MarkdownWorkerResponse,
} from "./workerProtocol";

const LEX_CACHE_LIMIT = 120;
const HIGHLIGHT_CACHE_LIMIT = 120;
const MAX_CACHED_BODY_CHARS = 200_000;
const MAX_CACHED_CODE_CHARS = 120_000;

const lexCache = new Map<string, MarkedToken[]>();
const highlightCache = new Map<string, HighlightToken[]>();

function remember<T>(cache: Map<string, T>, key: string, value: T, limit: number): T {
  if (cache.has(key)) cache.delete(key);
  cache.set(key, value);
  while (cache.size > limit) {
    const oldest = cache.keys().next();
    if (oldest.done) break;
    cache.delete(oldest.value);
  }
  return value;
}

function lexBody(body: string): MarkedToken[] {
  const hit = lexCache.get(body);
  if (hit) return remember(lexCache, body, hit, LEX_CACHE_LIMIT);
  // `marked` returns an array with a `links` side-table. The renderer consumes
  // the array only, and a plain array is what structured clone guarantees.
  const tokens = Array.from(lexBlocks(body, false)) as MarkedToken[];
  if (body.length > MAX_CACHED_BODY_CHARS) return tokens;
  return remember(lexCache, body, tokens, LEX_CACHE_LIMIT);
}

function highlightCode(code: string, language: string): HighlightToken[] {
  const key = `${language}\u0000${code}`;
  const hit = highlightCache.get(key);
  if (hit) return remember(highlightCache, key, hit, HIGHLIGHT_CACHE_LIMIT);
  const tokens = hl(code, hlLanguage(language));
  if (code.length > MAX_CACHED_CODE_CHARS) return tokens;
  return remember(highlightCache, key, tokens, HIGHLIGHT_CACHE_LIMIT);
}

const scope = self as unknown as {
  onmessage: ((event: MessageEvent<MarkdownWorkerRequest>) => void) | null;
  postMessage: (message: MarkdownWorkerResponse) => void;
};

function reply(message: MarkdownWorkerResponse): void {
  try {
    scope.postMessage(message);
  } catch {
    // A response that cannot be cloned is a bug, not a reason to take the
    // worker down: the caller's timeout handles the lost request and falls
    // back to the UI thread.
  }
}

scope.onmessage = (event) => {
  const request = event.data;
  if (!request || typeof request !== "object" || typeof request.id !== "number") {
    return;
  }
  const id = request.id;
  try {
    if (request.kind === "lex") {
      if (typeof request.body !== "string") {
        reply({ id, ok: false, error: "The lex request had no body." });
        return;
      }
      reply({ id, ok: true, tokens: lexBody(request.body) });
      return;
    }
    if (request.kind === "highlight") {
      if (typeof request.code !== "string") {
        reply({ id, ok: false, error: "The highlight request had no code." });
        return;
      }
      reply({
        id,
        ok: true,
        tokens: highlightCode(request.code, request.language || ""),
      });
      return;
    }
    reply({ id, ok: false, error: "Unknown markdown worker request." });
  } catch (error) {
    reply({
      id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
};
