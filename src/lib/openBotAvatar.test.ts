import { describe, expect, it } from "vitest";
import { validateAvatar } from "open-bot-avatar";
import {
  DEFAULT_OPENBOT_COLOUR,
  OPENBOT_COLOURS,
  OPENBOT_GALLERY,
  OPENBOT_INVALID_DEFINITIONS,
  OPENBOT_STATES,
  openBotColourForName,
  openBotDefinition,
  openBotDefinitionForName,
  openBotStateForMood,
  openBotStillUrl,
  openBotStyleId,
  openBotUrl,
  parseOpenBotStyle,
  parseOpenBotUrl,
  resolveOpenBotColour,
  resolveOpenBotDefinition,
} from "./openBotAvatar";
import { OPENBOT_GALLERY_SOURCE } from "./openBotGalleryData";
import type { AvatarMood } from "./avatar";

describe("OpenBot gallery", () => {
  it("bundles every upstream definition plus the package examples", () => {
    expect(OPENBOT_GALLERY_SOURCE).toHaveLength(23);
    expect(OPENBOT_GALLERY).toHaveLength(OPENBOT_GALLERY_SOURCE.length);
    expect(OPENBOT_INVALID_DEFINITIONS).toEqual([]);
  });

  it("validates every bundled definition with the package contract", () => {
    for (const definition of OPENBOT_GALLERY) {
      const result = validateAvatar(definition);
      expect(result.ok, `${definition.id} failed validation`).toBe(true);
    }
  });

  it("keeps ids unique and names present", () => {
    const ids = OPENBOT_GALLERY.map((definition) => definition.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const definition of OPENBOT_GALLERY) {
      expect(definition.name.trim().length).toBeGreaterThan(0);
      expect(openBotDefinition(definition.id)).toBe(definition);
    }
  });

  it("includes the seven package examples and the upstream gallery characters", () => {
    for (const id of [
      "controller",
      "barbell",
      "mug",
      "lightbulb",
      "chat",
      "sprout",
      "watering-can",
      "cat",
      "ghost",
      "rocket",
      "witch-hat",
    ]) {
      expect(openBotDefinition(id), `missing ${id}`).not.toBeNull();
    }
  });
});

describe("OpenBot URLs and styles", () => {
  it("builds and parses an encoded URL", () => {
    const url = openBotUrl("cat", "#F8A01C");
    expect(url).toBe("openbot://cat?color=%23f8a01c");
    expect(parseOpenBotUrl(url)).toEqual({ id: "cat", colour: "#f8a01c" });
  });

  it("accepts a missing colour and ignores unknown parameters", () => {
    expect(parseOpenBotUrl("openbot://ghost")).toEqual({ id: "ghost" });
    expect(parseOpenBotUrl("openbot://ghost?foo=bar&color=%23ff6a1a")).toEqual({
      id: "ghost",
      colour: "#ff6a1a",
    });
    expect(parseOpenBotUrl("openbot://ghost?color=#ff6a1a")).toEqual({
      id: "ghost",
      colour: "#ff6a1a",
    });
  });

  it("treats an invalid colour as missing so the name default applies", () => {
    expect(parseOpenBotUrl("openbot://cat?color=red")).toEqual({ id: "cat" });
    expect(parseOpenBotUrl("openbot://cat?color=%23123456ff")).toEqual({ id: "cat" });
    expect(resolveOpenBotColour({ name: "Ada", imageUrl: "openbot://cat?color=red" })).toBe(
      openBotColourForName("Ada"),
    );
  });

  it("falls back safely when the builder receives an invalid colour", () => {
    expect(openBotUrl("cat", "not-a-colour")).toBe(
      `openbot://cat?color=${encodeURIComponent(DEFAULT_OPENBOT_COLOUR)}`,
    );
  });

  it("rejects other schemes and empty ids", () => {
    expect(parseOpenBotUrl("https://example.com/me.png")).toBeNull();
    expect(parseOpenBotUrl("openbot://")).toBeNull();
    expect(parseOpenBotUrl(null)).toBeNull();
    expect(parseOpenBotUrl("openbot://?color=%23fff")).toBeNull();
  });

  it("parses the stored style form", () => {
    expect(openBotStyleId("cat")).toBe("openbot:cat");
    expect(parseOpenBotStyle("openbot:cat")).toBe("cat");
    expect(parseOpenBotStyle("openbot:witch-hat")).toBe("witch-hat");
    expect(parseOpenBotStyle("openbot:")).toBeNull();
    expect(parseOpenBotStyle("clay")).toBeNull();
    expect(parseOpenBotStyle(null)).toBeNull();
  });

  it("resolves explicit, URL and name in that order", () => {
    expect(
      resolveOpenBotDefinition({
        name: "Ada",
        definitionId: "ghost",
        imageUrl: "openbot://cat?color=%23ff6a1a",
      }).id,
    ).toBe("ghost");
    expect(
      resolveOpenBotDefinition({ name: "Ada", imageUrl: "openbot://cat?color=%23ff6a1a" }).id,
    ).toBe("cat");
    expect(resolveOpenBotDefinition({ name: "Ada" }).id).toBe(
      openBotDefinitionForName("Ada").id,
    );

    expect(
      resolveOpenBotColour({
        name: "Ada",
        colour: "#1fbf68",
        imageUrl: "openbot://cat?color=%23ff6a1a",
      }),
    ).toBe("#1fbf68");
    expect(
      resolveOpenBotColour({ name: "Ada", imageUrl: "openbot://cat?color=%23ff6a1a" }),
    ).toBe("#ff6a1a");
    expect(resolveOpenBotColour({ name: "Ada" })).toBe(openBotColourForName("Ada"));
  });
});

