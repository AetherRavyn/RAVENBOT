/**
 * OpenBot characters: a bundled gallery, a URL scheme and the mood mapping
 * that lets RAVENBOT render them beside Raven faces and DiceBear styles.
 *
 * The definitions and the renderer are MIT licensed, from
 * DominikScholz/open-bot-avatar (github.com/DominikScholz/open-bot-avatar).
 * Nothing here fetches at runtime: the gallery is compiled into the bundle,
 * every definition is validated with the package's own `validateAvatar`, and a
 * still is rendered to an SVG data URL by the package's `renderAvatarSvg`.
 *
 * Storage keeps the whole character in two existing columns, with no schema
 * change:
 *
 *   avatar_style = "openbot:cat"
 *   avatar_url   = "openbot://cat?color=%23f8a01c"
 *
 * The style names the character, the URL names the character and its body
 * colour, and both are enough for an agent to reload identically.
 */
import {
  renderAvatarSvg,
  validateAvatar,
  type AvatarDefinition,
  type AvatarMood as OpenBotMood,
  type AvatarState,
} from "open-bot-avatar";
import { stableIndex, type AvatarMood } from "$lib/avatar";
import { OPENBOT_GALLERY_SOURCE } from "$lib/openBotGalleryData";

/** The style prefix a stored agent record carries. */
export const OPENBOT_STYLE = "openbot";

/** The six states the OpenBot renderer knows. */
export type OpenBotState = AvatarState;

/** The OpenBot renderer's mood axis. */
export type OpenBotTone = OpenBotMood;

/** The six states, in the order a person meets them. */
export const OPENBOT_STATES: readonly OpenBotState[] = [
  "idle",
  "thinking",
  "working",
  "waiting",
  "blocked",
  "done",
] as const;

/** The package's default body colour, and the safe fallback for bad input. */
export const DEFAULT_OPENBOT_COLOUR = "#ff6a1a";

/**
 * A curated body palette, taken from the upstream gallery's own colours.
 *
 * Kept local rather than read from `gallery/colors.json` at runtime: the file
 * is a build-time input, not something the app may fetch. `#f4f4f4` is
 * included because several upstream characters (the mug, egg and ghost) are a
 * warm white, and it keeps its readability with the renderer's dark ink.
 */
export const OPENBOT_COLOURS: readonly string[] = [
  "#ff6a1a",
  "#f2384b",
  "#f8a01c",
  "#8b5cf6",
  "#1fbf68",
  "#14b3a5",
  "#a8693f",
  "#ec4899",
  "#1f7cf2",
  "#7a7a7a",
  "#f4f4f4",
] as const;

/**
 * The colour upstream uses for a character. Gallery thumbnails use it so the
 * characters look like the reference; a name-derived agent still gets a
 * stable colour from `openBotColourForName`.
 */
const CURATED_COLOUR: Readonly<Record<string, string>> = {
  controller: "#ff6a1a",
  barbell: "#f2384b",
  mug: "#f4f4f4",
  lightbulb: "#f8a01c",
  chat: "#8b5cf6",
  sprout: "#1fbf68",
  "watering-can": "#14b3a5",
  book: "#a8693f",
  brain: "#ec4899",
  bull: "#f2384b",
  bunny: "#ec4899",
  camera: "#7a7a7a",
  cat: "#f8a01c",
  cloud: "#14b3a5",
  egg: "#f4f4f4",
  ghost: "#f4f4f4",
  magnifier: "#1f7cf2",
  "paint-brush": "#1fbf68",
  palette: "#a8693f",
  rocket: "#7a7a7a",
  shield: "#1f7cf2",
  star: "#ff6a1a",
  "witch-hat": "#8b5cf6",
};

/**
 * Definitions that failed the package's contract, with the reason.
 *
 * Empty in a healthy build. The gallery below drops them rather than shipping
 * a renderer error into a roster; `openBotAvatar.test.ts` asserts this list is
 * empty and that the gallery kept every bundled definition, so a bad edit
 * fails loudly in CI instead of quietly shrinking the picker.
 */
export const OPENBOT_INVALID_DEFINITIONS: string[] = [];

