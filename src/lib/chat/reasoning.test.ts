import { describe, it, expect, beforeEach } from "vitest";
import {
  ReasoningStore,
  reasoning,
  parseStoredReasoning,
  REASONING_ROUND_SEPARATOR,
} from "./reasoning.svelte";

/**
 * The live reasoning store.
 *
 * Every test here is about a specific way reasoning used to be lost, because all
 * five of those failures presented as "the feature just isn't there" rather than
 * as a bug — which is why they all survived.
 */

describe("ReasoningStore", () => {
  let store: ReasoningStore;

  beforeEach(() => {
    store = new ReasoningStore();
  });

  const chunk = (thread: string, content: string, bot = "b1") =>
    store.ingest({ thread_id: thread, content, bot_id: bot });

  /** The one that matters most: a round boundary must not erase the trace. */
  it("keeps reasoning across a round boundary", () => {
    chunk("t1", "I should check the logs first.");
    store.endRound("t1");
    chunk("t1", "The logs show a timeout.");

    const rounds = store.byThread.t1.rounds;
    expect(rounds).toHaveLength(2);
    expect(rounds[0].text).toContain("check the logs");
    expect(rounds[1].text).toContain("timeout");
    // The finished round must not still claim to be receiving tokens, or the UI
    // would keep a cursor blinking on a stretch that ended minutes ago.
    expect(rounds[0].live).toBe(false);
    expect(rounds[1].live).toBe(true);
  });

  /**
   * The separator has to say *why* the trace continues. "after tool results" is
   * the part worth reading when an answer looks wrong — it is where the model
   * reacts to what it found.
   */
  it("marks which stretches came after tool results", () => {
    chunk("t1", "First.");
    store.endRound("t1");
    chunk("t1", "Second.");

    const rounds = store.byThread.t1.rounds;
    expect(rounds[0].afterTools).toBe(false);
    expect(rounds[1].afterTools).toBe(true);
    expect(store.text("t1")).toContain(REASONING_ROUND_SEPARATOR);
  });

  /**
   * Some providers start reasoning again after a tool result with no explicit
   * round boundary at all. Treating that as a continuation produces one
   * undifferentiated wall; dropping it loses thinking. It has to become a round.
   */
  it("opens a new round when reasoning resumes with no boundary", () => {
    chunk("t1", "One.");
    store.finish("t1");
    chunk("t1", "Two.");

    const rounds = store.byThread.t1.rounds;
    expect(rounds).toHaveLength(2);
    expect(rounds[0].live).toBe(false);
    expect(rounds[1].live).toBe(true);
  });

  it("separates concurrent threads", () => {
    chunk("t1", "Agent A thinking.");
    chunk("t2", "Agent B thinking.");

    expect(store.byThread.t1.rounds[0].text).toBe("Agent A thinking.");
    expect(store.byThread.t2.rounds[0].text).toBe("Agent B thinking.");
    // And a boundary on one must not close the other.
    store.endRound("t1");
    expect(store.byThread.t1.rounds[0].live).toBe(false);
    expect(store.byThread.t2.rounds[0].live).toBe(true);
  });

  it("names the agent doing the thinking", () => {
    chunk("t1", "Thinking.", "bot-42");
    expect(store.byThread.t1.botId).toBe("bot-42");
  });

  it("reports whether anything is being thought right now", () => {
    expect(store.thinking("t1")).toBe(false);
    chunk("t1", "Thinking.");
    expect(store.thinking("t1")).toBe(true);
    store.finish("t1");
    expect(store.thinking("t1")).toBe(false);
  });

  /** A boundary with nothing in flight is a no-op, not a stray round. */
  it("ignores a round boundary it has no stretch for", () => {
    expect(store.endRound("nope")).toBe(false);
    chunk("t1", "Thinking.");
    expect(store.endRound("t1")).toBe(true);
    expect(store.endRound("t1")).toBe(false);
    expect(store.byThread.t1.rounds).toHaveLength(1);
  });

  /**
   * Bounds, because reasoning is model output and model output is unbounded. A
   * runaway trace is a memory leak in a long session, and the cap keeps the most
   * recent thinking — the part still relevant to what happens next.
   */
  it("bounds a runaway trace", () => {
    for (let i = 0; i < 60; i++) chunk("t1", `thought ${i} `);
    const text = store.byThread.t1.rounds[0].text;
    expect(text.length).toBeLessThanOrEqual(40_000);
    // Most recent kept, oldest dropped.
    expect(text).toContain("thought 59");
  });

  it("bounds the number of rounds", () => {
    for (let i = 0; i < 40; i++) {
      chunk("t1", `round ${i}`);
      store.endRound("t1");
    }
    expect(store.byThread.t1.rounds.length).toBeLessThanOrEqual(24);
  });

  /**
   * An empty first round would render as a bare separator line above nothing.
   * It happens when a round boundary lands before any reasoning arrived, which is
   * an ordinary turn for a model that answers without thinking.
   */
  it("does not leave an empty opening round", () => {
    store.endRound("t1");
    chunk("t1", "Actually, here is the answer.");
    // Nothing was open, so there is still exactly one stretch.
    expect(store.byThread.t1.rounds).toHaveLength(1);
  });

  it("drops a thread that is genuinely gone", () => {
    chunk("t1", "Thinking.");
    store.drop("t1");
    expect(store.byThread.t1).toBeUndefined();
    // Dropping something absent is not an error.
    store.drop("t1");
  });

  it("ignores malformed chunks rather than throwing", () => {
    expect(store.ingest(null)).toBe(false);
    expect(store.ingest({})).toBe(false);
    expect(store.ingest({ thread_id: "t1" })).toBe(false);
    expect(store.ingest({ thread_id: "", content: "x" })).toBe(false);
    expect(store.ingest({ thread_id: "t1", content: "" })).toBe(false);
    expect(store.byThread).toEqual({});
  });

  /** One store, because three views of a run must not become three runs. */
  it("exposes a shared singleton", () => {
    expect(reasoning).toBeInstanceOf(ReasoningStore);
    reasoning.ingest({ thread_id: "shared", content: "Hi", bot_id: "b" });
    expect(reasoning.byThread.shared).toBeTruthy();
    reasoning.reset();
  });
});

