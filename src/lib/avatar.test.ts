import { describe, it, expect } from "vitest";
import {
  ANIMATIONS,
  avatarProfile,
  colourVars,
  COLOURS,
  computeMoods,
  EXPRESSIONS,
  faceFor,
  hueVars,
  initials,
  moodPresentation,
  motionFor,
  normaliseColour,
  normaliseExpression,
  normaliseShape,
  normaliseState,
  rosterSafeMotion,
  shade,
  SHAPES,
  UNREACHABLE_MOTIONS,
  SILHOUETTES,
  stableIndex,
  type AvatarMood,
} from "./avatar";

/**
 * Every roster mood, in the order the studio shows them.
 *
 * Written once and reused, because the alternative is the six-mood list pasted
 * into a dozen tests — which is exactly how a suite keeps passing after the thing
 * it describes has gained a state.
 */
const ALL_MOODS = [
  "idle",
  "working",
  "thinking",
  "waiting",
  "failed",
  "responded",
  "sleeping",
] as AvatarMood[];

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

  /**
   * The face is identity, and identity does not change with the work.
   *
   * This test used to assert the opposite — that the face went
   * attentive/curious/sad/pleased as the mood changed — because that is what the
   * code did. It is the wrong behaviour and it was worth reversing the test
   * rather than the code back: an agent that wears a different face while it
   * works is a different creature, and a roster where every avatar changes
   * character twice a minute is not a roster you can learn to read.
   *
   * So the face is fixed and the *motion* carries the mood, which is what the
   * next test covers.
   */
  it("keeps the agent's own face across every mood but one", () => {
    const p = avatarProfile("Coder");
    const faces = ALL_MOODS.filter((m) => m !== "waiting").map((m) => faceFor(p, m));
    expect(new Set(faces).size, "the face changed with the mood").toBe(1);
    expect(faces[0]).toBe(p.resting);
  });

  /**
   * The one override, and why it is the only one.
   *
   * An agent waiting on you has to read as paying attention. Without it, "waiting
   * for you" and "idle, has not got round to it" look identical, which is the
   * exact confusion the state exists to end. Everything else can be carried by
   * motion alone.
   */
  it("lets only waiting change the face", () => {
    const overriding = ALL_MOODS.filter((m) => moodPresentation(m).expression !== undefined);
    expect(overriding).toEqual(["waiting"]);
    expect(faceFor(avatarProfile("Coder"), "waiting")).toBe("attentive");
  });

  it("gives every mood a motion state", () => {
    for (const m of ALL_MOODS) {
      const p = moodPresentation(m);
      expect(p, `${m} has no presentation`).toBeDefined();
      expect(typeof p.state, `${m} has no state`).toBe("string");
      expect(p.state).toBe(motionFor(m));
    }
  });

  /**
   * The mapping is the design, so it is pinned.
   *
   * Grok encodes the same thing as a test, and for the same reason: a library
   * bump that adds or reclassifies a state silently changes what a roster means,
   * and nothing else would notice. Written out in full because a table that
   * only exists as a `Record` can be edited without anyone seeing what changed.
   */
  it("maps each roster mood to its documented motion", () => {
    expect(ALL_MOODS.map(motionFor)).toEqual([
      "idle",
      "thinking",
      "orbit",
      "notification",
      "exclamation",
      "burst",
      "sleep",
    ]);
  });

  /**
   * Every mood must look different from idle, or "needs you" and "nothing is
   * happening" are the same picture.
   */
  it("never leaves a busy mood looking idle", () => {
    const idle = motionFor("idle");
    for (const m of ALL_MOODS.filter((m) => m !== "idle")) {
      expect(motionFor(m), `${m} looks idle`).not.toBe(idle);
    }
  });

  /**
   * Every motion a mood can reach must be one the renderer can actually draw,
   * and must not be one that replaces an agent's outline.
   *
   * This replaced a hand-written list of the eight motions the old renderer
   * supported, which was the right kind of test for the wrong kind of thing: it
   * asserted against a copy of the implementation rather than against the
   * specification. Now it asserts the real invariants — every state is one of
   * Grok's fifteen, and none of the three that change the body can be reached
   * from a mood.
   */
  it("never reaches a motion the roster must not play", () => {
    const all = new Set<string>(ANIMATIONS.map((a) => a.id));
    expect(all.size, "the fifteen should be fifteen").toBe(15);
    for (const m of ALL_MOODS) {
      const s = motionFor(m);
      expect(all.has(s), `${m} → ${s} is not one of the fifteen`).toBe(true);
      expect(UNREACHABLE_MOTIONS as readonly string[]).not.toContain(s);
    }
  });

  /**
   * `busy` means "this state needs an animation clock", and after adding the
   * pulsing badge and the shaking mark, everything except a resting agent does.
   *
   * The old version asserted `failed` was *not* busy, which was true while the
   * exclamation was a static ring. It is a badge that shakes now, so a failure
   * that does not move is a failure nobody notices, and that is the one thing
   * this state exists to prevent.
   */
  it("runs a clock for everything except a resting agent", () => {
    for (const m of ALL_MOODS) {
      expect(moodPresentation(m).busy, `${m}`).toBe(m !== "idle");
    }
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

/* ── Grok Bot's avatar specification ───────────────────────────────────────
 * The eight shapes, sixteen expressions, twelve colours and fifteen animation
 * states are the contract with the rest of the product — the studio renders
 * them, the roster plays a subset, and a label in six locales names them. These
 * tests pin the size and the shape of that contract, because every one of the
 * four axes is duplicated in a translation file and a list that quietly shrinks
 * is invisible until a chip row comes back with a gap in it.
 */

describe("the Grok avatar specification", () => {
  it("has Grok's eight shapes", () => {
    expect([...SHAPES]).toEqual([
      "circle",
      "pebble",
      "squircle",
      "capsule",
      "triangle",
      "hexagon",
      "cloud",
      "droplet",
    ]);
  });

  it("has Grok's sixteen expressions", () => {
    expect(EXPRESSIONS.length).toBe(16);
    expect(new Set(EXPRESSIONS).size).toBe(16);
  });

  it("has Grok's twelve colours, with a hex for each", () => {
    expect(COLOURS.length).toBe(12);
    for (const c of COLOURS) {
      expect(c.hex, `${c.id} has no hex`).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("has Grok's fifteen animation states", () => {
    expect(ANIMATIONS.length).toBe(15);
    expect(ANIMATIONS.map((a) => a.id)).toEqual([
      "idle",
      "thinking",
      "wink",
      "wideEyes",
      "alert",
      "notification",
      "exclamation",
      "sleep",
      "egg",
      "hexagon",
      "play",
      "orbit",
      "swirl",
      "burst",
      "comet",
    ]);
  });

  /** `SILHOUETTES` is the old name and still imported all over the codebase. */
  it("keeps SILHOUETTES as an alias for SHAPES", () => {
    expect(SILHOUETTES).toBe(SHAPES);
  });
});

describe("normalising stored ids", () => {
  /**
   * The regression this exists for.
   *
   * Shape, expression and colour are all stored on the agent, so renaming a
   * member of any of these lists without a forward map hands every existing
   * agent the fallback face the first time the app loads — silently, and to
   * every agent at once.
   */
  it("maps every historical shape onto a current one", () => {
    expect(normaliseShape("round")).toBe("circle");
    expect(normaliseShape("hex")).toBe("hexagon");
    expect(normaliseShape("shield")).toBe("pebble");
    expect(normaliseShape("crystal")).toBe("squircle");
    expect(normaliseShape("droplet")).toBe("droplet");
    expect(normaliseShape("capsule")).toBe("capsule");
  });

  it("maps the historical expression", () => {
    expect(normaliseExpression("pleased")).toBe("happy");
  });

  it("passes a current id straight through", () => {
    for (const s of SHAPES) expect(normaliseShape(s)).toBe(s);
    for (const e of EXPRESSIONS) expect(normaliseExpression(e)).toBe(e);
    for (const c of COLOURS) expect(normaliseColour(c.id)).toBe(c.id);
  });

  it("never throws on absent or nonsense ids", () => {
    for (const bad of [null, undefined, "", "   ", "wat", 42 as unknown as string]) {
      expect(SHAPES).toContain(normaliseShape(bad));
      expect(EXPRESSIONS).toContain(normaliseExpression(bad));
      expect(COLOURS.map((c) => c.id)).toContain(normaliseColour(bad));
      expect(ANIMATIONS.map((a) => a.id)).toContain(normaliseState(bad));
    }
  });

  /**
   * Two legacy shapes must not collapse onto one current shape, or renaming a
   * list quietly merges two agents into the same face.
   */
  it("keeps the legacy shape map injective", () => {
    const legacy = ["round", "hex", "shield", "crystal", "droplet", "capsule"];
    const mapped = legacy.map(normaliseShape);
    expect(new Set(mapped).size, "two legacy shapes now share a face").toBe(legacy.length);
  });
});

describe("normaliseState", () => {
  it("accepts the camelCase ids", () => {
    expect(normaliseState("wideEyes")).toBe("wideEyes");
  });

  it("accepts the spaced forms the documentation uses", () => {
    expect(normaliseState("wide eyes")).toBe("wideEyes");
    expect(normaliseState("Wide Eyes")).toBe("wideEyes");
  });

  it("accepts kebab and snake forms", () => {
    expect(normaliseState("wide-eyes")).toBe("wideEyes");
    expect(normaliseState("wide_eyes")).toBe("wideEyes");
  });
});

describe("rosterSafeMotion", () => {
  /**
   * The one rule that separates the studio from a roster.
   *
   * `egg`, `hexagon` and `play` replace the outline, and the outline is how you
   * recognise an agent. The studio previews artwork on purpose and is allowed
   * them; a live roster is not, and has to be protected by something other than
   * every call site remembering.
   */
  it("refuses the three states that replace an agent's outline", () => {
    for (const s of UNREACHABLE_MOTIONS) {
      expect(rosterSafeMotion(s), `${s} should not reach a roster`).not.toBe(s);
    }
  });

  it("refuses them to one of the roster's own states", () => {
    const roster = new Set(ALL_MOODS.map(motionFor));
    for (const s of UNREACHABLE_MOTIONS) {
      expect(roster.has(rosterSafeMotion(s)), `${s} → ${rosterSafeMotion(s)}`).toBe(true);
    }
  });

  it("leaves every other state alone", () => {
    const safe = ANIMATIONS.filter((a) => !a.changesBody).map((a) => a.id);
    for (const s of safe) expect(rosterSafeMotion(s)).toBe(s);
  });

  it("flags exactly the three body-changing states in ANIMATIONS", () => {
    expect(ANIMATIONS.filter((a) => a.changesBody).map((a) => a.id)).toEqual([
      "egg",
      "hexagon",
      "play",
    ]);
  });
});

describe("colour", () => {
  /**
   * Ink and Cream sit in the same palette as Bright Blue, and a face drawn in a
   * fixed ink tone is invisible on one of them and glaring on the other. So the
   * face colour is chosen from the body's own luminance rather than hard-coded,
   * and these are the two ends that would catch a regression in that.
   */
  it("draws the face dark on a light body and light on a dark one", () => {
    expect(colourVars("cream").face).toBe("#141414");
    expect(colourVars("amber").face).toBe("#141414");
    expect(colourVars("ink").face).toBe("#f4f4f5");
    expect(colourVars("brown").face).toBe("#141414");
  });

  it("keeps the specified hex exactly", () => {
    for (const c of COLOURS) {
      expect(colourVars(c.id).base, `${c.id}`).toBe(c.hex);
    }
  });

  it("gives every colour a deeper and a lighter step", () => {
    for (const c of COLOURS) {
      const v = colourVars(c.id);
      expect(v.deep).not.toBe(v.base);
      expect(v.glow).not.toBe(v.base);
    }
  });

  it("shades towards white above and black below", () => {
    expect(shade("#000000", 1)).toBe("#ffffff");
    expect(shade("#ffffff", 1)).toBe("#ffffff");
    expect(shade("#ffffff", -1)).toBe("#000000");
    expect(shade("#808080", 0)).toBe("#808080");
  });

  it("returns the same object for the same colour", () => {
    expect(colourVars("blue")).toBe(colourVars("blue"));
  });

  /** `hue` still exists because a good deal of the app thinks in hue. */
  it("derives hue from the chosen hex rather than choosing separately", () => {
    for (const name of ["a", "b", "Ada", "Researcher", "very-long-agent-name"]) {
      const p = avatarProfile(name);
      expect(p.hex).toBe(colourVars(p.colour).base);
      expect(p.hue).toBeGreaterThanOrEqual(0);
      expect(p.hue).toBeLessThan(360);
    }
  });
});

describe("computeMoods", () => {
  it("puts a long run above a short one", () => {
    const m = moodsFor({ running: new Set(["a", "b"]), thinking: new Set(["b"]) });
    expect(m.a).toBe("working");
    expect(m.b).toBe("thinking");
  });

  it("still lets a failure outrank a long run", () => {
    const m = moodsFor({
      running: new Set(["a"]),
      thinking: new Set(["a"]),
      failed: new Set(["a"]),
    });
    expect(m.a).toBe("failed");
  });

  it("still lets a question outrank a long run", () => {
    const m = moodsFor({
      running: new Set(["a"]),
      thinking: newSetOf(["a"]),
      needsYou: new Set(["a"]),
    });
    expect(m.a).toBe("waiting");
  });
});

function newSetOf<T>(items: T[]): Set<T> {
  return new Set(items);
}
