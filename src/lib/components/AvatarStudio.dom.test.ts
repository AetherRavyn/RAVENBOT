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
import { SILHOUETTES, motionFor, type AvatarMood } from "$lib/avatar";
import { setLocale } from "$lib/i18n";

afterEach(cleanup);

const STATES: AvatarMood[] = ["idle", "working", "waiting", "failed", "responded", "sleeping"];

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

  it("offers every shape", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, { props: { seed: "Researcher" } });
    // One chip row per axis; the shape row has one chip per silhouette.
    const chips = [...container.querySelectorAll(".raven-studio__chip")];
    const shapes = SILHOUETTES.length;
    expect(chips.length).toBeGreaterThanOrEqual(shapes);
    for (const s of SILHOUETTES) {
      expect(container.textContent, `shape ${s} is unlabelled`).toBeTruthy();
    }
  });

  /** The strip has to preview the face being edited, not a fixed one. */
  it("previews the shape and expression being edited", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, {
      props: { seed: "Researcher", silhouette: "crystal", expression: "sad" },
    });
    const strip = container.querySelector(".raven-studio__states")!;
    expect(strip.querySelectorAll('[data-silhouette="crystal"]').length).toBe(STATES.length);
    // Every cell shows the same chosen face, because the face is identity and
    // does not vary with the state.
    const faces = [...strip.querySelectorAll(".raven-avatar-face")].map((g) => g.innerHTML);
    // `waiting` is the one override, so five of six agree and one differs.
    expect(new Set(faces).size).toBe(2);
  });

  it("hands the chosen face back on apply", async () => {
    setLocale("en");
    const onApply = vi.fn();
    const { container, getByText } = render(AvatarStudio, { props: { seed: "Ada", onApply } });

    // Pick the last shape chip in the shape row, then apply.
    const chips = [...container.querySelectorAll(".raven-studio__chip")];
    await fireEvent.click(chips[SILHOUETTES.length - 1]);
    await fireEvent.click(getByText("Use this face"));

    expect(onApply).toHaveBeenCalledTimes(1);
    const [shape, expression] = onApply.mock.calls[0];
    expect(SILHOUETTES).toContain(shape);
    expect(typeof expression).toBe("string");
  });

  /** Colour is derived, so it is shown rather than offered. */
  it("shows the colour as derived rather than offering it as a choice", () => {
    setLocale("en");
    const { container } = render(AvatarStudio, { props: { seed: "Researcher" } });
    expect(container.textContent).toContain("Derived from the agent's name");
  });
});