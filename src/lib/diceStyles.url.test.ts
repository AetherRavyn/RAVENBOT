import { describe, it, expect } from "vitest";
import { getDiceBearUrl } from "$lib/utils";
import { DICE_STYLES, AVATAR_BACKGROUNDS } from "$lib/diceStyles";

/**
 * The shape the API accepts, transcribed from its own validator.
 *
 * `backgroundColor` must match a single hex colour — 3, 4, 6 or 8 digits — and
 * the validator reads the *raw* query string, so a comma-separated list only
 * works if its commas are left unencoded. `URLSearchParams` encodes them, and
 * the request then 400s, which renders as a blank frame rather than an error.
 */
const SINGLE_COLOUR = /^#?([a-fA-F0-9]{3}|[a-fA-F0-9]{4}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/;

describe("every URL the app can build", () => {
  it("sends a background colour the API will accept", () => {
    for (const s of DICE_STYLES) {
      if (s.value === "raven-native") continue;
      const url = getDiceBearUrl("Researcher", s.value);
      const bg = new URL(url).searchParams.get("backgroundColor") ?? "";
      expect(bg, `${s.value} sent no background`).not.toBe("");
      expect(SINGLE_COLOUR.test(bg), `${s.value} sent ${JSON.stringify(bg)}`).toBe(true);
    }
  });

  it("never sends an encoded comma list, which 10.x rejects", () => {
    const url = getDiceBearUrl("Researcher", "clay");
    expect(url).not.toContain("%2C");
    expect(url).not.toContain("backgroundColor=,");
  });

  it("asks for the animation exactly where the catalogue says it can", () => {
    for (const s of DICE_STYLES) {
      if (s.value === "raven-native") continue;
      const url = getDiceBearUrl("Researcher", s.value);
      const asked = url.includes("animationVariant=");
      expect(asked, `${s.value}: animated=${s.animated} but asked=${asked}`).toBe(s.animated);
      if (s.animated) {
        expect(url).toContain(`animationVariant=${s.speed}`);
      }
    }
  });

  it("picks a background from the palette", () => {
    for (const s of DICE_STYLES) {
      if (s.value === "raven-native") continue;
      const bg = new URL(getDiceBearUrl("Researcher", s.value)).searchParams.get("backgroundColor")!;
      expect(AVATAR_BACKGROUNDS, `${s.value} picked ${bg}`).toContain(bg);
    }
  });

  /** A face that changed on every render would refetch on every render. */
  it("gives an agent a stable background", () => {
    for (const seed of ["Ada", "Researcher", "a", "Z"]) {
      expect(avatarBg(seed)).toBe(avatarBg(seed));
    }
    expect(AVATAR_BACKGROUNDS.length).toBeGreaterThan(1);
  });
});

function avatarBg(seed: string) {
  return new URL(getDiceBearUrl(seed, "clay")).searchParams.get("backgroundColor");
}
