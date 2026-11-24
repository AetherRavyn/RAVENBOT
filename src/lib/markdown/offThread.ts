/**
 * Optional off-thread lexing and highlighting.
 *
 * The renderer is deliberately synchronous when it has to be: a streaming
 * answer changes on every reveal tick, short bodies cost less to lex than a
 * postMessage round-trip, and a missing or crashed worker must never leave a
 * message blank. Everything larger and settled goes to the worker, while the
 * caller keeps its synchronous path as the fallback.
 *
 * The client owns the request ids, the timeout, the worker crash policy and a
 * small main-thread result cache. The worker owns its own bounded caches, so a
 * remount asks for a body the worker has already seen and gets it back without
 * re-lexing.
 */
import type { Token as MarkedToken } from "marked";
import type { Token as HighlightToken } from "$lib/highlight";
import {
  type MarkdownWorkerResponse,
  type WorkerToken,
} from "./workerProtocol";

/** Settled bodies shorter than this are cheaper to lex on the UI thread. */
export const OFF_THREAD_LEX_MIN_CHARS = 4_000;
/** Code shorter than this is highlighted synchronously while it is cheap. */
export const OFF_THREAD_HIGHLIGHT_MIN_CHARS = 1_200;

const DEFAULT_TIMEOUT_MS = 15_000;
const WORKER_COOLDOWN_MS = 30_000;
const LEX_CACHE_LIMIT = 120;
const HIGHLIGHT_CACHE_LIMIT = 120;
const PREWARMED_LIMIT = 240;
const PREWARM_BATCH = 24;
/** A huge body is still parsed, but its token tree is not retained twice. */
const MAX_CACHED_BODY_CHARS = 200_000;
const MAX_CACHED_CODE_CHARS = 120_000;

/**
 * The subset of Worker the client needs. Keeping it structural lets the unit
 * tests drive a fake without pretending to be a browser.
 */