const validated: AvatarDefinition[] = [];
for (const candidate of OPENBOT_GALLERY_SOURCE) {
  const result = validateAvatar(candidate);
  if (result.ok) validated.push(result.value);
  else OPENBOT_INVALID_DEFINITIONS.push(`${candidate.id ?? "unknown"}: ${result.errors.join("; ")}`);
}
if (OPENBOT_INVALID_DEFINITIONS.length > 0 && typeof console !== "undefined") {
  console.warn(
    "openBotAvatar: dropped invalid bundled definitions",
    OPENBOT_INVALID_DEFINITIONS.join("\n"),
  );
}

/**
 * Every bundled OpenBot character: the upstream gallery plus the package's
 * seven examples. Sorted by display name for the picker, with the array
 * itself stable so hashes and gallery order never move.
 */
export const OPENBOT_GALLERY: readonly AvatarDefinition[] = Object.freeze(
  [...validated].sort((a, b) => a.name.localeCompare(b.name)),
);

const BY_ID = new Map(OPENBOT_GALLERY.map((definition) => [definition.id, definition]));

/** Look a character up by its definition id. */
export function openBotDefinition(id: string | null | undefined): AvatarDefinition | null {
  if (typeof id !== "string") return null;
  return BY_ID.get(id.trim().toLowerCase()) ?? null;
}

/** The curated upstream colour for a character, used for gallery tiles. */
export function openBotCuratedColour(id: string): string {
  return CURATED_COLOUR[id] ?? DEFAULT_OPENBOT_COLOUR;
}

/** `openbot:cat`, the value stored in `avatar_style`. */
export function openBotStyleId(id: string): string {
  return `${OPENBOT_STYLE}:${id.trim().toLowerCase()}`;
}

/**
 * The definition id in an `openbot:<id>` style, or null for every other style.
 *
 * A style is the cheap check (it survives a missing URL), so callers use this
 * before falling into DiceBear/custom handling.
 */
export function parseOpenBotStyle(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!trimmed.toLowerCase().startsWith(`${OPENBOT_STYLE}:`)) return null;
  const id = trimmed.slice(OPENBOT_STYLE.length + 1).trim();
  return id.length > 0 ? id : null;
}

export interface OpenBotUrlParts {
  /** The definition id in the host position. */
  id: string;
  /** A valid hex colour, or undefined when the URL had none (or an invalid one). */
  colour?: string;
}

/**
 * `openbot://<definition-id>?color=<url-encoded-hex>`.
 *
 * A missing colour is legal and means "derive one from the name"; unknown
 * parameters are ignored. The parser is deliberately not `new URL` for the
 * colour value, because a raw `#` in a query is read as a fragment by the URL
 * parser — a hand-written URL should still work, and a builder-produced one
 * always escapes it.
 */
export function parseOpenBotUrl(value: string | null | undefined): OpenBotUrlParts | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!trimmed.toLowerCase().startsWith("openbot://")) return null;
  const rest = trimmed.slice("openbot://".length);
  const queryAt = rest.indexOf("?");
  const rawId = (queryAt === -1 ? rest : rest.slice(0, queryAt)).trim();
  if (rawId.length === 0) return null;

  let colour: string | undefined;
  if (queryAt !== -1) {
    for (const pair of rest.slice(queryAt + 1).split("&")) {
      const equals = pair.indexOf("=");
      const key = equals === -1 ? pair : pair.slice(0, equals);
      if (decodePart(key) !== "color") continue;
      const raw = equals === -1 ? "" : pair.slice(equals + 1);
      // A raw # is technically a fragment; a value that starts with one is the
      // colour itself, while a fragment after an encoded value is ignored.
      const candidate = raw.startsWith("#") ? raw : raw.split("#")[0];
      const decoded = decodePart(candidate);
      if (decoded) colour = normaliseOpenBotColour(decoded, "");
      break;
    }
  }

  return { id: decodePart(rawId), ...(colour ? { colour } : {}) };
}

