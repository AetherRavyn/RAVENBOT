/**
 * Live reasoning, per thread.
 *
 * This exists because reasoning was invisible for five independent reasons and
 * no amount of care in the renderer could fix it:
 *
 *  1. Only Anthropic extracted it. Every OpenAI-shaped provider took
 *     `_enable_reasoning` and returned a hardcoded `None`.
 *  2. Anthropic pushed it through the *text* channel wrapped in literal
 *     `<think>` markers, so it was indistinguishable from the answer.
 *  3. The text buffer is cleared at every tool round, so the trace vanished the
 *     moment an agent used a tool — exactly when it is most worth reading.
 *  4. Only the final round's reasoning was persisted.
 *  5. The renderer extracted only the first `<think>` block and dumped the rest
 *     into the answer as raw markup.
 *
 * The backend now sends reasoning as its own event and persists it in its own
 * field. This is the live half: an accumulator per thread that `clear` does not
 * touch, which is the whole point — the answer's buffer is legitimately reset
 * between model rounds, and reasoning must not be reset with it.
 *
 * Rounds are separated when a `clear` arrives rather than run together. A run is
 * many model rounds — think, act, think again — and one undifferentiated wall of
 * text is not readable by anyone. The separator says why the trace continues,
 * because "after tool results" is the part worth reading when an answer looks
 * wrong.
 */

/** One contiguous stretch of thinking, with what produced it. */
export interface ReasoningRound {
  /** The thinking itself. */
  text: string;
  /** `false` for the first round of a run, `true` once tools have reported. */
  afterTools: boolean;
  /** True while tokens are still arriving into this round. */
  live: boolean;
}

export interface ThreadReasoning {
  rounds: ReasoningRound[];
  /** The agent doing the thinking, for the label beside it. */
  botId: string;
  startedAt: number;
}

/** The rule between rounds. Says *why* the trace continues. */
export const REASONING_ROUND_SEPARATOR = "after tool results";

/** Cap per round, so a runaway trace cannot grow without bound. */
const MAX_PER_ROUND = 40_000;
/** Cap on rounds kept live, matching how many the backend folds into a message. */
const MAX_ROUNDS = 24;

export class ReasoningStore {
  /** Keyed by thread id. */
  byThread = $state<Record<string, ThreadReasoning>>({});

  /**
   * Fold one reasoning chunk in.
   *
   * Returns true when something changed, so a caller can decide whether to
   * scroll — appending to a trace the reader cannot see is worse than not
   * scrolling, but a scroll on an unchanged trace is just a jump.
   */
  ingest(payload: any): boolean {
    const threadId = String(payload?.thread_id || "");
    const content = payload?.content;
    if (!threadId || typeof content !== "string" || !content) return false;

    const existing = this.byThread[threadId];
    if (!existing) {
      this.byThread = {
        ...this.byThread,
        [threadId]: {
          botId: String(payload?.bot_id || ""),
          startedAt: Date.now(),
          rounds: [{ text: content, afterTools: false, live: true }],
        },
      };
      return true;
    }

    const rounds = [...existing.rounds];
    const last = rounds[rounds.length - 1];
    if (!last || !last.live) {
      // A new round with no `clear` in between — some providers start reasoning
      // again after a tool result without an explicit round boundary. Treated as
      // a continuation rather than dropped.
      rounds.push({ text: content, afterTools: true, live: true });
    } else {
      rounds[rounds.length - 1] = {
        ...last,
        text: (last.text + content).slice(-MAX_PER_ROUND),
      };
    }
    this.byThread = {
      ...this.byThread,
      [threadId]: { ...existing, rounds: rounds.slice(-MAX_ROUNDS) },
    };
    return true;
  }

  /**
   * A new model round began.
   *
   * The answer's buffer is reset — tool-round fragments must not mix with the
   * final response — and the current reasoning stretch is closed, so the next one
   * is visibly a new stretch rather than more of the same wall.
   *
   * Closes rather than clears, and that difference is the bug this store was
   * written to prevent.
   */
  endRound(threadId: string): boolean {
    const existing = this.byThread[threadId];
    if (!existing) return false;
    const rounds = existing.rounds;
    const last = rounds[rounds.length - 1];
    if (!last || !last.live) return false;

    const next = [...rounds];
    next[next.length - 1] = { ...last, live: false };
    // An empty opening round would render as a stray separator.
    const cleaned = next.length > 1 && !next[0].text.trim() ? next.slice(1) : next;
    this.byThread = { ...this.byThread, [threadId]: { ...existing, rounds: cleaned } };
    return true;
  }

  /** The run finished, or the thread was cleared: the trace is complete. */
  finish(threadId: string): boolean {
    const existing = this.byThread[threadId];
    if (!existing) return false;
    const rounds = existing.rounds.map((r) => (r.live ? { ...r, live: false } : r));
    this.byThread = { ...this.byThread, [threadId]: { ...existing, rounds } };
    return true;
  }

  /**
   * Drop a thread's trace.
   *
   * Only for a thread that is genuinely gone — a new conversation, a run
   * abandoned. Not on `clear`, which is a round boundary.
   */
  drop(threadId: string): void {
    if (!this.byThread[threadId]) return;
    const next = { ...this.byThread };
    delete next[threadId];
    this.byThread = next;
  }

  reset(): void {
    this.byThread = {};
  }

  /** The whole trace as one string, for copying or for a plain-text fallback. */
  text(threadId: string): string {
    const existing = this.byThread[threadId];
    if (!existing) return "";
    return existing.rounds
      .filter((r) => r.text.trim())
      .map((r) => r.text.trim())
      .join(`\n\n— ${REASONING_ROUND_SEPARATOR} —\n\n`);
  }

  /** Is anything being thought right now? */
  thinking(threadId: string): boolean {
    return this.byThread[threadId]?.rounds.some((r) => r.live) ?? false;
  }
}

/**
 * One store for the app.
 *
 * A module singleton rather than a per-component instance because the same trace
 * has to be readable from the 1:1 thread, the office channel, and the fleet list
 * at the same time — three views of one run, not three runs. Per-component state
 * is how those drift apart.
 */
export const reasoning = new ReasoningStore();

/**
 * Split a stored reasoning trace into the rounds to render.
 *
 * The persisted form is one string with `---` separators — plain text, because it
 * lives in a JSON column and has to survive being read by anything. Rendering it
 * needs the boundaries back, so they are recovered here rather than at write time,
 * which keeps the stored value readable in a database browser.
 */
export function parseStoredReasoning(raw: string | null | undefined): ReasoningRound[] {
  if (!raw || !raw.trim()) return [];
  const parts = raw
    .split(/\n\s*---\s*\n/)
    .map((p) => p.replace(/^\s*\*\(after tool results\)\*\s*/i, "").trim())
    .filter(Boolean);
  if (!parts.length) return [];
  return parts.map((text, i) => ({ text, afterTools: i > 0, live: false }));
}
