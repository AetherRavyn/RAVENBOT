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
import {
  ANIMATIONS,
  avatarProfile,
  COLOURS,
  EXPRESSIONS,
  faceFor,
  moodPresentation,
  SHAPES,
  UNREACHABLE_MOTIONS,
  type AvatarMood,
} from "$lib/avatar";
import { DEFAULT_AVATAR_STYLE, currentDiceBearUrl, parseDiceBearUrl } from "$lib/diceStyles";

afterEach(cleanup);

/**
 * Every roster mood. Kept in one place so a state added to the model shows up
 * here as a failure rather than as coverage that quietly stopped applying.
 */
const MOODS: AvatarMood[] = [
  "idle",
  "working",
  "thinking",
  "waiting",
  "failed",
  "responded",
  "sleeping",
];


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
   * The face stays put; the frame says what is happening.
   *
   * This asserted the opposite once — that a working agent drew a different face
   * — because that is what the code did. The face is identity: an agent that
   * changes character while it works cannot be recognised at a glance, which
   * defeats the point of a roster you learn to read. So the geometry is compared
   * *within* one mood, to prove the mood is what moved, and the face is compared
   * *across* moods, to prove it did not.
   */
  it("keeps the same face across moods and changes the state", () => {
    // The face group only: the rings are motion, drawn in the same SVG, and are
    // *supposed* to appear and disappear with the mood.
    const faceFor_ = (mood: string) =>
      render(RavenAvatar, { props: { name: "Ada", mood: mood as any, style: "raven-native" } })
        .container.querySelector(".raven-avatar-face")!.innerHTML;
    const faces = ["idle", "working", "failed", "responded"].map(faceFor_);
    expect(new Set(faces).size, "the drawn face changed with the mood").toBe(1);
  });

  it("changes the frame's motion state with the mood", () => {
    const state = (mood: string) =>
      render(RavenAvatar, { props: { name: "Ada", mood: mood as any, style: "raven-native" } })
        .container.querySelector(".raven-avatar")!.getAttribute("data-state");
    expect(["idle", "working", "waiting", "failed", "responded", "sleeping"].map(state))
      .toEqual(["idle", "thinking", "notification", "exclamation", "burst", "sleep"]);
  });

  /** The one face override, and it has to be visible. */
  it("lets a waiting agent look attentive", () => {
    const waiting = render(RavenAvatar, { props: { name: "Ada", mood: "waiting", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    const idle = render(RavenAvatar, { props: { name: "Ada", mood: "idle", style: "raven-native" } })
      .container.querySelector("svg")!.innerHTML;
    expect(waiting).not.toBe(idle);
  });

  it("resolves a drawable expression for every mood, for any name", () => {
    const names = ["Ada", "Grace", "Researcher", "", "a-very-long-agent-name"];
    const known = new Set<string>(EXPRESSIONS);
    for (const name of names) {
      const profile = avatarProfile(name);
      expect(profile, `no profile for ${JSON.stringify(name)}`).toBeTruthy();
      for (const mood of MOODS) {
        const expression = faceFor(profile, mood);
        expect(known.has(expression), `${mood} produced ${expression}`).toBe(true);
        // The face is identity, so it is the agent's own resting expression
        // unless — and only unless — the mood carries an override. `waiting` is
        // the only one that does.
        const override = moodPresentation(mood).expression;
        expect(expression).toBe(override ?? profile.resting);
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

/* ── The Grok avatar specification, rendered ───────────────────────────────── */

describe("RavenAvatar — the eight shapes", () => {
  /**
   * Every shape has to draw *something*, for every name.
   *
   * A shape that falls through to a default is not a crash and not an error, so
   * nothing else would notice. The check is on the outline element rather than
   * the whole SVG because the face differs per name and would mask it.
   */
  it("draws a distinct outline for each of Grok's eight shapes", () => {
    const drawn = SHAPES.map((shape) => {
      const { container } = render(RavenAvatar, {
        props: { name: "Ada", style: "raven-native", shape },
      });
      return container.querySelector(".raven-av-outline")!.getAttribute("d")!;
    });
    for (const d of drawn) expect(d.length).toBeGreaterThan(20);
    expect(new Set(drawn).size, "two shapes draw the same outline").toBe(SHAPES.length);
  });

  it("normalises a stored legacy shape rather than falling back", () => {
    const legacy = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", shape: "crystal" },
    }).container.querySelector(".raven-av-outline")!.getAttribute("d")!;
    const current = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", shape: "squircle" },
    }).container.querySelector(".raven-av-outline")!.getAttribute("d")!;
    expect(legacy).toBe(current);
    expect(legacy.length).toBeGreaterThan(20);
  });
});

describe("RavenAvatar — the twelve colours", () => {
  it("paints the body in the chosen colour, not a hue approximation", () => {
    for (const c of COLOURS) {
      const root = render(RavenAvatar, {
        props: { name: "Ada", style: "raven-native", colour: c.id },
      }).container;
      // The colour reaches the body as a custom property, which is what lets
      // the CSS transition and restyle it without redrawing the SVG.
      const wrap = root.querySelector(".raven-avatar") as HTMLElement;
      expect(wrap.style.getPropertyValue("--av-face"), `${c.id}`).toBe(c.hex);
      // And the body reads that property rather than an approximation of it.
      expect(root.querySelector(".raven-avatar-body path")!.getAttribute("fill")).toBe(
        "var(--av-face)",
      );
    }
  });

  it("puts the body colour in the custom properties the CSS reads", () => {
    const el = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", colour: "pink" },
    }).container.querySelector(".raven-avatar") as HTMLElement;
    expect(el.style.getPropertyValue("--av-face")).toBe(COLOURS.find((c) => c.id === "pink")!.hex);
  });
});

describe("RavenAvatar — the fifteen animation states", () => {
  /**
   * Every state must render something, and must say which one it is.
   *
   * `data-state` is what the whole stylesheet keys off, so a state that renders
   * with the wrong attribute is silent — it looks like a slightly less
   * interesting avatar rather than a broken one.
   */
  it("marks every state it is asked to play", () => {
    for (const a of ANIMATIONS) {
      const el = render(RavenAvatar, {
        props: { name: "Ada", style: "raven-native", state: a.id },
      }).container.querySelector(".raven-avatar")!;
      expect(el.getAttribute("data-state"), a.id).toBe(a.id);
    }
  });

  it("draws the parts each state's own CSS animates", () => {
    // The three states that carry a piece of SVG the others do not.
    const needs = [
      ["thinking", ".raven-av-dots"],
      ["notification", ".raven-av-badge"],
      ["exclamation", ".raven-av-alert"],
      ["sleep", ".raven-av-zzz"],
      ["comet", ".raven-av-tail"],
      ["burst", ".raven-av-rays"],
      ["alert", ".raven-av-pulse"],
      ["swirl", ".raven-av-swirl"],
    ] as const;
    for (const [state, selector] of needs) {
      const { container } = render(RavenAvatar, {
        props: { name: "Ada", style: "raven-native", state },
      });
      expect(container.querySelector(selector), `${state} is missing ${selector}`).toBeTruthy();
    }
  });

  /** Orbit's rings are mood-driven as well, so both paths must draw them. */
  it("draws orbit rings when the state asks and when the mood asks", () => {
    const byState = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", state: "orbit" },
    }).container;
    const byMood = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", mood: "thinking" },
    }).container;
    expect(byState.querySelector(".raven-avatar-rings")).toBeTruthy();
    expect(byMood.querySelector(".raven-avatar-rings")).toBeTruthy();
  });

  /**
   * The safety rule, at the only place it can actually be enforced.
   *
   * A mood may never produce one of the three states that replace an agent's
   * outline. The component applies `rosterSafeMotion` to the mood's state, so
   * this asserts the end-to-end behaviour rather than the helper in isolation.
   */
  it("never plays a body-changing state from a mood", () => {
    for (const mood of MOODS) {
      const el = render(RavenAvatar, {
        props: { name: "Ada", style: "raven-native", mood },
      }).container.querySelector(".raven-avatar")!;
      const played = el.getAttribute("data-state")!;
      expect(UNREACHABLE_MOTIONS as readonly string[]).not.toContain(played);
      expect(container_alt(el), `${mood} drew an alternate body`).toBeFalsy();
    }
  });

  it("does play one when asked by name, which is the studio previewing artwork", () => {
    const { container } = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", state: "egg" },
    });
    expect(container.querySelectorAll(".raven-av-altbody").length).toBeGreaterThan(0);
  });

  /**
   * `wink` closes one eye. The two eyes are separate elements precisely so this
   * is expressible, which means it is worth pinning: merging them back into one
   * group would still draw a face and would quietly delete the state.
   */
  it("gives each eye its own element, so wink can close exactly one", () => {
    const { container } = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", expression: "neutral" },
    });
    expect(container.querySelectorAll(".raven-av-eye")).toHaveLength(2);
    expect(container.querySelector(".raven-av-eye--l")).toBeTruthy();
    expect(container.querySelector(".raven-av-eye--r")).toBeTruthy();
  });
});

describe("RavenAvatar — the sixteen expressions", () => {
  it("draws a face for every expression", () => {
    const drawn = EXPRESSIONS.map((expression) =>
      render(RavenAvatar, {
        props: { name: "Ada", style: "raven-native", expression },
      }).container.querySelector(".raven-avatar-face")!.innerHTML,
    );
    for (const html of drawn) expect(html.length).toBeGreaterThan(0);
    expect(new Set(drawn).size, "two expressions draw the same face").toBe(EXPRESSIONS.length);
  });

  it("maps the historical expression name rather than falling back", () => {
    const legacy = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", expression: "pleased" },
    }).container.querySelector(".raven-avatar-face")!.innerHTML;
    const current = render(RavenAvatar, {
      props: { name: "Ada", style: "raven-native", expression: "happy" },
    }).container.querySelector(".raven-avatar-face")!.innerHTML;
    expect(legacy).toBe(current);
  });
});

function container_alt(el: Element): Element | null {
  return el.querySelector(".raven-av-altbody");
}
