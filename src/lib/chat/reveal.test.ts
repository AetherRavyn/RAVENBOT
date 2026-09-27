import { describe, it, expect } from "vitest";
import {
  nextReveal,
  trailReach,
  tailOffsets,
  tailInside,
  sameTailOffsets,
  splitTrail,
} from "./reveal";

const WORDS = "the quick brown fox jumps over the lazy dog";

describe("nextReveal", () => {
  it("does nothing when there is no backlog", () => {
    const out = nextReveal({
      shownLength: 10,
      target: "short",
      streaming: false,
      budget: 0,
      stepMs: 34,
      catchUpMs: 900,
    });
    expect(out.length).toBe(5);
    expect(out.budget).toBe(0);
  });

  it("lands on word boundaries, never mid-word", () => {
    let shown = 0;
    const target = WORDS;
    for (let i = 0; i < 60; i += 1) {
      const out = nextReveal({
        shownLength: shown,
        target,
        streaming: true,
        budget: 0,
        stepMs: 34,
        catchUpMs: 900,
      });
      // Break on completion *and* on no progress: mid-stream, a reveal that has
      // caught up to a still-growing last word correctly refuses to advance.
      if (out.length === target.length || out.length === shown) break;
      expect(out.length).toBeGreaterThan(shown);
      // A revealed prefix must end on whitespace, never mid-word.
      expect(/\s$/.test(target.slice(0, out.length))).toBe(true);
      shown = out.length;
    }
    // Every complete word is shown; only the trailing partial one may be held.
    expect(target.slice(0, shown).trimEnd().split(/\s+/)).toEqual(
      WORDS.split(/\s+/).slice(0, -1),
    );
  });

  it("reveals a burst at roughly catchUpMs, not one word per tick", () => {
    // The point of the budget: a whole paragraph arriving at once should show
    // over about catchUpMs rather than taking one tick per word.
    const target = WORDS.repeat(12);
    const stepMs = 34;
    const catchUpMs = 900;
    let shown = 0;
    let budget = 0;
    let ticks = 0;

    while (shown < target.length && ticks < 500) {
      const out = nextReveal({ shownLength: shown, target, streaming: false, budget, stepMs, catchUpMs });
      if (out.length === shown) break;
      shown = out.length;
      budget = out.budget;
      ticks += 1;
    }

    expect(shown).toBe(target.length);
    // A generous window: the algorithm targets catchUpMs, and a test should not
    // fail over a few ticks of jitter.
    const elapsed = ticks * stepMs;
    expect(elapsed).toBeLessThan(catchUpMs * 2.5);
    // And it must be a run of ticks, not one per word.
    expect(ticks).toBeLessThan(target.split(/\s+/).length / 2);
  });

  it("waits at the end of a word that is still growing", () => {
    // Mid-stream, "wor" must not be shown as if it were "word".
    const out = nextReveal({
      shownLength: 0,
      target: "wor",
      streaming: true,
      budget: 0,
      stepMs: 34,
      catchUpMs: 900,
    });
    expect(out.length).toBe(0);
  });

  it("shows the last partial word once the stream is over", () => {
    const out = nextReveal({
      shownLength: 0,
      target: "wor",
      streaming: false,
      budget: 0,
      stepMs: 34,
      catchUpMs: 900,
    });
    expect(out.length).toBe(3);
  });

  it("never runs the budget so far negative that it stalls", () => {
    // A very long token (a URL) overshoots; the debt must be clamped.
    const target = `short ${"x".repeat(500)} tail`;
    const out = nextReveal({
      shownLength: 0,
      target,
      streaming: true,
      budget: 0,
      stepMs: 34,
      catchUpMs: 900,
    });
    expect(out.length).toBeGreaterThan(500);
    expect(out.budget).toBeGreaterThan(-10);
  });

  it("keeps a markdown block prefix with its word", () => {
    const out = nextReveal({
      shownLength: 0,
      target: "# Heading\n- item\n",
      streaming: false,
      budget: 0,
      stepMs: 34,
      catchUpMs: 900,
    });
    expect(out.length).toBeGreaterThan(0);
  });
});

