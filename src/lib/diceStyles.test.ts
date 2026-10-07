import { describe, it, expect } from "vitest";
import {
  DICE_STYLES,
  DICE_TEMPO,
  DEFAULT_AVATAR_STYLE,
  diceStyle,
  styleAnimates,
  parseDiceBearUrl,
  currentDiceBearUrl,
  type DiceSpeed,
} from "./diceStyles";
import { getDiceBearUrl, isNativeAvatarStyle } from "./utils";
import { DICEBEAR_VERSION } from "./diceVersions";

/**
 * The style catalogue and the URL builder.
 *
 * The numbers in the catalogue are measured from the API rather than assumed,
 * and that is the thing worth testing: a style that claims to animate and does
 * not is invisible — it renders a perfectly good static avatar, so nothing looks
 * broken and the picker is simply lying. The cadence claim is the same: it is the
 * only reason to choose one animated style over another, and it is the number
 * shown in the tooltip.
 */

const byValue = new Map(DICE_STYLES.map((s) => [s.value, s]));

describe("the style catalogue", () => {
  it("has no duplicate slugs", () => {
    const seen = new Set<string>();
    const dupes = DICE_STYLES.filter((s) => (seen.has(s.value) ? true : (seen.add(s.value), false)));
    expect(dupes.map((d) => d.value)).toEqual([]);
  });

  /**
   * The two claims have to agree with each other, or a tile promises motion and
   * the face sits still.
   */
  it("marks a style animated if and only if it has a speed", () => {
    for (const s of DICE_STYLES) {
      expect(Boolean(s.speed), `${s.value}: speed without animated`).toBe(s.animated);
      expect(s.loopSeconds !== null, `${s.value}: loop without animated`).toBe(s.animated);
      if (s.animated) {
        expect(s.speed).not.toBe("none");
        expect(s.loopSeconds!).toBeGreaterThan(0);
      }
    }
  });

  it("only offers speeds the API accepts", () => {
    const valid: DiceSpeed[] = ["none", "slowest", "slow", "medium", "fast", "fastest"];
    for (const s of DICE_STYLES) {
      if (s.speed) expect(valid, `${s.value}`).toContain(s.speed);
    }
  });

  it("has a real tempo for every speed", () => {
    // `fastest` has to be shorter than `slowest`, or "faster" is a lie.
    expect(DICE_TEMPO.fastest).toBeLessThan(DICE_TEMPO.slowest);
    expect(DICE_TEMPO.fast).toBeLessThan(DICE_TEMPO.slow);
    expect(DICE_TEMPO.slow).toBeLessThan(DICE_TEMPO.slowest);
  });

  /**
   * The point of the exercise: a face should visibly change while you look at
   * it.
   *
   * Two separate claims, because they fail in opposite directions. The
   * *default* gets a hard promise — two to three seconds — because that is the
   * cadence a user who never opens the picker sees, and it is the only number
   * the rest of the app is tuned against. The *catalogue* gets an honest
   * ceiling instead: `shapes` really does take 12.6 s, and the previous version
   * of this test filtered `loopSeconds <= 12` before asserting `<= 12`, which
   * asserted nothing at all while looking like it did.
   */
  it("changes the default face every two to three seconds", () => {
    const fallback = diceStyle(DEFAULT_AVATAR_STYLE)!;
    expect(fallback.animated).toBe(true);
    expect(fallback.loopSeconds!).toBeGreaterThanOrEqual(2);
    expect(fallback.loopSeconds!).toBeLessThanOrEqual(3);
  });

  it("animates the living styles at a cadence a reader can see", () => {
    const living = DICE_STYLES.filter((s) => s.animated);
    expect(living.length).toBeGreaterThan(10);
    for (const s of living) {
      expect(s.loopSeconds!, `${s.value} is too slow to notice`).toBeLessThanOrEqual(13);
    }
    // A catalogue whose only fast style is the default is one style, not a
    // system, so most of them have to sit well inside the ceiling.
    expect(living.filter((s) => (s.loopSeconds ?? 99) <= 6).length).toBeGreaterThan(10);
  });

  /**
   * The five faces this app was asked for by name all sit in the band, and the
   * one that is still is still on purpose — `notionists-neutral` has no
   * animation variant upstream, so the wrapper breathes instead.
   */
  it("keeps the hand-picked faces in the readable band", () => {
    for (const value of ["clay", "voxel-bot", "initial-face", "pixelbot"]) {
      const s = diceStyle(value)!;
      expect(s.animated, `${value} should animate`).toBe(true);
      expect(s.loopSeconds!, `${value} cadence`).toBeLessThanOrEqual(4.5);
    }
    expect(diceStyle("notionists-neutral")!.animated).toBe(false);
  });

  it("puts the animated styles where a reader will find them", () => {
    const sorted = [...DICE_STYLES].sort((a, b) => (a.animated === b.animated ? 0 : a.animated ? -1 : 1));
    expect(sorted[0].animated).toBe(true);
    // And the first animated one is the default, so the two agree.
    expect(sorted[0].value).toBe(DEFAULT_AVATAR_STYLE);
  });

  it("looks a style up, and reports an unknown one as still", () => {
    expect(diceStyle("clay")?.animated).toBe(true);
    expect(styleAnimates("clay")).toBe(true);
    expect(diceStyle("does-not-exist")).toBeUndefined();
    // Asking for motion on a style we do not know must not put a speed in the
    // URL: an unrecognised `animationVariant` is silently static, so it would
    // cost a parameter and gain nothing.
    expect(styleAnimates("does-not-exist")).toBe(false);
  });

  it("includes the styles that were asked for by name", () => {
    for (const value of ["clay", "initial-face", "voxel-bot", "pixelbot", "notionists-neutral"]) {
      expect(byValue.has(value), `${value} is missing`).toBe(true);
    }
  });
});

