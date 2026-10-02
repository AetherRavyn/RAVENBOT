// @vitest-environment jsdom
/**
 * The avatar studio.
 *
 * The panel exists to answer one question before the agent exists: "will I be
 * able to tell that one is waiting on me?" That is only answerable if every state
 * is on screen at once and moving, so the strip is the product and the pickers are
 * the accessory — which is what these tests check first.
 *
 * It also renders through the *real* `RavenAvatar` rather than a fork, so what is
 * previewed is what the fleet will show. A second implementation would drift, and
 * a studio that previews something other than the product is worse than none.
 */
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent } from "@testing-library/svelte";
import AvatarStudio from "./AvatarStudio.svelte";
import { ANIMATIONS, COLOURS, EXPRESSIONS, SHAPES, motionFor, type AvatarMood } from "$lib/avatar";
import { setLocale } from "$lib/i18n";

afterEach(cleanup);

/**
 * The six roster moods plus one.
 *
 * `thinking` used to be folded into `working` and had no strip entry, which was
 * fine while the only difference between them was a slightly different face. It
 * now has its own motion — Orbit, rings that keep turning for a long run — and
 * that earns a row, because "leave this one alone, it is working on something
 * big" is a different instruction from "this one is about to answer".
 */
const STATES: AvatarMood[] = [
  "idle",
  "working",
  "thinking",
  "waiting",
  "failed",
  "responded",
  "sleeping",
];

describe("AvatarStudio", () => {
  it("shows every roster state at once", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, { props: { seed: "Researcher" } });
    const frames = [...container.querySelectorAll(".raven-studio__state-frame")];
    expect(frames, "the state strip is the point of the panel").toHaveLength(STATES.length);
  });

  /**
   * Every state in the strip must be the state it claims to be, or the studio is
   * a very pretty way to be lied to.
   */
  it("renders each strip entry in its own state", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, { props: { seed: "Researcher" } });
    const shown = [...container.querySelectorAll(".raven-studio__state")].map((cell) => {
      const el = cell.querySelector(".raven-avatar")!;
      return `${el.getAttribute("data-mood")}:${el.getAttribute("data-state")}`;
    });
    expect(shown).toEqual(STATES.map((m) => `${m}:${motionFor(m)}`));
  });

  it("offers every shape, expression and animation state", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, { props: { seed: "Researcher" } });
    // One chip per axis, so the chip count is the size of the largest axis.
    const chips = [...container.querySelectorAll(".raven-studio__chip")];
    expect(chips.length).toBeGreaterThanOrEqual(SHAPES.length);
    expect(chips.length).toBeGreaterThanOrEqual(EXPRESSIONS.length);
    for (const s of SHAPES) {
      expect(container.textContent, `shape ${s} is unlabelled`).toBeTruthy();
    }
    for (const e of EXPRESSIONS) {
      expect(container.textContent, `expression ${e} is unlabelled`).toBeTruthy();
    }
  });

  /**
   * The second half of Grok's studio clause: *play every animation*.
   *
   * An avatar is otherwise only ever seen in whichever state the app happened
   * to load in, which makes the other fourteen a guess. All fifteen on screen at
   * once is the feature, so it is asserted rather than assumed.
   */
  it("previews all fifteen animation states", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, { props: { seed: "Researcher" } });
    const tiles = [...container.querySelectorAll(".raven-studio__anim")];
    expect(tiles, "the animation strip is half the point of the panel").toHaveLength(15);
    expect(ANIMATIONS.length).toBe(15);
    const shown = tiles.map((tile) => tile.querySelector(".raven-avatar")!.getAttribute("data-state"));
    expect(new Set(shown).size, "two tiles are previewing the same state").toBe(15);
    for (const a of ANIMATIONS) {
      expect(shown, `${a.id} is not previewed`).toContain(a.id);
    }
  });

  /** The strip has to preview the face being edited, not a fixed one. */
  it("previews the shape and expression being edited", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, {
      props: { seed: "Researcher", silhouette: "squircle", expression: "sad" },
    });
    const strip = container.querySelector(".raven-studio__states")!;
    expect(strip.querySelectorAll('[data-shape="squircle"]').length).toBe(STATES.length);
    // Every cell shows the same chosen face, because the face is identity and
    // does not vary with the state.
    const faces = [...strip.querySelectorAll(".raven-avatar-face")].map((g) => g.innerHTML);
    // `waiting` is the one override, so all but one agree and one differs.
    expect(new Set(faces).size).toBe(2);
  });

  it("hands the chosen face back on apply", async () => {
    setLocale("en");
    const onApply = vi.fn();
    const { container, getByText } = render(AvatarStudio, { props: { seed: "Ada", onApply } });

    // Pick the last shape chip in the shape row, then apply.
    const chips = [...container.querySelectorAll(".raven-studio__chip")];
    await fireEvent.click(chips[SHAPES.length - 1]);
    await fireEvent.click(getByText("Use this face"));

    expect(onApply).toHaveBeenCalledTimes(1);
    const [shape, expression, colour] = onApply.mock.calls[0];
    expect(SHAPES).toContain(shape);
    expect(EXPRESSIONS).toContain(expression);
    expect(COLOURS.map((c) => c.id)).toContain(colour);
  });

  /**
   * Colour used to be derived from the name and merely *shown*, on the argument
   * that a colour a user cannot set wrong is worth more than one more axis to
   * fiddle with. Grok makes it the third choosable axis alongside shape and
   * expression, which is the right call: it is part of identity, and identity
   * is exactly what the studio exists to set.
   */
  it("offers all twelve colours as swatches, and applies the one clicked", async () => {
    setLocale("en");
    const onApply = vi.fn();
    const { container, getByText } = render(AvatarStudio, {
      props: { seed: "Researcher", onApply },
    });
    const swatches = [...container.querySelectorAll(".raven-studio__swatch")];
    expect(swatches, "colour is the third axis now").toHaveLength(COLOURS.length);
    for (const c of COLOURS) {
      expect(container.textContent || container.innerHTML, `${c.id} has no label`).toBeTruthy();
    }

    await fireEvent.click(swatches[COLOURS.length - 1]);
    await fireEvent.click(getByText("Use this face"));
    expect(onApply.mock.calls[0][2]).toBe(COLOURS[COLOURS.length - 1].id);
  });
});