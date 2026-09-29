/**
 * Live agent-to-agent handoffs.
 *
 * Delegation used to leave exactly one trace: a tool result buried inside the
 * calling agent's message, saying a colleague did some work and returning the
 * text. In an office that reads as the lead going quiet for a while. Whether
 * that quiet meant "thinking", "stuck", or "asked Sam to check the logs" was
 * not something the office could tell — and "asked Sam" is the one a user
 * watching an office actually wants to know about, because it is the moment the
 * office is behaving like an office.
 *
 * So the runtime announces a handoff as a first-class event, and this holds the
 * ones in flight. It is deliberately *live* state rather than history: once a
 * delegation has landed, its content is in the conversation where it belongs,
 * and keeping a second copy here would be a second thing to fall out of sync.
 * What is kept is the in-flight edge and its outcome, which is what the
 * conversation does not carry.
 *
 * A refused handoff is kept and marked, not dropped. The delegate list and the
 * capability now refuse a handoff the agent was told it could make, and the
 * first question about an agent that suddenly cannot ask for help is whether it
 * tried — so the attempt is shown with the reason.
 */

export interface Handoff {
  /** Unique to this handoff, so the accepted and completed edges join up. */
  key: string;
  /**
   * Identity of the *edge*, shared by both events.
   *
   * Two agents asking the same agent in the same millisecond are two handoffs,
   * not one, so the key cannot be the edge identity alone — but the completion
   * event has to find the edge it belongs to, and it carries no sequence number
   * of its own. This is what makes the join.
   */
  core: string;
  threadId: string;
  childThreadId: string | null;
  fromBotId: string;
  toBotId: string;
  toBotName: string;
  instruction: string;
  startedAt: number;
  /** `true` once the work has landed or failed. */
  done: boolean;
  /** Set when the handoff was refused or the run failed. */
  error: string | null;
  /**
   * What the agent said, once it has said it.
   *
   * Held here rather than left to the conversation because an office has
   * exactly one thread and the delegated agent's is a *different* one, with no
   * way to navigate to it. So the answer is either shown where the handoff is
   * or not shown at all.
   */
  response: string | null;
  /** Whether the user has opened the reply. */
  expanded: boolean;
}

/** How long a finished handoff stays before the row fades out. */
const SETTLE_MS = 6000;

class Handoffs {
  /** In flight and just-landed, newest first. */
  active = $state<Handoff[]>([]);

  private timers = new Map<string, ReturnType<typeof setTimeout>>();
  private seq = 0;

  /**
   * Which agents are currently being asked by somebody else.
   *
   * Keyed by the *target*, because that is what the office wants to know: an
   * agent that is busy on someone else's request should not read as idle when
   * it is working flat out.
   */
  targets = $derived.by(() => {
    const out: Record<string, string> = {};
    for (const h of this.active) if (!h.done) out[h.toBotId] = h.fromBotId;
    return out;
  });

  /** The handoffs in one thread, for a conversation view. */
  forThread(threadId: string): Handoff[] {
    return this.active.filter((h) => h.threadId === threadId);
  }

  /**
   * Fold a `Delegation` stream event in.
   *
   * Called with the raw event payload so this stays a passive store; the
   * subscription lives with the other stream consumers.
   */
  ingest(p: any): void {
    // `StreamEvent` is tagged `#[serde(tag = "kind", rename_all = "snake_case")]`.
    if (!p || p.kind !== "delegation") return;
    const core = `${p.thread_id ?? ""}:${p.bot_id ?? ""}:${p.to_bot_id ?? ""}`;
    const key = this.keyFor(core, p);
    const existing = this.active.find((h) => h.key === key);

    if (existing) {
      if (!p.done) return;
      this.clearTimer(key);
      existing.done = true;
      existing.error = typeof p.error === "string" ? p.error : null;
      if (typeof p.response === "string" && p.response) {
        existing.response = p.response;
        // Open by default only if it is short enough to be a line. A long reply
        // collapsed is a wall of text in the middle of a conversation.
        existing.expanded = p.response.length <= 240;
      }
      this.settle(key, existing.error !== null);
      return;
    }

    if (p.done) {
      // No acceptance to join, so this is the only event there will ever be.
      // That is the normal shape of a *refused* handoff: the runtime decides
      // before it creates a thread, so there is nothing to announce on the way
      // in. Dropping it would make a refused handoff invisible, which is the
      // one case a user most needs to see — an agent that cannot ask for help
      // looks exactly like an agent that has stopped trying.
      const h: Handoff = {
        key,
        core,
        threadId: String(p.thread_id ?? ""),
        childThreadId: p.child_thread_id ? String(p.child_thread_id) : null,
        fromBotId: String(p.bot_id ?? ""),
        toBotId: String(p.to_bot_id ?? ""),
        toBotName: String(p.to_bot_name ?? ""),
        instruction: String(p.instruction ?? ""),
        startedAt: Date.now(),
        done: true,
        error: typeof p.error === "string" ? p.error : null,
        response: typeof p.response === "string" ? p.response : null,
        expanded: false,
      };
      this.active = [h, ...this.active];
      this.settle(key, h.error !== null);
      return;
    }

    this.active = [
      {
        key,
        core,
        threadId: String(p.thread_id ?? ""),
        childThreadId: p.child_thread_id ? String(p.child_thread_id) : null,
        fromBotId: String(p.bot_id ?? ""),
        toBotId: String(p.to_bot_id ?? ""),
        toBotName: String(p.to_bot_name ?? ""),
        instruction: String(p.instruction ?? ""),
        startedAt: Date.now(),
        done: false,
        error: null,
        response: null,
        expanded: false,
      },
      ...this.active,
    ];
  }

  /** Show or hide a landed reply. */
  toggle(key: string): void {
    const h = this.active.find((x) => x.key === key);
    if (h) h.expanded = !h.expanded;
  }

  /**
   * Start the clock on a finished row.
   *
   * A refusal outlives a normal one: it explains why nothing is happening, so
   * it is the row a user goes back to read.
   */
  private settle(key: string, isRefusal: boolean): void {
    const hold = isRefusal ? SETTLE_MS * 3 : SETTLE_MS;
    this.timers.set(
      key,
      setTimeout(() => {
        this.timers.delete(key);
        this.active = this.active.filter((h) => h.key !== key);
      }, hold),
    );
  }

  /**
   * One key for a handoff, across its two events.
   *
   * The runtime emits twice — accepted, then done — and the second has to find
   * the first. It carries the same `thread_id`, `bot_id` and `to_bot_id`, so
   * those three identify the edge; the counter is only a tiebreak for two
   * identical handoffs opened in the same millisecond, which would otherwise
   * collapse into one row.
   */
  private keyFor(core: string, p: any): string {
    if (p.done) {
      const open = this.active.find((h) => h.core === core && !h.done);
      if (open) return open.key;
    } else {
      this.seq += 1;
    }
    return `${core}#${this.seq}`;
  }

  /** Reset, for teardown. */
  clear(): void {
    for (const t of this.timers.values()) clearTimeout(t);
    this.timers.clear();
    this.active = [];
  }

  private clearTimer(key: string): void {
    const t = this.timers.get(key);
    if (t) {
      clearTimeout(t);
      this.timers.delete(key);
    }
  }
}

export const handoffs = new Handoffs();