describe("parseStoredReasoning", () => {
  /**
   * The stored form is one plain string with `---` separators, because it lives
   * in a JSON column and has to stay readable in a database browser. Rendering
   * needs the boundaries back, so they are recovered on the way out.
   */
  it("recovers round boundaries from the stored form", () => {
    const stored = [
      "I should check the logs.",
      "*(after tool results)*\n\nThe logs show a timeout on line 40.",
    ].join("\n\n---\n\n");

    const rounds = parseStoredReasoning(stored);
    expect(rounds).toHaveLength(2);
    expect(rounds[0].text).toBe("I should check the logs.");
    expect(rounds[0].afterTools).toBe(false);
    expect(rounds[1].afterTools).toBe(true);
    // The label is a marker for the renderer, not part of the prose.
    expect(rounds[1].text).toBe("The logs show a timeout on line 40.");
    expect(rounds[1].live).toBe(false);
  });

  it("handles a single round with no separators", () => {
    const rounds = parseStoredReasoning("Just one thought.");
    expect(rounds).toHaveLength(1);
    expect(rounds[0].afterTools).toBe(false);
  });

  it("returns nothing for absent or blank reasoning", () => {
    expect(parseStoredReasoning(null)).toEqual([]);
    expect(parseStoredReasoning(undefined)).toEqual([]);
    expect(parseStoredReasoning("")).toEqual([]);
    expect(parseStoredReasoning("   \n  ")).toEqual([]);
  });

  it("drops empty stretches rather than rendering bare separators", () => {
    const rounds = parseStoredReasoning("One.\n\n---\n\n\n\n---\n\nTwo.");
    expect(rounds).toHaveLength(2);
  });
});
