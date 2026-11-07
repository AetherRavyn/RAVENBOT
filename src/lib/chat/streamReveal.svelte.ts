// Client-side word-by-word reveal decoupled from token arrival (OpenBot
// streaming pattern): SSE deltas append to the RAW buffer; the visible string
// advances in whole words. The newest word keeps its blur-in tail until the
// stream FINISHES (not merely until the pump catches up), so the tail never
// snaps sharp mid-run. Reduced motion shows everything instantly.
//
// The reveal is **backlog-aware**. A fixed one-word-per-tick pump looks lovely
// when tokens trickle and is unusable when they burst: a fast model emits a
// 1,000-word answer in twenty seconds, the pump needs a minute, and the run
// reports "done" while the answer is still typing itself out. Each tick now
// closes a fraction of the gap (in whole words), so the visible text stays
// roughly half a second behind the model regardless of burst size.
import { prefersReducedMotion } from "$lib/a11y";

const WORD_START = /\S/;

function gapMs(): number {
  if (typeof document === "undefined") return 60;
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--stream-gap");
  const n = parseFloat(raw);
  return Number.isFinite(n) && n > 0 ? n : 60;
}

export class StreamReveal {
  raw = $state("");
  shown = $state("");
  /** True while a reveal session is live: the newest word carries the tail. */
  active = $state(false);
  private timer: ReturnType<typeof setInterval> | null = null;

  push(chunk: string) {
    this.raw += chunk;
    this.ensurePump();
  }

  /** Follow an externally-owned raw buffer (e.g. `streamingText += token`). */
  track(target: string) {
    if (target === this.raw) return;
    const appended = target.startsWith(this.raw) || this.raw === "";
    this.raw = target;
    if (!appended || target === "") {
      // Buffer cleared (`clear`/thread switch) or replaced (retry) — restart
      // cleanly and kill the pump NOW so no stale tick survives.
      this.shown = "";
      this.active = false;
      this.stop();
      if (target === "") return;
    }
    if (prefersReducedMotion()) {
      this.shown = target;
      this.active = false;
      this.stop();
      return;
    }
    this.ensurePump();
  }

  /** How many characters the reader has not seen yet. */
  get backlog(): number {
    return Math.max(0, this.raw.length - this.shown.length);
  }

  private ensurePump() {
    if (prefersReducedMotion()) {
      this.stop();
      this.shown = this.raw;
      return;
    }
    if (this.timer) return;
    this.active = true;
    this.timer = setInterval(() => this.tick(), gapMs());
  }

  private tick() {
    const backlog = this.backlog;
    if (backlog <= 0) {
      // Caught up, but the run is still live: stop ticking while KEEPING
      // `active` so the tail stays on the newest word until finish().
      this.stop();
      return;
    }

    // Close ~1/10th of the gap per tick, at least one character. Because the
    // step is in characters but the cut is moved to a word boundary, the
    // reader never sees half a word.
    const start = this.shown.length;
    let end = Math.min(this.raw.length, start + Math.max(1, Math.ceil(backlog / 10)));

    // Move forward to the end of the word we landed in, so the next reveal
    // does not begin mid-word.
    while (end < this.raw.length && WORD_START.test(this.raw[end])) end++;
    if (end < this.raw.length) end++; // include the trailing space

    // Never cut an inline code span in half. A partially revealed
    // `Strict-Transport-Security` renders as a stray backtick and, worse, the
    // fragment after it is parsed as prose. If the cut is inside an odd number
    // of backticks, extend to the closing backtick when it has arrived, or hold
    // at the opening one until it does.
    const ticks = (this.raw.slice(0, end).match(/`/g) || []).length;
    if (ticks % 2 === 1) {
      const next = this.raw.indexOf("`", end);
      if (next !== -1) {
        end = next + 1;
      } else {
        end = this.raw.slice(0, end).lastIndexOf("`");
      }
    }

    this.shown = this.raw.slice(0, end);
  }

  /** Everything revealed except the newest word. */
  get body(): string {
    return this.splitTail().body;
  }

  /** The newest revealed word — rendered with OpenBot's blur-in tail. */
  get tail(): string {
    return this.splitTail().tail;
  }

  /**
   * Split the newest word off the revealed text, unless doing so would cut an
   * inline code span.
   *
   * The renderer parses `body` and `tail` as separate markdown fragments (the
   * tail gets the blur-in treatment). If the opening backtick lands in `body`
   * and the closing one in `tail`, each fragment sees an unmatched backtick and
   * the code span renders as literal backticks with a blank line between them.
   * In that case the whole string stays in `body`; the tail effect is not worth
   * corrupting the answer for.
   */
  private splitTail(): { body: string; tail: string } {
    if (!this.active) return { body: this.shown, tail: "" };
    const m = this.shown.match(/\S+\s*$/);
    if (!m) return { body: this.shown, tail: "" };
    const body = this.shown.slice(0, this.shown.length - m[0].length);
    const ticks = (body.match(/`/g) || []).length;
    if (ticks % 2 === 1) return { body: this.shown, tail: "" };
    return { body, tail: m[0].trimEnd() };
  }

  /** `done` event: show the full text sharply; hold it until the caller clears. */
  finish() {
    this.stop();
    this.shown = this.raw;
    this.active = false;
  }

  /** Buffer cleared server-side (`clear` event) — drop everything. */
  reset() {
    this.stop();
    this.raw = "";
    this.shown = "";
    this.active = false;
  }

  /** Stop the pump (view teardown). Safe to call repeatedly. */
  stop() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }
}
