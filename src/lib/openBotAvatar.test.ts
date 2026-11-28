import { describe, expect, it } from "vitest";
import { validateAvatar } from "open-bot-avatar";
import {
  DEFAULT_OPENBOT_COLOUR,
  OPENBOT_COLOURS,
  OPENBOT_GALLERY,
  OPENBOT_INVALID_DEFINITIONS,
  OPENBOT_STATES,
  openBotActivityForTool,
  openBotColourForName,
  openBotDefinition,
  openBotDefinitionForName,
  openBotStateForActivity,
  openBotStateForMood,
  openBotStillUrl,
  openBotStyleId,
  openBotUrl,
  OPENBOT_ACTIVITIES,
  parseOpenBotStyle,
  parseOpenBotUrl,
  resolveOpenBotColour,
  resolveOpenBotDefinition,
  surpriseOpenBotId,
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

  it("round-trips every render option, with color first", () => {
    const url = openBotUrl("cat", {
      colour: "#F8A01C",
      depth: 9,
      followPointer: true,
      finish: "fluffy",
      renderer: "scene",
    });
    expect(url).toBe(
      "openbot://cat?color=%23f8a01c&depth=9&follow=1&finish=fluffy&renderer=scene",
    );
    expect(url.indexOf("color=")).toBeLessThan(url.indexOf("depth="));
    expect(parseOpenBotUrl(url)).toEqual({
      id: "cat",
      colour: "#f8a01c",
      depth: 9,
      followPointer: true,
      finish: "fluffy",
      renderer: "scene",
    });
  });

  it("writes false follow and flat finish, and omits canvas", () => {
    const url = openBotUrl("ghost", {
      colour: "#f4f4f4",
      depth: 0,
      followPointer: false,
      finish: "flat",
      renderer: "canvas",
    });
    expect(url).toBe("openbot://ghost?color=%23f4f4f4&depth=0&follow=0&finish=flat");
    expect(parseOpenBotUrl(url)).toEqual({
      id: "ghost",
      colour: "#f4f4f4",
      depth: 0,
      followPointer: false,
      finish: "flat",
    });
  });

  it("keeps the old colour-only call shape and stored URL working", () => {
    expect(openBotUrl("ghost", "#f4f4f4")).toBe("openbot://ghost?color=%23f4f4f4");
    expect(openBotUrl("ghost", { colour: "#f4f4f4" })).toBe(
      "openbot://ghost?color=%23f4f4f4",
    );
    expect(parseOpenBotUrl("openbot://ghost?color=%23f4f4f4")).toEqual({
      id: "ghost",
      colour: "#f4f4f4",
    });
  });

  it("omits invalid builder options instead of writing them", () => {
    const url = openBotUrl("cat", {
      colour: "#f8a01c",
      depth: 15,
      followPointer: "yes" as never,
      finish: "satin" as never,
      renderer: "webgl" as never,
    });
    expect(url).toBe("openbot://cat?color=%23f8a01c");
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

  it("ignores invalid render options rather than passing them through", () => {
    const invalid =
      "openbot://cat?color=%23f8a01c&depth=15&follow=2&finish=metal&renderer=canvas";
    expect(parseOpenBotUrl(invalid)).toEqual({ id: "cat", colour: "#f8a01c" });
    expect(
      parseOpenBotUrl(
        "openbot://cat?depth=-1&follow=true&finish=Flat&renderer=webgl&color=%23f8a01c",
      ),
    ).toEqual({ id: "cat", colour: "#f8a01c" });
    expect(parseOpenBotUrl("openbot://cat?depth=3.5&depth=abc")).toEqual({ id: "cat" });
  });

  it("parses options without a colour", () => {
    expect(
      parseOpenBotUrl("openbot://cat?depth=4&follow=1&finish=glass"),
    ).toEqual({ id: "cat", depth: 4, followPointer: true, finish: "glass" });
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

describe("OpenBot tool activity", () => {
  it("maps the common tool-name shapes onto the activity vocabulary", () => {
    const cases: Array<[string, string]> = [
      ["web_search", "searching"],
      ["grep", "searching"],
      ["browser_dom", "browsing"],
      ["web_fetch", "browsing"],
      ["file_read", "reading"],
      ["read_file", "reading"],
      ["list_dir", "reading"],
      ["file_write", "editing"],
      ["apply_patch", "editing"],
      ["spawn_agent", "delegating"],
      ["run_tests", "reviewing"],
      ["git_diff", "reviewing"],
      ["shell_exec", "running"],
      ["terminal", "running"],
      ["some_new_tool", "running"],
    ];
    for (const [tool, expected] of cases) {
      expect(openBotActivityForTool(tool), tool).toBe(expected);
      expect(OPENBOT_ACTIVITIES).toContain(expected);
    }
    expect(openBotActivityForTool("")).toBeNull();
    expect(openBotActivityForTool(null)).toBeNull();
  });

  it("uses the nearest of the six renderer poses and documents the sharing", () => {
    const cases = [
      ["searching", "thinking"],
      ["browsing", "thinking"],
      ["reading", "thinking"],
      ["editing", "working"],
      ["running", "working"],
      ["delegating", "working"],
      ["reviewing", "waiting"],
    ] as const;
    for (const [activity, state] of cases) {
      expect(openBotStateForActivity(activity), activity).toBe(state);
    }
    expect(openBotStateForActivity(null)).toBe("working");
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

describe("OpenBot surprise shuffle", () => {
  it("never picks the current character", () => {
    for (const definition of OPENBOT_GALLERY) {
      const next = surpriseOpenBotId(definition.id, () => 0);
      expect(next).not.toBe(definition.id);
      expect(openBotDefinition(next)).not.toBeNull();
    }
  });

  it("is deterministic for an injected roll and clamps the edges", () => {
    const notCurrent = OPENBOT_GALLERY.filter((definition) => definition.id !== "cat");
    expect(surpriseOpenBotId("cat", () => 0)).toBe(notCurrent[0].id);
    expect(surpriseOpenBotId("cat", () => 0.999999)).toBe(
      notCurrent[notCurrent.length - 1].id,
    );
    expect(surpriseOpenBotId("cat", () => 1)).toBe(
      notCurrent[notCurrent.length - 1].id,
    );
  });

  it("treats a missing or unknown current id as no exclusion", () => {
    expect(surpriseOpenBotId(null, () => 0)).toBe(OPENBOT_GALLERY[0].id);
    expect(surpriseOpenBotId("not-a-character", () => 0)).toBe(OPENBOT_GALLERY[0].id);
  });
});