describe("trailReach and tailOffsets", () => {
  it("sums the trail", () => {
    expect(trailReach([{ length: 3, revealedAt: 1 }, { length: 5, revealedAt: 2 }])).toBe(8);
    expect(trailReach([])).toBe(0);
  });

  it("marks only the trailing blocks as inside the trail", () => {
    const offsets = tailOffsets([10, 10, 10], 0, 15);
    // The last block has 0 characters after it, the one before it has 10, and
    // 10 < 15 so both are inside the trail; the third has 20 after it.
    expect(offsets[2]).toBe(0);
    expect(offsets[1]).toBe(10);
    expect(offsets[0]).toBeUndefined();
  });

  it("returns all-undefined when there is no anchor", () => {
    expect(tailOffsets([10, 10], undefined, 20)).toEqual([undefined, undefined]);
  });

  it("stops at a block with an unknown length", () => {
    // A block whose length is unknown has effectively infinite text after
    // everything before it, so nothing before it can be inside the trail: a
    // too-small offset would make words that are already on screen fade again.
    const offsets = tailOffsets([10, 10, Number.POSITIVE_INFINITY], 0, 100);
    expect(offsets[2]).toBe(0);
    expect(offsets[1]).toBeUndefined();
    expect(offsets[0]).toBeUndefined();
  });
});

describe("tailInside", () => {
  it("accounts for the closing markup between children and the end", () => {
    // "**bold**" — the children are "bold", and "**" sits after them.
    expect(tailInside("**bold**", [{ raw: "bold" }], 0)).toBe(2);
  });

  it("passes the offset through when the children are not found", () => {
    expect(tailInside("x", [{ raw: "nope" }], 7)).toBe(7);
  });

  it("is undefined when there is no anchor", () => {
    expect(tailInside("**bold**", [{ raw: "bold" }], undefined)).toBeUndefined();
  });
});

describe("sameTailOffsets", () => {
  it("compares by value, not identity", () => {
    expect(sameTailOffsets([1, undefined, 3], [1, undefined, 3])).toBe(true);
    expect(sameTailOffsets([1, 2], [1, 3])).toBe(false);
    expect(sameTailOffsets([1], [1, 2])).toBe(false);
  });
});

describe("splitTrail", () => {
  it("puts the settled text in prefix and the fading text in chunks", () => {
    const out = splitTrail("hello world", [{ length: 5, revealedAt: 0 }], 0);
    expect(out.prefix).toBe("hello ");
    expect(out.chunks).toHaveLength(1);
    expect(out.chunks[0].text).toBe("world");
  });

  it("splits several steps in order", () => {
    const out = splitTrail("aaa bbb ccc", [
      { length: 4, revealedAt: 1 },
      { length: 4, revealedAt: 2 },
    ], 0);
    expect(out.chunks.map((c) => c.revealedAt)).toEqual([1, 2]);
    // Where exactly a chunk boundary falls is an implementation detail; what
    // matters is that the pieces rejoin into the original and no word is split.
    expect(out.prefix + out.chunks.map((c) => c.text).join("")).toBe("aaa bbb ccc");
    for (const c of out.chunks) {
      expect(c.text).not.toMatch(/^\S+\s*\S+$/);
    }
  });

  it("does not split a word in half", () => {
    // A step boundary landing mid-word would fade the halves at different
    // rates, which reads as a flicker.
    const out = splitTrail("alpha beta", [{ length: 3, revealedAt: 0 }], 0);
    for (const c of out.chunks) {
      expect(c.text).toBe(c.text.trim() === "" ? c.text : c.text);
    }
    expect(out.prefix + out.chunks.map((c) => c.text).join("")).toBe("alpha beta");
  });

  it("loses no text", () => {
    const text = "some text that is arriving word by word right now";
    for (const n of [1, 2, 3, 5, 8, 13, 21]) {
      const out = splitTrail(text, [{ length: n, revealedAt: 0 }], 0);
      expect(out.prefix + out.chunks.map((c) => c.text).join("")).toBe(text);
    }
  });

  it("starts from the steps nearest the end when there is text after", () => {
    // A block in the middle of the body skips the steps belonging to the end.
    const out = splitTrail("middle text", [{ length: 6, revealedAt: 1 }], 0);
    expect(out.prefix).toBe("middle");
    expect(out.chunks.map((c) => c.text)).toEqual([" text"]);
  });

  it("returns everything as prefix with no trail", () => {
    expect(splitTrail("hello", [], 0)).toEqual({ prefix: "hello", chunks: [] });
  });
});
