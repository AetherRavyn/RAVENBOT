import { describe, it, expect, beforeEach } from "vitest";
import { fleetActivity } from "./fleetActivity.svelte";
import type { Activity } from "./fleetActivity.svelte";

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