function decodePart(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;

/**
 * Accept `#rgb` / `#rrggbb`, with or without the hash, and lower-case it.
 * Anything else (a CSS colour, a gradient, an 8-digit value the renderer's
 * fast path does not accept) falls back to `fallback` rather than reaching a
 * styling sink.
 */
export function normaliseOpenBotColour(
  value: string | null | undefined,
  fallback: string = DEFAULT_OPENBOT_COLOUR,
): string {
  if (typeof value !== "string") return fallback;
  const trimmed = value.trim();
  const withHash = trimmed.startsWith("#") ? trimmed : `#${trimmed}`;
  return HEX.test(withHash) ? withHash.toLowerCase() : fallback;
}

/** Build a complete OpenBot URL. An invalid colour falls back safely. */
export function openBotUrl(id: string, colourHex?: string | null): string {
  const definitionId = ((id ?? "").trim() || OPENBOT_GALLERY[0]?.id || "idle").toLowerCase();
  const colour = normaliseOpenBotColour(colourHex, DEFAULT_OPENBOT_COLOUR);
  return `openbot://${encodeURIComponent(definitionId)}?color=${encodeURIComponent(colour)}`;
}

/** A stable character for a name, used when an agent has not chosen one. */
export function openBotDefinitionForName(name: string): AvatarDefinition {
  const seed = (name ?? "").trim() || "Agent";
  return OPENBOT_GALLERY[stableIndex(`${seed}:openbot`, OPENBOT_GALLERY.length)];
}

/** A stable body colour for a name, used when no colour was ever chosen. */
export function openBotColourForName(name: string): string {
  const seed = (name ?? "").trim() || "Agent";
  return OPENBOT_COLOURS[stableIndex(`${seed}:openbot-colour`, OPENBOT_COLOURS.length)];
}

/** Resolve a definition id, an `openbot://` URL or a name into a definition. */
export function resolveOpenBotDefinition(input: {
  name?: string | null;
  definitionId?: string | null;
  imageUrl?: string | null;
}): AvatarDefinition {
  return (
    openBotDefinition(input.definitionId) ??
    (input.imageUrl ? openBotDefinition(parseOpenBotUrl(input.imageUrl)?.id) : null) ??
    openBotDefinitionForName(input.name ?? "Agent")
  );
}

/** Resolve a colour with the documented precedence: explicit, URL, name. */
export function resolveOpenBotColour(input: {
  name?: string | null;
  colour?: string | null;
  imageUrl?: string | null;
}): string {
  const explicit = input.colour ? normaliseOpenBotColour(input.colour, "") : "";
  if (explicit) return explicit;
  const fromUrl = parseOpenBotUrl(input.imageUrl)?.colour;
  if (fromUrl) return fromUrl;
  return openBotColourForName(input.name ?? "Agent");
}

/**
 * RAVENBOT's seven moods onto the renderer's six states.
 *
 * `sleeping` is the one mood with no separate OpenBot state: it is an agent
 * with nothing happening, which is idle. `responded` becomes `done`, the
 * package's celebration pose, and `failed` becomes `blocked`, its stopped pose.
 */
export function openBotStateForMood(mood: AvatarMood | null | undefined): OpenBotState {
  switch (mood) {
    case "thinking":
      return "thinking";
    case "working":
      return "working";
    case "waiting":
      return "waiting";
    case "failed":
      return "blocked";
    case "responded":
      return "done";
    case "idle":
    case "sleeping":
    default:
      return "idle";
  }
}

/** The renderer's two-mood axis, from a RAVENBOT mood. */
export function openBotToneForMood(mood: AvatarMood | null | undefined): OpenBotTone {
  return mood === "responded" ? "happy" : "neutral";
}

export function isOpenBotState(value: unknown): value is OpenBotState {
  return typeof value === "string" && (OPENBOT_STATES as readonly string[]).includes(value);
}

/**
 * One fixed frame as a self-contained SVG data URL.
 *
 * Used by gallery tiles, the studio's six-state strip, and the fallback when a
 * DOM/WebGL host is unavailable. No canvas, no raw HTML insertion: the SVG is
 * escaped by the renderer and then percent-encoded as a whole.
 */
export function openBotStillUrl(
  definition: AvatarDefinition | string,
  colourHex?: string | null,
  state: OpenBotState = "idle",
  mood: OpenBotTone = "neutral",
): string {
  const resolved =
    typeof definition === "string"
      ? openBotDefinition(definition) ?? openBotDefinitionForName(definition)
      : definition;
  const colour = normaliseOpenBotColour(colourHex, DEFAULT_OPENBOT_COLOUR);
  const safeState = isOpenBotState(state) ? state : "idle";
  const safeMood: OpenBotTone = mood === "happy" ? "happy" : "neutral";
  try {
    const svg = renderAvatarSvg(resolved, {
      color: colour,
      state: safeState,
      mood: safeMood,
      reducedMotion: true,
      size: 96,
    });
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  } catch (error) {
    console.warn("openBotAvatar: could not render still", error);
    const fallback = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path d="M20 20H80V80H20Z"/></svg>';
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(fallback)}`;
  }
}
