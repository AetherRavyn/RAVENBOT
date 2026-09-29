// @vitest-environment jsdom
/**
 * The agent avatar, in a DOM.
 *
 * Two kinds of avatar, and they are not interchangeable:
 *
 *  - A **DiceBear style** is an `<img>` holding a remote SVG. DiceBear 10.x puts
 *    the CSS keyframes inside that SVG, so the face moves on its own, and it
 *    sits behind `prefers-reduced-motion` so a visitor who asks for less motion
 *    gets a still one. The *seed* decides the face and nothing else — the avatar
 *    has no idea what the agent is doing.
 *  - `raven-native` is drawn here, as inline SVG, with a different set of
 *    geometry per expression. It is the only style that costs no request, and
 *    the only one whose face is a function of the agent's state.
 *
 * So "the avatar reacts to the agent's work" is true by two different mechanisms,
 * and the tests below pin each separately. Conflating them is how a system ends
 * up claiming reactivity while every face is a static image.
 *
 * The first version of the generated face injected its geometry with `{@html}`,
 * which in a Tauri window means a model-supplied agent name could inject script
 * into a surface holding the IPC bridge. Both paths are pinned here as emitting
 * elements and text, never a string of markup.
 */
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import { tick } from "svelte";
import RavenAvatar from "./RavenAvatar.svelte";
import RavenLogo from "./RavenLogo.svelte";
import { avatarProfile, faceFor, moodPresentation } from "$lib/avatar";
import { DEFAULT_AVATAR_STYLE, currentDiceBearUrl, parseDiceBearUrl } from "$lib/diceStyles";

afterEach(cleanup);

const MOODS = ["idle", "working", "waiting", "failed", "responded", "sleeping"] as const;

/**
 * The frame element, with only the props a test actually means to set.
 *
 * Omitting `style` matters: passing `null` is not the same as leaving it out,
 * and conflating them hides exactly the distinction the default-style test is
 * about.
 */
function frame(name: string, mood: string, extra: Record<string, unknown> = {}) {
  return render(RavenAvatar, { props: { name, mood: mood as any, ...extra } }).container
    .querySelector(".raven-avatar")!;
}

describe("RavenAvatar — a DiceBear style", () => {
  it("is an image, not injected markup", () => {
    const { container } = render(RavenAvatar, { props: { name: "Researcher" } });
    const img = container.querySelector("img");
    expect(img, "the default style should be a DiceBear image").not.toBeNull();
    expect(container.querySelector("svg")).toBeNull();
    expect(container.innerHTML).not.toContain("&lt;svg");
  });

  /** The whole point of the default. */
  it("defaults to a style that animates", () => {
    const el = frame("Researcher", "idle");
    expect(el.getAttribute("data-source")).toBe("dice");
    expect(el.getAttribute("data-animates")).toBe("true");
    expect(DEFAULT_AVATAR_STYLE).not.toBe("raven-native");
  });

  it("asks for the animation the catalogue records", () => {
    const { container } = render(RavenAvatar, { props: { name: "Ada" } });
    const src = container.querySelector("img")!.getAttribute("src")!;
    expect(src).toContain("api.dicebear.com/10.x/");
    expect(src).toContain("animationVariant=");
  });

  it("drops the animation when asked to", () => {
    const { container } = render(RavenAvatar, { props: { name: "Ada", animated: false } });
    const src = container.querySelector("img")!.getAttribute("src")!;
    expect(src).not.toContain("animationVariant");
    // And the frame must know, so it does not add motion of its own either.
    expect(frame("Ada", "idle", { animated: false }).getAttribute("data-animates")).toBeNull();
  });

  /**
   * A still style must be *known* to be still, or the frame breathes on top of a
   * face that never moves and the two read as a fault.
   */
  it("marks a still style as still", () => {
    const el = frame("Ada", "idle", { style: "notionists-neutral" });
    expect(el.getAttribute("data-source")).toBe("dice");
    expect(el.getAttribute("data-animates")).toBeNull();
  });

  /**
   * An agent with no stored style is most agents, since the column is newer than
   * some of them. It has to land on the default style, not slip through to the
   * generated face — one still face in a fleet of moving ones reads as a fault.
   */
  it("falls back to the default style when none was ever chosen", () => {
    for (const missing of [null, "", "   "]) {
      const el = frame("Ada", "idle", { style: missing });
      expect(el.getAttribute("data-source"), `style ${JSON.stringify(missing)}`).toBe("dice");
      expect(el.getAttribute("data-animates")).toBe("true");
    }
  });

  it("carries the mood on the frame, since the image cannot", () => {
    for (const mood of MOODS) {
      expect(frame("Ada", mood).getAttribute("data-mood")).toBe(mood);
    }
  });

  it("is deterministic: the same name and style give the same URL", () => {
    const a = render(RavenAvatar, { props: { name: "Ada", style: "clay" } })
      .container.querySelector("img")!.getAttribute("src");
    const b = render(RavenAvatar, { props: { name: "Ada", style: "clay" } })
      .container.querySelector("img")!.getAttribute("src");
    expect(b).toBe(a);
  });

  it("gives different agents different faces", () => {
    const a = render(RavenAvatar, { props: { name: "Ada", style: "clay" } })
      .container.querySelector("img")!.getAttribute("src")!;
    const b = render(RavenAvatar, { props: { name: "Grace", style: "clay" } })
      .container.querySelector("img")!.getAttribute("src")!;
    expect(b).not.toBe(a);
  });

  it("falls back to the generated face when the image cannot load", async () => {
    const { container } = render(RavenAvatar, { props: { name: "Ada" } });
    container.querySelector("img")!.dispatchEvent(new Event("error"));
    // Svelte batches the state change out of the handler, so the swap is not
    // visible until it flushes.
    await tick();
    // A fleet list full of broken-image icons is worse than one where every
    // face is at least a face.
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("svg")).not.toBeNull();
  });
});

