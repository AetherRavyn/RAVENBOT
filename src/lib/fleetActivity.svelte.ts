/**
 * Fleet activity: what every agent is doing, derived from the stream.
 *
 * This is the one place that listens to `agent-stream`, and it derives live
 * state from the feed rather than reading the persisted `bot.status` column —
 * a status written at the end of a run tells you nothing about what is
 * happening now.
 *
 * Two things come out of it:
 *
 *  - an **activity** badge (working / needs you / replied / idle), which is
 *    about urgency, and
 *  - a **mood** for the avatar face, which is about what the agent is
 *    experiencing.
 *
 * They are not the same thing, which is why there are two. A failed run is not
 * `working` — it has stopped — but it is the thing a user most needs to see, so
 * the mood precedence puts failure above everything while the badge stays
 * quiet once the run is over.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { computeMoods, type AvatarMood } from "$lib/avatar";
import { handoffs } from "$lib/handoffs.svelte";

export type Activity = "working" | "attention" | "responded" | "idle";

/** How long a "just replied" state holds before the agent is idle again. */
const RESPONDED_HOLD_MS = 3000;
/** How long a failure face holds, so it is readable but not a permanent scar. */
const FAILED_HOLD_MS = 8000;

class FleetActivity {
  states = $state<Record<string, Activity>>({});
  moods = $state<Record<string, AvatarMood>>({});

  private timers = new Map<string, ReturnType<typeof setTimeout>>();
  private unlisten: UnlistenFn | null = null;
  private refs = 0;
  private listening = false;

  /**
   * The badge for an agent.
   *
   * An agent that another agent has just handed work to counts as working, even
   * if it has not emitted anything yet. A delegation runs a whole child run in
   * a different thread, and the *target's* stream events are routed to that
   * thread's emitter — so in the office the colleague doing the work looks
   * completely idle while it works. Without this, a working office shows one
   * busy agent and several empty chairs.
   */
  get(botId: string): Activity {
    if (handoffs.targets[botId]) return "working";
    return this.states[botId] ?? "idle";
  }

  /** The face to draw. A missing entry means idle, not unknown. */
  mood(botId: string): AvatarMood {
    if (handoffs.targets[botId]) return "working";
    return this.moods[botId] ?? "idle";
  }

  /** How many agents need a person right now. Drives the rail badge. */
  attentionCount(botId: string): number {
    return this.states[botId] === "attention" ? 1 : 0;
  }

  /**
   * What the whole fleet is doing, for the elements that have one place to
   * show it — the title-bar mark, the window title, anything global.
   *
   * Derived from `states` rather than kept as a second flag, because two
   * sources for "is it busy" is two sources that can disagree: a mark that
   * says working while the badge says idle is a bug nobody can reproduce.
   *
   * `attention` outranks `working`. An agent waiting on a person is a stricter
   * fact than an agent running — the run has *stopped* — and a mark that
   * glowed "busy" while a decision sat parked would be answering a question
   * nobody asked. `responded` counts as neither: the work is over, and a logo
   * that stayed lit after the answer arrived would teach people to ignore it.
   *
   * The return type is deliberately narrower than `Activity`. It *cannot* be
   * `responded`, and if it could the type would not be the thing that caught it.
   */
  get busiest(): "working" | "attention" | null {
    let running = false;
    for (const s of Object.values(this.states)) {
      if (s === "attention") return "attention";
      if (s === "working") running = true;
    }
    // A delegation in flight counts even though its target's stream lives in
    // another thread — the same hole `get()` plugs for a single agent.
    if (Object.keys(handoffs.targets).length > 0) running = true;
    return running ? "working" : null;
  }

  /** Subscribe once; refcounted so HMR and re-mounts do not double-listen. */
  async start(): Promise<void> {
    this.refs++;
    if (this.unlisten || this.listening) return;
    this.listening = true;
    const fn = await listen<any>("agent-stream", (e) => this.handle(e.payload));
    this.listening = false;
    // `stop()` may have been called while the listener was being registered.
    if (this.refs === 0) {
      fn();
      return;
    }
    this.unlisten = fn;
  }

  stop(): void {
    this.refs = Math.max(0, this.refs - 1);
    if (this.refs > 0) return;
    this.unlisten?.();
    this.unlisten = null;
    for (const t of this.timers.values()) clearTimeout(t);
    this.timers.clear();
    this.states = {};
    this.moods = {};
    handoffs.clear();
  }

  handle(p: any): void {
    // Folded first and without an early return: a delegation event carries a
    // `bot_id`, so the guard below would otherwise let it past, but the shape
    // of the rest of the switch does not apply to it and it needs no badge.
    handoffs.ingest(p);

    const botId = p?.bot_id;
    if (!botId) return;

    switch (p.kind) {
      case "run_started":
      case "delta":
      case "tool_started":
      case "tool_finished":
        // A new run supersedes an old failure: the agent is no longer sitting
        // on a failure, it is working.
        this.set(botId, "working");
        break;

      case "approval_requested":
      case "question_asked":
      case "paused":
        this.set(botId, "attention");
        break;

      case "approval_decided":
      case "question_answered":
        // The question was answered, so the agent is back to work.
        this.set(botId, "working");
        break;

      case "error":
      case "error_card":
        this.fail(botId);
        break;

      case "done":
        this.replied(botId);
        break;
    }
  }

  private set(botId: string, next: Activity, mood?: AvatarMood): void {
    this.hold(botId);
    this.states[botId] = next;
    this.moods[botId] = mood ?? defaultMood(next);
  }

  private replied(botId: string): void {
    this.set(botId, "responded", "responded");
    this.expire(botId, RESPONDED_HOLD_MS, "idle");
  }

  /**
   * A failed run.
   *
   * Held longer than a reply because it is the one state a user must not miss,
   * but still temporary — a permanently sad face stops meaning anything.
   */
  private fail(botId: string): void {
    this.set(botId, "responded", "failed");
    this.expire(botId, FAILED_HOLD_MS, "idle");
  }

  private expire(botId: string, ms: number, to: Activity): void {
    this.timers.set(
      botId,
      setTimeout(() => {
        this.timers.delete(botId);
        this.states[botId] = to;
        this.moods[botId] = "idle";
      }, ms),
    );
  }

  private hold(botId: string): void {
    const t = this.timers.get(botId);
    if (t) {
      clearTimeout(t);
      this.timers.delete(botId);
    }
  }
}

function defaultMood(activity: Activity): AvatarMood {
  switch (activity) {
    case "working":
      return "working";
    case "attention":
      return "waiting";
    default:
      return "idle";
  }
}

export const fleetActivity = new FleetActivity();

export { computeMoods };