describe("getDiceBearUrl", () => {
  it("requests the current API version", () => {
    expect(getDiceBearUrl("Ada", "bottts")).toContain(`dicebear.com/${DICEBEAR_VERSION}/bottts/svg`);
  });

  it("falls back to a seed rather than an empty one", () => {
    expect(getDiceBearUrl("   ", "bottts")).toContain("seed=Agent");
  });

  it("asks for the animation when the style animates", () => {
    expect(getDiceBearUrl("Ada", "clay")).toContain("animationVariant=fastest");
  });

  it("sends no speed for a still style", () => {
    expect(getDiceBearUrl("Ada", "notionists-neutral")).not.toContain("animationVariant");
  });

  it("sends no speed for a style it does not know", () => {
    expect(getDiceBearUrl("Ada", "not-a-style")).not.toContain("animationVariant");
  });

  it("can be told to hold still", () => {
    expect(getDiceBearUrl("Ada", "clay", "", false)).not.toContain("animationVariant");
  });

  it("keeps the extra parameters it was given", () => {
    const url = getDiceBearUrl("Ada", "bottts", "backgroundColor=ff0000&scale=200");
    expect(url).toContain("backgroundColor=ff0000");
    expect(url).toContain("scale=200");
  });

  it("returns nothing for a style drawn by this app", () => {
    // "" is the honest answer: there is no remote face to fetch, and storing a
    // stale URL for a locally drawn style is how a local style ends up remote.
    expect(getDiceBearUrl("Ada", "raven-native")).toBe("");
    expect(isNativeAvatarStyle("raven-native")).toBe(true);
    expect(isNativeAvatarStyle("clay")).toBe(false);
  });

  it("is deterministic, so an agent keeps its face between renders", () => {
    expect(getDiceBearUrl("Ada", "clay")).toBe(getDiceBearUrl("Ada", "clay"));
  });
});

describe("stored URLs", () => {
  it("reads a version, style and seed", () => {
    const found = parseDiceBearUrl("https://api.dicebear.com/9.x/voxel-bot/svg?seed=Ada&radius=50");
    expect(found).toMatchObject({ style: "voxel-bot", seed: "Ada" });
  });

  it("supplies a seed when the stored URL has none", () => {
    expect(parseDiceBearUrl("https://api.dicebear.com/9.x/bottts/svg")!.seed).toBe("Agent");
  });

  it("rejects a URL that is not a DiceBear SVG", () => {
    // Each of these is a way the old check could be fooled, or a real thing a
    // user can set that must be left alone.
    for (const url of [
      "https://evil.example/api.dicebear.com/9.x/bottts/svg?seed=x",
      "https://api.dicebear.com/9.x/bottts/png?seed=x",
      "https://api.dicebear.com/9.x/bottts/svg/extra?seed=x",
      "https://api.dicebear.com/9x/bottts/svg?seed=x",
      "https://api.dicebear.com/attacker/9.x/bottts/svg",
      "http://api.dicebear.com/9.x/bottts/svg",
      "ftp://api.dicebear.com/9.x/bottts/svg",
    ]) {
      expect(parseDiceBearUrl(url), `wrongly claimed ${url}`).toBeNull();
    }
  });

  it("upgrades a frozen URL and keeps the agent's face", () => {
    const after = currentDiceBearUrl(
      "https://api.dicebear.com/9.x/voxel-bot/svg?seed=Researcher&backgroundColor=6366f1",
    )!;
    expect(after.url).toContain(`dicebear.com/${DICEBEAR_VERSION}/voxel-bot/svg`);
    expect(after.url).toContain("seed=Researcher");
    expect(after.url).toContain("backgroundColor=6366f1");
    expect(after.animates).toBe(true);
  });

  it("is idempotent, so a re-render does not keep changing the URL", () => {
    // A URL that changed on every render would refetch the avatar on every mood
    // change and flash.
    const once = currentDiceBearUrl("https://api.dicebear.com/9.x/clay/svg?seed=Ada")!;
    const twice = currentDiceBearUrl(once.url)!;
    expect(twice.url).toBe(once.url);
    expect(twice.animates).toBe(once.animates);
  });

  it("drops a stale animationVariant rather than carrying it forward", () => {
    // A URL saved with a speed the new catalogue no longer uses would otherwise
    // keep requesting it — and an unrecognised value is silently static.
    const after = currentDiceBearUrl(
      "https://api.dicebear.com/9.x/clay/svg?seed=Ada&animationVariant=medium",
    )!;
    expect(after.url).toContain("animationVariant=fastest");
  });
});