describe("RavenAvatar — the generated face", () => {
  it("is drawn locally, with no request", () => {
    const { container } = render(RavenAvatar, { props: { name: "Ada", style: "raven-native" } });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("svg")).not.toBeNull();
    expect(frame("Ada", "idle", { style: "raven-native" }).getAttribute("data-source")).toBe("generated");
  });

  it("draws something for every mood", () => {
    for (const mood of MOODS) {
      const { container, unmount } = render(RavenAvatar, {
        props: { name: "Researcher", mood: mood as any, style: "raven-native" },
      });
      const svg = container.querySelector("svg")!;
      expect(
        svg.querySelectorAll("path, circle, ellipse, rect").length,
        `mood ${mood} drew no geometry`,
      ).toBeGreaterThan(0);
      unmount();
    }
  });

  /**
   * The reactivity the generated face has and a DiceBear style cannot: the
   * geometry itself is a function of the mood.
   */
  it("draws a different face for a working agent than an idle one", () => {
    const idle = render(RavenAvatar, { props: { name: "Ada", mood: "idle", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    const working = render(RavenAvatar, { props: { name: "Ada", mood: "working", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    expect(working).not.toBe(idle);
  });

  it("draws a different face for a failed agent than a working one", () => {
    const failed = render(RavenAvatar, { props: { name: "Ada", mood: "failed", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    const working = render(RavenAvatar, { props: { name: "Ada", mood: "working", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    expect(working).not.toBe(failed);
  });

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
          // imposing, so two agents resting are meant to look different.
          expect(expression).toBe(profile.resting);
        } else {
          expect(expression).toBe(moodPresentation(mood).expression);
        }
      }
    }
  });

  it("gives the same name the same face", () => {
    const a = render(RavenAvatar, { props: { name: "Ada", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    const b = render(RavenAvatar, { props: { name: "Ada", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    expect(b).toBe(a);
  });
});

describe("RavenAvatar — a custom image", () => {
  it("is used as given, never rewritten", () => {
    const url = "https://example.com/me.png";
    const { container } = render(RavenAvatar, { props: { name: "Ada", imageUrl: url } });
    expect(container.querySelector("img")!.getAttribute("src")).toBe(url);
    expect(frame("Ada", "idle", { imageUrl: url }).getAttribute("data-source")).toBe("custom");
  });

  it("outranks the style, because a user who uploaded a picture meant it", () => {
    const { container } = render(RavenAvatar, {
      props: { name: "Ada", imageUrl: "https://example.com/me.png", style: "clay" },
    });
    expect(container.querySelector("img")!.getAttribute("src")).toBe("https://example.com/me.png");
  });
});

describe("RavenAvatar — accessibility", () => {
  it("names itself when it is the only label, and hides itself when it is not", () => {
    const labelled = render(RavenAvatar, { props: { name: "Ada" } }).container
      .querySelector('[role="img"]');
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
   * Silent beside the wordmark, labelled as a control — deliberate opposites.
   * In the title bar the mark sits beside the word RAVENBOT, so announcing it
   * would read the name twice; as a control it is the only thing naming the app.
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

describe("stored DiceBear URLs", () => {
  it("recognises one and keeps the seed", () => {
    const found = parseDiceBearUrl("https://api.dicebear.com/9.x/bottts/svg?seed=Ada&radius=50");
    expect(found).not.toBeNull();
    expect(found!.style).toBe("bottts");
    expect(found!.seed).toBe("Ada");
    // A chosen background has to survive the upgrade.
    expect(found!.params.get("radius")).toBe("50");
  });

  it("leaves anything that is not a DiceBear URL alone", () => {
    for (const url of [
      "https://example.com/a.png",
      "/ravenicon.png",
      "data:image/svg+xml;base64,xxx",
      "not a url",
      "",
      null,
    ]) {
      expect(parseDiceBearUrl(url as any), `wrongly claimed ${url}`).toBeNull();
      expect(currentDiceBearUrl(url as any)).toBeNull();
    }
  });

  /**
   * The reason this function exists. A 9.x URL cannot animate — the option did
   * not exist — so every agent saved before the upgrade has a permanently still
   * face, and fixing it in the database would mean a migration for something the
   * renderer can do on its own.
   */
  it("upgrades a frozen 9.x URL to an animating 10.x one, same seed", () => {
    const before = "https://api.dicebear.com/9.x/voxel-bot/svg?seed=Ada&radius=50";
    const after = currentDiceBearUrl(before)!;
    expect(after.url).toContain("/10.x/voxel-bot/svg");
    expect(after.url).toContain("seed=Ada");
    expect(after.url).toContain("radius=50");
    expect(after.animates).toBe(true);
    expect(after.url).toContain("animationVariant=");
  });

  it("upgrades a still style without inventing motion", () => {
    const after = currentDiceBearUrl("https://api.dicebear.com/9.x/notionists-neutral/svg?seed=Ada")!;
    expect(after.animates).toBe(false);
    expect(after.url).not.toContain("animationVariant");
  });

  it("honours a request for a still avatar", () => {
    const after = currentDiceBearUrl("https://api.dicebear.com/9.x/clay/svg?seed=Ada", false)!;
    expect(after.animates).toBe(false);
    expect(after.url).not.toContain("animationVariant");
  });
});