export interface MarkdownWorkerLike {
  postMessage(message: unknown): void;
  terminate(): void;
  onmessage: ((event: MessageEvent) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
  onmessageerror: ((event: MessageEvent) => void) | null;
}

type RequestPayload =
  | { kind: "lex"; body: string }
  | { kind: "highlight"; code: string; language: string };

interface PendingRequest {
  resolve: (tokens: WorkerToken[]) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export interface MarkdownWorkerClient {
  /** True when a worker can be started right now. May start the worker. */
  available(): boolean;
  /** Lex a settled body off-thread, or null when the worker is unavailable. */
  lex(body: string): Promise<MarkedToken[] | null>;
  /** Highlight code off-thread, or null when the worker is unavailable. */
  highlight(code: string, language: string): Promise<HighlightToken[] | null>;
  cachedLex(body: string): MarkedToken[] | undefined;
  cachedHighlight(code: string, language: string): HighlightToken[] | undefined;
  /** Ask the worker to cache long settled bodies for future mounts. */
  prewarm(bodies: readonly string[]): void;
  /** Drop caches, terminate the worker and allow a fresh start. Tests use this. */
  reset(): void;
  /** Terminate the worker without clearing the result caches. */
  dispose(): void;
}

function highlightKey(code: string, language: string): string {
  return `${language}\u0000${code}`;
}

function remember<T>(
  cache: Map<string, T>,
  key: string,
  value: T,
  limit: number,
): void {
  if (cache.has(key)) cache.delete(key);
  cache.set(key, value);
  while (cache.size > limit) {
    const oldest = cache.keys().next();
    if (oldest.done) break;
    cache.delete(oldest.value);
  }
}

export function createMarkdownWorkerClient(
  createWorker: () => MarkdownWorkerLike | null,
  timeoutMs = DEFAULT_TIMEOUT_MS,
): MarkdownWorkerClient {
  const lexCache = new Map<string, MarkedToken[]>();
  const highlightCache = new Map<string, HighlightToken[]>();
  const lexInFlight = new Map<string, Promise<MarkedToken[] | null>>();
  const highlightInFlight = new Map<string, Promise<HighlightToken[] | null>>();
  const prewarmed = new Set<string>();
  const pending = new Map<number, PendingRequest>();

  let worker: MarkdownWorkerLike | null | undefined;
  let coolingUntil = 0;
  let nextId = 1;

  function failAll(reason: Error): void {
    for (const [id, entry] of pending) {
      clearTimeout(entry.timer);
      entry.reject(reason);
      pending.delete(id);
    }
  }

  function discard(reason: Error): void {
    failAll(reason);
    try {
      worker?.terminate();
    } catch {
      /* already gone */
    }
    // `undefined`, not `null`: after the cooldown `ensureWorker` must be able
    // to create a fresh worker. A permanent null would turn one crash into a
    // permanently synchronous renderer.
    worker = undefined;
    coolingUntil = Date.now() + WORKER_COOLDOWN_MS;
    prewarmed.clear();
  }

  function handleMessage(event: MessageEvent): void {
    const message = event.data as MarkdownWorkerResponse | null;
    if (!message || typeof message.id !== "number") return;
    const entry = pending.get(message.id);
    if (!entry) return;
    pending.delete(message.id);
    clearTimeout(entry.timer);
    if (message.ok && Array.isArray(message.tokens)) {
      entry.resolve(message.tokens);
    } else if (message.ok) {
      entry.reject(new Error("The markdown worker returned an invalid token list."));
    } else {
      entry.reject(new Error(message.error || "The markdown worker failed."));
    }
  }

  function ensureWorker(): MarkdownWorkerLike | null {
    if (worker !== undefined) return worker;
    if (Date.now() < coolingUntil) return null;
    let created: MarkdownWorkerLike | null = null;
    try {
      created = createWorker();
    } catch (error) {
      console.warn("Markdown worker could not start; using the UI thread.", error);
    }
    if (!created) {
      // Do not pretend the feature is permanently unavailable: a chunk load
      // can fail transiently. Retry once the cooldown passes.
      worker = undefined;
      coolingUntil = Date.now() + WORKER_COOLDOWN_MS;
      return null;
    }
    worker = created;
    worker.onmessage = handleMessage;
    worker.onerror = (event) => {
      discard(new Error(event?.message || "The markdown worker crashed."));
    };
    worker.onmessageerror = () => {
      discard(new Error("The markdown worker returned an uncloneable message."));
    };
    return worker;
  }

  function request(message: RequestPayload): Promise<WorkerToken[]> {
    const active = ensureWorker();
    if (!active) {
      return Promise.reject(new Error("The markdown worker is unavailable."));
    }
    const id = nextId++;
    return new Promise<WorkerToken[]>((resolve, reject) => {
      const timer = setTimeout(() => {
        // A worker that cannot answer in the budget is not safe to keep:
        // terminate it and let the caller fall back to the synchronous path.
        discard(new Error(`The markdown worker timed out after ${timeoutMs}ms.`));
      }, timeoutMs);
      pending.set(id, { resolve, reject, timer });
      try {
        active.postMessage({ ...message, id });
      } catch (error) {
        pending.delete(id);
        clearTimeout(timer);
        const failure = error instanceof Error ? error : new Error(String(error));
        discard(failure);
        reject(failure);
      }
    });
  }

  function lex(body: string): Promise<MarkedToken[] | null> {
    const cached = lexCache.get(body);
    if (cached) {
      remember(lexCache, body, cached, LEX_CACHE_LIMIT);
      return Promise.resolve(cached);
    }
    const existing = lexInFlight.get(body);
    if (existing) return existing;
    const promise = (async () => {
      try {
        const tokens = (await request({ kind: "lex", body })) as MarkedToken[];
        if (body.length <= MAX_CACHED_BODY_CHARS) {
          remember(lexCache, body, tokens, LEX_CACHE_LIMIT);
        }
        return tokens;
      } catch (error) {
        console.warn("Markdown lexing fell back to the UI thread.", error);
        return null;
      }
    })();
    lexInFlight.set(body, promise);
    void promise.finally(() => {
      if (lexInFlight.get(body) === promise) lexInFlight.delete(body);
    });
    return promise;
  }

  function highlight(
    code: string,
    language: string,
  ): Promise<HighlightToken[] | null> {
    const key = highlightKey(code, language);
    const cached = highlightCache.get(key);
    if (cached) {
      remember(highlightCache, key, cached, HIGHLIGHT_CACHE_LIMIT);
      return Promise.resolve(cached);
    }
    const existing = highlightInFlight.get(key);
    if (existing) return existing;
    const promise = (async () => {
      try {
        const tokens = (await request({
          kind: "highlight",
          code,
          language,
        })) as HighlightToken[];
        if (code.length <= MAX_CACHED_CODE_CHARS) {
          remember(highlightCache, key, tokens, HIGHLIGHT_CACHE_LIMIT);
        }
        return tokens;
      } catch (error) {
        console.warn("Code highlighting fell back to the UI thread.", error);
        return null;
      }
    })();
    highlightInFlight.set(key, promise);
    void promise.finally(() => {
      if (highlightInFlight.get(key) === promise) highlightInFlight.delete(key);
    });
    return promise;
  }

  function prewarm(bodies: readonly string[]): void {
    const candidates = bodies.filter(
      (body) =>
        typeof body === "string" &&
        body.length >= OFF_THREAD_LEX_MIN_CHARS &&
        body.length <= MAX_CACHED_BODY_CHARS &&
        !lexCache.has(body) &&
        !prewarmed.has(body),
    );
    if (candidates.length === 0) return;
    if (!ensureWorker()) return;
    let sent = 0;
    for (const body of candidates) {
      prewarmed.add(body);
      if (prewarmed.size > PREWARMED_LIMIT) {
        const oldest = prewarmed.values().next();
        if (!oldest.done) prewarmed.delete(oldest.value);
      }
      // The lex result is cached on the main thread as well, so the row that
      // eventually mounts can render synchronously instead of showing the
      // raw-source placeholder for a frame.
      void lex(body);
      sent += 1;
      if (sent >= PREWARM_BATCH) return;
    }
  }

  return {
    available: () => ensureWorker() !== null,
    lex,
    highlight,
    cachedLex: (body) => lexCache.get(body),
    cachedHighlight: (code, language) =>
      highlightCache.get(highlightKey(code, language)),
    prewarm,
    reset() {
      failAll(new Error("The markdown worker was reset."));
      try {
        worker?.terminate();
      } catch {
        /* already gone */
      }
      worker = undefined;
      coolingUntil = 0;
      lexCache.clear();
      highlightCache.clear();
      lexInFlight.clear();
      highlightInFlight.clear();
      prewarmed.clear();
    },
    dispose() {
      failAll(new Error("The markdown worker was disposed."));
      try {
        worker?.terminate();
      } catch {
        /* already gone */
      }
      worker = undefined;
      coolingUntil = 0;
    },
  };
}

function browserWorker(): MarkdownWorkerLike | null {
  if (typeof Worker === "undefined") return null;
  return new Worker(new URL("./worker.ts", import.meta.url), {
    type: "module",
    name: "ravenbot-markdown",
  });
}

export const markdownWorker = createMarkdownWorkerClient(browserWorker);

/**
 * Prepare the long bodies in a transcript before their rows mount.
 *
 * Virtualization means only a few rows exist at a time. Warming the long ones
 * while the short ones paint turns the first scroll into a cache hit instead
 * of a parse. The worker bounds both the batch and its own cache.
 */
export function prewarmMarkdownMessages(messages: readonly any[]): void {
  if (messages.length === 0) return;
  const bodies: string[] = [];
  for (const message of messages) {
    const content = message?.content;
    const text =
      typeof content === "string"
        ? content
        : typeof content?.text === "string"
          ? content.text
          : "";
    if (text.length >= OFF_THREAD_LEX_MIN_CHARS) bodies.push(text);
  }
  if (bodies.length === 0) return;
  markdownWorker.prewarm(bodies);
}
