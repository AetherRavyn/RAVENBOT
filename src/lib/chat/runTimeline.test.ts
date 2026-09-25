import { describe, it, expect } from "vitest";
import { describeRunEvent, RunTimeline } from "./runTimeline.svelte";

describe("run timeline", () => {
  it("maps known event kinds and ignores the rest", () => {
    expect(describeRunEvent({ kind: "run_started", bot_id: "a" })).toEqual({ botId: "a", phase: "start" });
    expect(describeRunEvent({ kind: "tool_started", bot_id: "a", name: "shell_exec" })).toEqual({
      botId: "a", phase: "tool", detail: "shell_exec",
    });
    expect(describeRunEvent({ kind: "done", bot_id: "a" })).toEqual({ botId: "a", phase: "done" });
    expect(describeRunEvent({ kind: "delta", bot_id: "a", content: "hi" })).toBeNull();
    expect(describeRunEvent({ kind: "usage", bot_id: "a", tokens: 5 })).toBeNull();
    expect(describeRunEvent({ kind: "run_started" })).toBeNull();
    expect(describeRunEvent(null)).toBeNull();
  });

  it("appends entries with increasing ids and timestamps", () => {
    const tl = new RunTimeline();
    expect(tl.track({ kind: "run_started", bot_id: "a" }, 100)).toBe(true);
    expect(tl.track({ kind: "approval_requested", bot_id: "b" }, 200)).toBe(true);
    expect(tl.track({ kind: "clear", bot_id: "a" }, 300)).toBe(false);
    expect(tl.events.map((e) => [e.botId, e.phase, e.at])).toEqual([
      ["a", "start", 100],
      ["b", "approval", 200],
    ]);
    expect(tl.events[1].id).toBeGreaterThan(tl.events[0].id);
  });

  it("caps history and resets between runs", () => {
    const tl = new RunTimeline();
    for (let i = 0; i < 60; i++) tl.track({ kind: "tool_started", bot_id: "a", name: `t${i}` }, i);
    expect(tl.events.length).toBe(40);
    expect(tl.events[tl.events.length - 1].detail).toBe("t59");
    expect(tl.events[0].detail).toBe("t20");
    tl.reset();
    expect(tl.events).toEqual([]);
  });
});
