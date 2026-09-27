import { describe, it, expect } from "vitest";
import {
  avatarProfile,
  computeMoods,
  faceFor,
  hueVars,
  initials,
  moodPresentation,
  SILHOUETTES,
  stableIndex,
  type AvatarMood,
} from "./avatar";

const AGENTS = ["a", "b"];

function moodsFor(over: Partial<Parameters<typeof computeMoods>[0]> = {}) {
  return computeMoods({
    agentIds: AGENTS,
    running: new Set(),
    failed: new Set(),
    needsYou: new Set(),
    replied: new Set(),
    ...over,
  });
}

describe("stableIndex", () => {
  it("is deterministic", () => {
    expect(stableIndex("Coder", 8)).toBe(stableIndex("Coder", 8));
  });

  it("stays in range", () => {
    for (let i = 0; i < 400; i += 1) {
      const v = stableIndex(`agent-${i}`, 6);
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThan(6);
    }
  });

  it("spreads near-neighbouring names across buckets", () => {
    // The backend hash was a byte sum, so `Coder` and `Coder2` and `Coder3`
    // all landed in the same bucket and three agents in an office shared a
    // face. One character of difference should usually change the result.
    const a = stableIndex("Coder", 6);
    const b = stableIndex("Coder2", 6);
    expect(a).not.toBe(b);
  });

  it("does not collide on anagrams", () => {
    // A byte sum cannot tell `abc` from `cba` at all.
    expect(stableIndex("abc", 97)).not.toBe(stableIndex("cba", 97));
  });
});

describe("avatarProfile", () => {
  it("gives the same agent the same face every time", () => {
    expect(avatarProfile("QA")).toEqual(avatarProfile("QA"));
  });

  it("only ever picks a known silhouette", () => {
    for (let i = 0; i < 200; i += 1) {
      expect(SILHOUETTES).toContain(avatarProfile(`agent-${i}`).silhouette);
    }
  });

  it("gives two named agents different faces", () => {
    // The office templates have 7 roles with distinct names; if two of them
    // matched on any trait the roster would be unreadable.
    const names = ["CEO", "Planner", "Architect", "Coder", "Tester", "QA", "DevOps"];
    for (let i = 0; i < names.length; i += 1) {
      for (let j = i + 1; j < names.length; j += 1) {
        const a = avatarProfile(names[i]);
        const b = avatarProfile(names[j]);
        expect(
          a.silhouette !== b.silhouette || a.hue !== b.hue,
          `${names[i]} and ${names[j]} look identical`,
        );
      }
    }
  });

  it("handles an empty or odd name", () => {
    expect(() => avatarProfile("")).not.toThrow();
    expect(() => avatarProfile("日本語")).not.toThrow();
    expect(() => avatarProfile("a".repeat(500))).not.toThrow();
  });
});

describe("mood precedence", () => {
  // This is the ordering that matters. A failure shown as "working" hides the
  // one state a user must not miss behind the turn that produced it.
  it("puts a failure above everything else", () => {
    expect(
      moodsFor({
        failed: new Set(["a"]),
        needsYou: new Set(["a"]),
        running: new Set(["a"]),
        replied: new Set(["a"]),
      }).a,
    ).toBe("failed");
  });

  it("puts a pending question above running work", () => {
    expect(moodsFor({ needsYou: new Set(["a"]), running: new Set(["a"]) }).a).toBe("waiting");
  });

  it("puts running work above an unread reply", () => {
    expect(moodsFor({ running: new Set(["a"]), replied: new Set(["a"]) }).a).toBe("working");
  });

  it("shows a reply nobody has read", () => {
    expect(moodsFor({ replied: new Set(["a"]) }).a).toBe("responded");
  });

  it("leaves a quiet agent out, so a missing key means idle", () => {
    expect(moodsFor()).toEqual({});
  });

  it("keeps one agent's state out of another's", () => {
    const out = computeMoods({
      agentIds: ["a", "b", "c", "d"],
      running: new Set(["a"]),
      failed: new Set(["b"]),
      needsYou: new Set(["c"]),
      replied: new Set(),
    });
    expect(out).toEqual({ a: "working", b: "failed", c: "waiting" });
  });

  it("puts a sleeping agent to sleep regardless of the rest", () => {
    expect(
      moodsFor({ sleeping: new Set(["a"]), failed: new Set(["a"]) }).a,
    ).toBe("sleeping");
  });
});

describe("faceFor", () => {
  it("wears the agent's resting face when idle", () => {
    // The resting face is chosen from the name, so a quiet agent is still
    // recognisable rather than a generic neutral blob.
    const p = avatarProfile("Coder");
    expect(faceFor(p, "idle")).toBe(p.resting);
  });

  it("changes with the work", () => {
    const p = avatarProfile("Coder");
    const faces = (["working", "waiting", "failed", "responded"] as AvatarMood[]).map((m) =>
      faceFor(p, m),
    );
    // working → attentive, waiting → curious, failed → sad, responded → pleased
    expect(faces).toEqual(["attentive", "curious", "sad", "pleased"]);
  });

  it("gives every mood a presentation", () => {
    for (const m of ["idle", "working", "waiting", "failed", "responded", "sleeping"] as AvatarMood[]) {
      const p = moodPresentation(m);
      expect(p).toBeDefined();
      expect(typeof p.expression).toBe("string");
    }
  });

  it("only marks a busy mood busy", () => {
    expect(moodPresentation("working").busy).toBe(true);
    expect(moodPresentation("sleeping").busy).toBe(true);
    expect(moodPresentation("idle").busy).toBe(false);
    expect(moodPresentation("failed").busy).toBe(false);
  });

  it("breathes only where the motion means something", () => {
    expect(moodPresentation("working").breathe).toBeGreaterThan(0);
    expect(moodPresentation("idle").breathe).toBe(0);
    // A failure must hold still. Motion on a sad face reads as panic.
    expect(moodPresentation("failed").breathe).toBe(0);
    expect(moodPresentation("failed").rings).toBe(false);
  });

  it("falls back to idle for a mood it does not know", () => {
    const unknown = moodPresentation("nonsense" as AvatarMood);
    expect(unknown).toEqual(moodPresentation("idle"));
  });
});

describe("hueVars", () => {
  it("produces a usable HSL triple", () => {
    const v = hueVars(210);
    expect(v.accent).toBe("210 72% 62%");
    expect(v.dim).toBe("210 40% 44%");
    expect(v.deep).toBe("210 55% 28%");
  });
});

describe("initials", () => {
  it("takes the first two letters of one name", () => {
    expect(initials("QA")).toBe("QA");
  });

  it("takes the outer letters of two", () => {
    expect(initials("Raven Prime")).toBe("RP");
    expect(initials("Grand Archivist of the Codex")).toBe("GC");
  });

  it("never returns nothing", () => {
    expect(initials("")).toBe("?");
    expect(initials("   ")).toBe("?");
  });
});
