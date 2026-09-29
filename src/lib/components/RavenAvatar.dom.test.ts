// @vitest-environment jsdom
/**
 * The generated faces, in a DOM.
 *
 * `avatar.test.ts` covers the geometry as data: the seed a name produces, the
 * mood precedence, the expression each mood maps to. None of that proves a face
 * is *drawn* — and the component is where the first version of this went wrong.
 * It injected the geometry with `{@html}`, which in a Tauri window means a
 * model-supplied name could inject script into a surface that holds the IPC
 * bridge. The fix was to make the shape data declarative rather than markup, so
 * the thing worth pinning is that the component emits elements and never a
 * string of markup.
 *
 * The other thing worth pinning is that the face is *reactive* — the whole
 * point of the system is that an agent looks different while it works, and a
 * component that renders a static face would pass every geometry test.
 */
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import RavenAvatar from "./RavenAvatar.svelte";
import RavenLogo from "./RavenLogo.svelte";
import { avatarProfile, faceFor, moodPresentation } from "$lib/avatar";

afterEach(cleanup);

const MOODS = ["idle", "working", "waiting", "failed", "responded", "sleeping"] as const;

describe("RavenAvatar", () => {
  it("draws with elements rather than injected markup", () => {
    const { container } = render(RavenAvatar, { props: { name: "Researcher" } });
    expect(container.querySelector("svg")).not.toBeNull();
    // Svelte renders text as text; an `{@html}` sink would not.
    expect(container.innerHTML).not.toContain("&lt;svg");
  });

  it("draws something for every mood", () => {
    for (const mood of MOODS) {
      const { container, unmount } = render(RavenAvatar, { props: { name: "Researcher", mood } });
      const svg = container.querySelector("svg");
      expect(svg, `no svg for mood ${mood}`).not.toBeNull();
      // A face with no geometry is an empty box, which is not a face.
      expect(svg!.querySelectorAll("path, circle, ellipse, rect").length,
        `mood ${mood} drew no geometry`).toBeGreaterThan(0);
      unmount();
    }
  });

  /**
   * The reactive part. Two moods of the same agent must not draw identically —
   * otherwise "the agents are alive" is a claim about nothing.
   */
  it("draws a different face for a working agent than an idle one", () => {
    const idle = render(RavenAvatar, { props: { name: "Researcher", mood: "idle" } });
    const idlePaths = idle.container.querySelector("svg")!.innerHTML;
    idle.unmount();

    const working = render(RavenAvatar, { props: { name: "Researcher", mood: "working" } });
    const workingPaths = working.container.querySelector("svg")!.innerHTML;

    expect(workingPaths).not.toBe(idlePaths);
  });

  it("draws a different face for a failed agent than a working one", () => {
    const failed = render(RavenAvatar, { props: { name: "Researcher", mood: "failed" } });
    const failedPaths = failed.container.querySelector("svg")!.innerHTML;
    failed.unmount();
    const working = render(RavenAvatar, { props: { name: "Researcher", mood: "working" } });
    expect(working.container.querySelector("svg")!.innerHTML).not.toBe(failedPaths);
  });

  /**
   * Identity comes from the name, so two agents in one office must not be
   * interchangeable. Deterministic too: the same name has to look the same on
   * every render, or the fleet rearranges itself on each poll.
   */
  it("gives the same name the same face, and different names different ones", () => {
    const a = render(RavenAvatar, { props: { name: "Ada" } }).container.querySelector("svg")!.innerHTML;
    const b = render(RavenAvatar, { props: { name: "Ada" } }).container.querySelector("svg")!.innerHTML;
    expect(b).toBe(a);

    const other = render(RavenAvatar, { props: { name: "Grace" } }).container.querySelector("svg")!.innerHTML;
    // Not a hard requirement — two names may collide onto similar geometry — but
    // they must not be *identical* across the whole drawing.
    expect(other).not.toBe(a);
  });

  it("gives an agent with no name a face anyway", () => {
    expect(() => render(RavenAvatar, { props: { name: "" } })).not.toThrow();
    const { container } = render(RavenAvatar, { props: { name: "" } });
    expect(container.querySelector("svg")!.querySelectorAll("path, circle, ellipse, rect").length)
      .toBeGreaterThan(0);
  });

  it("prefers a custom image when one is given", () => {
    const { container } = render(RavenAvatar, {
      props: { name: "Ada", imageUrl: "https://example.com/a.png" },
    });
    const img = container.querySelector("img");
    expect(img).not.toBeNull();
    expect(img!.getAttribute("src")).toBe("https://example.com/a.png");
    // Not both: a face behind a portrait is a rendering accident, not a fallback.
    expect(container.querySelector("svg")).toBeNull();
  });

  /**
   * Whatever it draws, the expression must be one the face system defines.
   *
   * The two ends of the chain: a name produces a profile, a profile plus a mood
   * produces an expression, and every expression a mood can ask for has to be
   * one the renderer knows how to draw. A name the seeder cannot handle must
   * still yield a usable profile rather than nothing to draw.
   */
  it("resolves a drawable expression for every mood, for any name", () => {
    const names = ["Ada", "Grace", "Researcher", "", "a-very-long-agent-name"];
    const known = new Set(["neutral", "attentive", "pleased", "sad", "curious", "sleepy"]);
    for (const name of names) {
      const profile = avatarProfile(name);
      expect(profile, `no profile for ${JSON.stringify(name)}`).toBeTruthy();
      for (const mood of MOODS) {
        const expression = faceFor(profile, mood);
        expect(known.has(expression), `${mood} produced ${expression}`).toBe(true);
        if (mood === "idle") {
          // Idle is the one mood the agent chooses rather than the situation
          // imposing, so two agents resting look different. `moodPresentation`
          // says nothing useful here — its idle entry is a neutral fallback.
          expect(expression).toBe(profile.resting);
        } else {
          expect(expression).toBe(moodPresentation(mood).expression);
        }
      }
    }
  });

  it("names itself when it is the only label, and hides itself when it is not", () => {
    // The role and the label are on the drawn element, not the render wrapper.
    const labelled = render(RavenAvatar, { props: { name: "Ada" } }).container.querySelector('[role="img"]');
    expect(labelled).not.toBeNull();
    expect(labelled!.getAttribute("aria-label")).toBe("Ada");

    // Beside a visible name, an avatar is decoration and must say so, or a
    // screen reader reads the name twice.
    const decorative = render(RavenAvatar, { props: { name: "Ada", decorative: true } }).container;
    expect(decorative.querySelector('[aria-hidden="true"]')).not.toBeNull();
    expect(decorative.querySelector('[role="img"]')).toBeNull();
  });
});

describe("RavenLogo", () => {
  it("draws as elements", () => {
    const { container } = render(RavenLogo, { props: {} });
    expect(container.querySelector("svg")).not.toBeNull();
    expect(container.querySelectorAll("path, circle, ellipse, rect").length).toBeGreaterThan(0);
  });

  /**
   * The mark is silent on its own and labelled when it is a control.
   *
   * In the title bar it sits beside the word RAVENBOT, so announcing it would
   * read the name twice; as an interactive control it is the only thing naming
   * the app, so it needs a label. Both are deliberate and the opposite of each
   * other, which is why they are pinned together.
   */
  it("is silent beside the wordmark and labelled when it is a control", () => {
    const plain = render(RavenLogo, { props: {} }).container;
    expect(plain.querySelector('[aria-hidden="true"]')).not.toBeNull();
    expect(plain.querySelector('[role="button"]')).toBeNull();

    const interactive = render(RavenLogo, { props: { interactive: true } }).container;
    const control = interactive.querySelector('[role="button"]');
    expect(control).not.toBeNull();
    expect(control!.getAttribute("aria-label")).toBeTruthy();
  });
});
