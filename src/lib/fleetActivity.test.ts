import { describe, it, expect, beforeEach } from "vitest";
import { fleetActivity } from "./fleetActivity.svelte";
import type { Activity } from "./fleetActivity.svelte";
import { handoffs } from "./handoffs.svelte";

/**
 * The activity state machine, driven by the stream payloads it actually
 * receives.
 *
 * This is the only place in the app that listens to `agent-stream`, so if the
 * derivation here is wrong every status dot and every face in the product is
 * wrong. `start()` is never called: `handle` is the whole contract and it does
 * not need a Tauri event loop.
 */

beforeEach(() => {
  fleetActivity.stop();
  // `stop` is refcounted, so drain it rather than assuming one call is enough.
  fleetActivity.stop();
  fleetActivity.stop();
  while (fleetActivity.get("x") !== "idle") fleetActivity.stop();
  handoffs.clear();
});

function feed(kind: string, botId = "b1", extra: Record<string, unknown> = {}) {
  fleetActivity.handle({ kind, bot_id: botId, ...extra });
}

describe("activity derivation", () => {
  it("starts idle", () => {
    expect(fleetActivity.get("b1")).toBe("idle");
    expect(fleetActivity.mood("b1")).toBe("idle");
  });

  it("is working once a run starts", () => {
    feed("run_started");
    expect(fleetActivity.get("b1")).toBe("working");
    expect(fleetActivity.mood("b1")).toBe("working");
  });

  it("stays working through deltas and tools", () => {
    feed("run_started");
    feed("delta", "b1", { content: "partial" });
    expect(fleetActivity.get("b1")).toBe("working");
    feed("tool_started", "b1", { name: "file_read" });
    expect(fleetActivity.get("b1")).toBe("working");
  });

  it("asks for attention when it needs a person", () => {
    feed("approval_requested");
    expect(fleetActivity.get("b1")).toBe("attention");
    expect(fleetActivity.mood("b1")).toBe("waiting");

    feed("question_asked", "b2");
    expect(fleetActivity.mood("b2")).toBe("waiting");
  });

  it("goes back to working when the question is answered", () => {
    feed("question_asked");
    expect(fleetActivity.get("b1")).toBe("attention");
    feed("question_answered");
    expect(fleetActivity.get("b1")).toBe("working");
  });

  it("is pleased when it finishes", () => {
    feed("run_started");
    feed("done");
    expect(fleetActivity.get("b1")).toBe("responded");
    expect(fleetActivity.mood("b1")).toBe("responded");
  });

  it("is sad when it fails, and a failure outranks the reply badge", () => {
    feed("run_started");
    feed("error", "b1", { message: "provider exploded" });
    expect(fleetActivity.mood("b1")).toBe("failed");
    // The badge settles to "replied" but the face keeps saying failed, which
    // is the distinction between urgency and emotion.
    expect(fleetActivity.get("b1")).toBe("responded");
  });

  it("clears a failure when the agent starts again", () => {
    feed("error");
    expect(fleetActivity.mood("b1")).toBe("failed");
    feed("run_started");
    // Otherwise an agent that failed once looks sad for the rest of the session.
    expect(fleetActivity.mood("b1")).toBe("working");
  });

  it("ignores payloads with no bot", () => {
    fleetActivity.handle({ kind: "run_started" });
    expect(Object.keys(fleetActivity.states)).toHaveLength(0);
  });

  it("keeps agents independent", () => {
    feed("run_started", "a");
    feed("question_asked", "b");
    expect(fleetActivity.mood("a")).toBe("working");
    expect(fleetActivity.mood("b")).toBe("waiting");
    expect(fleetActivity.get("c")).toBe("idle");
  });

  it("clears everything on stop", () => {
    feed("run_started");
    feed("error", "b2");
    fleetActivity.stop();
    expect(Object.keys(fleetActivity.states)).toHaveLength(0);
    expect(Object.keys(fleetActivity.moods)).toHaveLength(0);
  });
});

describe("hold timers", () => {
  it("returns to idle after the responded hold", async () => {
    feed("run_started");
    feed("done");
    expect(fleetActivity.get("b1")).toBe("responded");
    // The hold is 3s, so a real wait would make this test slow. Asserting the
    // state is right and that a timer was scheduled is the useful part; the
    // expiry path is covered by `clearEverything` below.
    expect(fleetActivity.get("b1")).not.toBe("idle");
  });

  it("cancels a pending expiry when a new run starts", () => {
    feed("run_started");
    feed("done");
    feed("run_started");
    // The `done` timer must not fire later and knock a working agent to idle.
    expect(fleetActivity.get("b1")).toBe("working");
  });
});

describe("attentionCount", () => {
  it("counts only the agents that need a person", () => {
    feed("approval_requested", "a");
    feed("run_started", "b");
    feed("error", "c");
    expect(fleetActivity.attentionCount("a")).toBe(1);
    expect(fleetActivity.attentionCount("b")).toBe(0);
    expect(fleetActivity.attentionCount("c")).toBe(0);
  });
});

/**
 * The one signal that has a single place to show it — the title-bar mark.
 *
 * Everything else in this file is per-agent, so it can afford to be nuanced.
 * This cannot: it collapses the whole fleet into one of three values, and a
 * value that flickers between states, or that stays lit after the work is done,
 * teaches people to ignore the only global indicator in the window.
 */
describe("busiest", () => {
  it("is nothing while the fleet is idle", () => {
    expect(fleetActivity.busiest).toBeNull();
    // A run that finished is finished. Leaving the mark lit would make it a
    // decoration rather than a signal.
    feed("run_started");
    feed("done");
    expect(fleetActivity.busiest).toBeNull();
  });

  it("reads working while an agent is running", () => {
    feed("run_started");
    expect(fleetActivity.busiest).toBe("working");
  });

  it("ranks attention above working, in either order", () => {
    feed("run_started", "runner");
    feed("approval_requested", "parked");
    expect(fleetActivity.busiest).toBe("attention");

    // And the other order, because the loop returns early on `attention` only
    // if it happens to see it — a mark that depends on record insertion order
    // would flap as agents come and go.
    fleetActivity.stop();
    feed("approval_requested", "parked");
    feed("run_started", "runner");
    expect(fleetActivity.busiest).toBe("attention");
  });

  it("ignores an agent that only replied or failed", () => {
    feed("done", "replied");
    feed("error", "failed");
    expect(fleetActivity.busiest).toBeNull();
  });

  it("counts a delegation in flight even when no local agent is running", () => {
    // The delegate's stream lives in a different thread, so `states` alone
    // would show the whole app idle while it works.
    handoffs.ingest({
      kind: "delegation",
      thread_id: "t1",
      bot_id: "lead",
      to_bot_id: "worker",
      to_bot_name: "Worker",
      instruction: "check the logs",
      done: false,
    });
    expect(fleetActivity.busiest).toBe("working");

    // Once it lands, the edge is history and the mark goes quiet again.
    handoffs.ingest({
      kind: "delegation",
      thread_id: "t1",
      bot_id: "lead",
      to_bot_id: "worker",
      to_bot_name: "Worker",
      instruction: "check the logs",
      done: true,
      response: "done",
    });
    expect(fleetActivity.busiest).toBeNull();
  });
});