describe("stable defaults", () => {
  it("gives the same name the same character and colour", () => {
    expect(openBotDefinitionForName("Ada").id).toBe(openBotDefinitionForName("Ada").id);
    expect(openBotColourForName("Ada")).toBe(openBotColourForName("Ada"));
    expect(openBotColourForName("")).toBe(openBotColourForName("Agent"));
  });

  it("spreads a small set of names across more than one character and colour", () => {
    const names = ["Ada", "Grace", "Linus", "Margaret", "Barbara", "Edsger"];
    const definitions = new Set(names.map((name) => openBotDefinitionForName(name).id));
    const colours = new Set(names.map((name) => openBotColourForName(name)));
    expect(definitions.size).toBeGreaterThan(1);
    expect(colours.size).toBeGreaterThan(1);
    for (const colour of colours) expect(OPENBOT_COLOURS).toContain(colour);
  });

  it("has exactly the six renderer states", () => {
    expect(OPENBOT_STATES).toEqual([
      "idle",
      "thinking",
      "working",
      "waiting",
      "blocked",
      "done",
    ]);
  });
});

describe("RAVENBOT mood → OpenBot state", () => {
  const expected: Record<AvatarMood, string> = {
    idle: "idle",
    sleeping: "idle",
    thinking: "thinking",
    working: "working",
    waiting: "waiting",
    failed: "blocked",
    responded: "done",
  };

  it("maps every mood, including sleeping and responded", () => {
    for (const [mood, state] of Object.entries(expected)) {
      expect(openBotStateForMood(mood as AvatarMood)).toBe(state);
    }
  });

  it("treats a missing or unknown mood as idle", () => {
    expect(openBotStateForMood(null)).toBe("idle");
    expect(openBotStateForMood(undefined)).toBe("idle");
    expect(openBotStateForMood("nonsense" as AvatarMood)).toBe("idle");
  });
});

describe("OpenBot stills", () => {
  it("is a percent-encoded SVG data URL with geometry", () => {
    const url = openBotStillUrl("cat", "#f8a01c", "working", "happy");
    expect(url.startsWith("data:image/svg+xml;charset=utf-8,")).toBe(true);
    expect(url).not.toContain("<");
    expect(url).not.toContain("#");
    const svg = decodeURIComponent(url.slice("data:image/svg+xml;charset=utf-8,".length));
    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg).toContain("<path");
    expect(svg).toContain('viewBox="0 0 100 100"');
  });

  it("falls back to idle for an unknown runtime state", () => {
    const url = openBotStillUrl("cat", "#f8a01c", "exploded" as never);
    expect(decodeURIComponent(url)).toContain('viewBox="0 0 100 100"');
  });

  it("normalises the colour it renders", () => {
    const url = openBotStillUrl("cat", "f8a01c");
    const svg = decodeURIComponent(url);
    expect(svg).toContain('fill="#f8a01c"');
  });
});
