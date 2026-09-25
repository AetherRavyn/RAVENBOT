// Client-side word-by-word reveal decoupled from token arrival (OpenBot
// streaming pattern): SSE deltas append to the RAW buffer; the visible string
// advances one word every --stream-gap (60ms), so the reveal stays smooth
// whether tokens burst or trickle. The newest word keeps its blur-in tail
// until the stream FINISHES (not merely until the pump catches up), so the
// tail never snaps sharp mid-run. Reduced motion shows everything instantly.
import { prefersReducedMotion } from "$lib/a11y";

const WORD = /\S+\s*/g;

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

  private ensurePump() {
    if (prefersReducedMotion()) {
      this.stop();
      this.shown = this.raw;
      return;
    }
    if (this.timer) return;
    this.active = true;
    this.timer = setInterval(() => {
      if (this.shown.length >= this.raw.length) {
        // Caught up, but the run is still live: stop ticking while KEEPING
        // `active` so the tail stays on the newest word until finish().
        this.stop();
        return;
      }
      WORD.lastIndex = this.shown.length;
      const m = WORD.exec(this.raw.slice(this.shown.length));
      this.shown = this.raw.slice(0, this.shown.length + (m ? m[0].length : 1));
    }, gapMs());
  }

  /** Everything revealed except the newest word. */
  get body(): string {
    if (!this.active) return this.shown;
    const m = this.shown.match(/\S+\s*$/);
    return m ? this.shown.slice(0, this.shown.length - m[0].length) : this.shown;
  }

  /** The newest revealed word — rendered with OpenBot's blur-in tail. */
  get tail(): string {
    if (!this.active) return "";
    return this.shown.match(/\S+(?=\s*$)/)?.[0] ?? "";
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
