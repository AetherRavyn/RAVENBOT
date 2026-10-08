/**
 * The reveal pump: owns the timer, feeds the budgeted step, and keeps the
 * trail of steps that are still fading.
 *
 * The maths lives in `./reveal` as pure functions so it can be tested without
 * a DOM. This is the part that has to be a class, because it owns a timer and
 * the trail is mutable state that a component re-render must not reset.
 */
import { prefersReducedMotion } from "$lib/a11y";
import { nextReveal, type RevealChunk } from "./reveal";

/** How often the shown text advances. */
const DEFAULT_STEP_MS = 34;
/**
 * How far behind the received text the shown text is allowed to settle.
 *
 * A reply that arrives in one chunk therefore reveals over roughly this long,
 * and a model that streams quickly does not run away from the reader.
 */
const DEFAULT_CATCH_UP_MS = 900;
/** A step stays in the trail this long after it was revealed. */
const TRAIL_TTL_MS = 420;

function stepMs(): number {
  if (typeof document === "undefined") return DEFAULT_STEP_MS;
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--stream-gap");
  const n = parseFloat(raw);
  return Number.isFinite(n) && n > 0 ? n : DEFAULT_STEP_MS;
}

export class RevealPump {
  /** Everything received so far. */
  raw = $state("");
  /** The prefix of `raw` currently shown. */
  shown = $state("");
  /** True while the stream is live, so the newest step keeps its fade. */
  active = $state(false);
  /** The steps still fading, oldest first. */
  trail = $state<RevealChunk[]>([]);

  private timer: ReturnType<typeof setInterval> | null = null;
  private budget = 0;
  private startedAt = 0;

  /**
   * Follow an externally-owned buffer.
   *
   * The caller appends provider deltas to its own string; this keeps the shown
   * prefix in step. A buffer that was shortened or replaced — a `clear` event,
   * a regenerated answer, a thread switch — restarts cleanly and kills the pump
   * immediately, so no stale tick can resurrect old text.
   */
  track(target: string): void {
    if (target === this.raw) return;
    const appended = this.raw === "" || target.startsWith(this.raw);
    this.raw = target;

    if (!appended || target === "") {
      this.reset();
      if (target === "") return;
    }

    if (prefersReducedMotion()) {
      this.shown = target;
      this.trail = [];
      this.active = false;
      this.stop();
      return;
    }
    this.ensurePump();
  }

  /** Begin a new stream from nothing. */
  start(): void {
    this.reset();
    this.active = true;
    this.ensurePump();
  }

  /** The run is over: show everything sharply and drop the trail. */
  finish(): void {
    this.stop();
    this.shown = this.raw;
    this.trail = [];
    this.active = false;
  }

  /** Drop everything. */
  reset(): void {
    this.stop();
    this.raw = "";
    this.shown = "";
    this.trail = [];
    this.active = false;
    this.budget = 0;
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  private ensurePump(): void {
    if (this.timer) return;
    this.active = true;
    if (!this.startedAt) this.startedAt = performance.now();
    this.timer = setInterval(() => this.tick(), stepMs());
  }

  private tick(): void {
    if (this.shown.length >= this.raw.length) {
      // Caught up. Stop ticking but stay active, so the newest step keeps
      // fading until the run actually finishes rather than snapping sharp
      // between words.
      this.stop();
      return;
    }

    const before = this.shown.length;
    const next = nextReveal({
      shownLength: this.shown.length,
      target: this.raw,
      streaming: true,
      budget: this.budget,
      stepMs: stepMs(),
      catchUpMs: DEFAULT_CATCH_UP_MS,
    });

    this.shown = this.raw.slice(0, next.length);
    this.budget = next.budget;

    const revealed = next.length - before;
    if (revealed > 0) {
      this.trail = [
        ...this.trail,
        { length: revealed, revealedAt: performance.now() },
      ];
      this.pruneTrail();
    }
  }

  private pruneTrail(): void {
    const now = performance.now();
    this.trail = this.trail.filter((c) => now - c.revealedAt < TRAIL_TTL_MS);
  }
}
