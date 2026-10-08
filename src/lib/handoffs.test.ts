import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { handoffs } from "./handoffs.svelte";

/**
 * The handoff store's whole job is joining a two-event edge and then forgetting
 * it. Both halves are easy to get subtly wrong in a way no type checker sees:
 * a completion that does not find its opening leaves a spinner running forever,
 * and a forgotten row leaves a permanent "delegating" badge on an idle office.
 */

const accepted = (over: Record<string, unknown> = {}) => ({
  kind: "delegation",
  bot_id: "lead",
  thread_id: "t1",
  child_thread_id: "child-1",
  to_bot_id: "sam",
  to_bot_name: "Sam",
  instruction: "check the logs",
  done: false,
  response: null,
  error: null,
  ...over,
});

const completed = (over: Record<string, unknown> = {}) => ({
  ...accepted({ done: true }),
  ...over,
});

describe("handoffs", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    handoffs.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("ignores events that are not delegations", () => {
    handoffs.ingest({ kind: "delta", bot_id: "lead", thread_id: "t1" });
    handoffs.ingest({ kind: "tool_started", bot_id: "lead", thread_id: "t1" });
    handoffs.ingest(null);
    handoffs.ingest(undefined);
    expect(handoffs.active).toHaveLength(0);
  });

  it("records an accepted handoff as in flight", () => {
    handoffs.ingest(accepted());
    expect(handoffs.active).toHaveLength(1);
    const [h] = handoffs.active;
    expect(h.fromBotId).toBe("lead");
    expect(h.toBotId).toBe("sam");
    expect(h.done).toBe(false);
  });

  /**
   * The join. The two events share no unique id, so if the completion cannot
   * find its opening the office shows an agent delegating forever.
   */
  it("a completion updates the row its acceptance opened", () => {
    handoffs.ingest(accepted());
    const key = handoffs.active[0].key;

    handoffs.ingest(completed({ response: "found it" }));

    expect(handoffs.active).toHaveLength(1);
    expect(handoffs.active[0].key).toBe(key);
    expect(handoffs.active[0].done).toBe(true);
  });

  it("keeps the row as working until it is settled, then forgets it", () => {
    handoffs.ingest(accepted());
    handoffs.ingest(completed());

    // Still there immediately — the reply is what the user needs to catch.
    expect(handoffs.active).toHaveLength(1);

    vi.advanceTimersByTime(60_000);
    expect(handoffs.active).toHaveLength(0);
  });

  /**
   * Two handoffs between the same pair carry identical event fields, so the
   * join has to be per-occurrence or the second completion closes the first
   * row and the second one spins forever.
   */
  it("two handoffs between the same agents stay separate", () => {
    handoffs.ingest(accepted());
    handoffs.ingest(accepted());
    expect(handoffs.active).toHaveLength(2);
    const keys = handoffs.active.map((h) => h.key);
    expect(new Set(keys).size).toBe(2);

    handoffs.ingest(completed());
    // The oldest open edge is the one a completion closes.
    expect(handoffs.active.filter((h) => h.done)).toHaveLength(1);
    expect(handoffs.active.filter((h) => !h.done)).toHaveLength(1);
  });

  it("a refused handoff is kept, with its reason, and lives longer", () => {
    handoffs.ingest(accepted());
    handoffs.ingest(completed({ error: "not on your delegate list" }));

    const [h] = handoffs.active;
    expect(h.done).toBe(true);
    expect(h.error).toContain("delegate list");
    // A refusal explains why nothing is happening, so it outlives a normal row.
    vi.advanceTimersByTime(10_000);
    expect(handoffs.active).toHaveLength(1);
    vi.advanceTimersByTime(60_000);
    expect(handoffs.active).toHaveLength(0);
  });

  /** A refusal never created a thread, so pointing at one would mislead. */
  it("a refused handoff carries no child thread", () => {
    handoffs.ingest(accepted({ done: true, child_thread_id: null, error: "nope" }));
    expect(handoffs.active[0].childThreadId).toBeNull();
  });

  it("reports the in-flight target so a working agent is not read as idle", () => {
    handoffs.ingest(accepted());
    expect(handoffs.targets.sam).toBe("lead");

    handoffs.ingest(completed());
    // Landed: Sam is no longer being waited on.
    expect(handoffs.targets.sam).toBeUndefined();
  });

  it("filters by thread for a conversation view", () => {
    handoffs.ingest(accepted({ thread_id: "t1" }));
    handoffs.ingest(accepted({ thread_id: "t2", to_bot_id: "ada" }));
    expect(handoffs.forThread("t1")).toHaveLength(1);
    expect(handoffs.forThread("t1")[0].toBotId).toBe("sam");
    expect(handoffs.forThread("nope")).toHaveLength(0);
  });

  /**
   * A completion with no acceptance is indistinguishable from a refusal — the
   * event carries no marker saying which — so it is shown rather than dropped.
   * A row that fades after a few seconds is a small cost; a refusal that never
   * appears is the failure this store exists to prevent.
   */
  it("a completion with no matching acceptance still shows, then settles", () => {
    handoffs.ingest(completed());
    expect(handoffs.active).toHaveLength(1);
    expect(handoffs.active[0].done).toBe(true);
    vi.advanceTimersByTime(60_000);
    expect(handoffs.active).toHaveLength(0);
  });

  it("a repeated acceptance is not duplicated", () => {
    handoffs.ingest(accepted());
    handoffs.ingest(accepted());
    expect(handoffs.active).toHaveLength(2);
  });
});
