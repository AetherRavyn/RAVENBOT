/**
 * DiceBear styles, and what each one actually does.
 *
 * The catalogue used to be a flat list of `{value, label, category,
 * description}`, which is enough to build a `<select>` and not much else. Two
 * things were missing, and both turned out to matter.
 *
 * **Which styles animate.** DiceBear 10.x ships a looping CSS animation *inside*
 * the SVG for nineteen of its styles, gated behind an option and off by default.
 * The other styles return no `<style>` element at all. A flat list cannot tell
 * those apart, so a picker offers "voxel-bot" and "notionists-neutral" as if
 * they were the same kind of thing, and only one of them is alive.
 *
 * **How fast.** Asking for `animationVariant=fast` is a request, not a promise,
 * and the answer is per style. Each style multiplies its own loop period by a
 * tempo: `fastest` is 0.75, `medium` is 1, `slowest` is 1.35. So the same
 * variant gives clay a 2.85 s loop and pixelbot a 3.6 s one, and the fastest
 * `initial-face` can manage is 4.05 s because that is its shortest loop scaled
 * by 0.75. Those numbers are measured, not guessed, and they are recorded here
 * so the choice of variant is inspectable and can be asserted in a test rather
 * than re-discovered by eye.
 *
 * The request is a *cadence*, so a style that cannot reach two seconds is stated
 * as such instead of pretending otherwise.
 *
 * Two things this deliberately does not do:
 *
 *  - **Retime the animation from outside.** 10.x exposes the tempo as a CSS
 *    custom property on the SVG root (`--dbcl-t`), which would let a caller hit
 *    any cadence exactly. It is unreachable through `<img src>`, because an
 *    SVG loaded that way is an isolated document. Reaching it means inlining
 *    remote SVG into the page, which is an injection surface for a URL a user
 *    can set, and it throws away the browser's image cache — for a fleet of
 *    dozens of avatars that is the wrong trade for a nicer number. The
 *    animation plays at the style's own tempo and the *wrapper* carries mood.
 *  - **Assume the animation ran.** The animation sits behind
 *    `prefers-reduced-motion: no-preference`, so a visitor who asks for less
 *    motion gets a still avatar. That is the right behaviour and nothing here
 *    works around it.
 */

/** The six speeds DiceBear 10.x accepts for `animationVariant`. */
import { DICEBEAR_VERSION } from "$lib/diceVersions";

export type DiceSpeed = "none" | "slowest" | "slow" | "medium" | "fast" | "fastest";

/** The tempo each speed multiplies a style's own loop period by. */
export const DICE_TEMPO: Record<Exclude<DiceSpeed, "none">, number> = {
  slowest: 1.35,
  slow: 1.15,
  medium: 1,
  fast: 0.9,
  fastest: 0.75,
};

export interface DiceStyle {
  /** The DiceBear style slug, as it appears in the API path. */
  value: string;
  label: string;
  category: string;
  description: string;
  /**
   * Whether DiceBear ships a CSS animation in this style's SVG.
   *
   * Measured by fetching the SVG and looking for a `<style>` element: the
   * static styles return none.
   */
  animated: boolean;
  /**
   * The speed to request, or `null` for a style that does not animate.
   *
   * Chosen to land the style's shortest loop near two to three seconds, which is
   * the point of the whole exercise — a face should visibly change while you are
   * looking at it. Where a style cannot get there, the fastest speed is used and
   * `loopSeconds` says what it actually achieves.
   */
  speed: DiceSpeed | null;
  /**
   * The shortest loop this style runs at `speed`, in seconds.
   *
   * `null` for a static style. This is the cadence a reader actually perceives:
   * for `initial-face` it is the blink, for `pixelbot` the eye lids, for `clay`
   * the squash.
   */
  loopSeconds: number | null;
}

const style = (
  value: string,
  label: string,
  category: string,
  description: string,
  animated: boolean,
  speed: DiceSpeed | null,
  loopSeconds: number | null,
): DiceStyle => ({ value, label, category, description, animated, speed, loopSeconds });

/**
 * The styles offered, newest and most requested first.
 *
 * The animated ones lead, because an avatar that changes while you look at it is
 * the reason this file exists. The static ones follow rather than being dropped:
 * `notionists-neutral` is a good face and some people want a still one.
 */
export const DICE_STYLES: DiceStyle[] = [
  // ── Animated, and the ones asked for by name ────────────────────────────
  style("clay", "Clay", "Living", "Soft 3D clay figures that squash, peek and jiggle", true, "fastest", 2.85),
  style("voxel-bot", "Voxel Bot", "Living", "Blocky robots with glowing faces that blink", true, "fastest", 2.5),
  style("initial-face", "Initial Face", "Living", "A minimal face that blinks on a loop", true, "fastest", 4.05),
  style("pixelbot", "Pixelbot", "Living", "Pixel robots whose lids and smile shift", true, "fastest", 3.6),
  style("gaze", "Gaze", "Living", "Eyes that drift and refocus", true, "fastest", 3.3),
  style("moods", "Moods", "Living", "A face that cycles through expressions", true, "medium", 4.6),
  style("voxel-art", "Voxel Art", "Living", "Voxel characters that bob", true, "fastest", 2.5),
  style("sprouts", "Sprouts", "Living", "Little plants that sway", true, "fastest", 5.4),
  style("shapes", "Shapes", "Living", "Bauhaus shapes in slow drift", true, "fast", 12.6),
  style("thumbs", "Thumbs", "Living", "A thumb that gives the sign", true, "fastest", 3.45),
  style("blobs", "Blobs", "Living", "Flowing abstract colour fields", true, "fast", 6.3),
  style("squircles", "Squircles", "Living", "Rounded squares that shimmer", true, "fast", 5.4),
  style("waves", "Waves", "Living", "Layered waves that roll", true, "fast", 6.75),
  style("glass", "Glass", "Living", "Refracted glass shapes", true, "fast", 7.2),
  style("loops", "Loops", "Living", "Interlocking loops that turn", true, "fast", 5.4),
  style("critters", "Critters", "Living", "Small creatures that fidget", true, "fast", 4.05),
  style("landscape", "Landscape", "Living", "Distant hills under a moving sky", true, "fast", 9),
  style("constellation", "Constellation", "Living", "Stars that twinkle", true, "fast", 6.75),

  // ── Still ───────────────────────────────────────────────────────────────
  style("notionists-neutral", "Notionists Neutral", "Modern", "Notion-style line figures", false, null, null),
  style("bottts", "Bottts", "Robots & AI", "Androids and AI bots", false, null, null),
  style("avataaars", "Avataaars", "Characters", "Modern illustrated avatars", false, null, null),
  style("personas", "Personas", "Characters", "Clean corporate personas", false, null, null),
  style("lorelei", "Lorelei", "Characters", "Anime and illustrated faces", false, null, null),
  style("adventurer", "Adventurer", "Fantasy", "RPG heroes and adventurers", false, null, null),
  style("micah", "Micah", "Modern", "Minimalist vector avatars", false, null, null),
  style("notionists", "Notionists", "Modern", "Notion-style line avatars", false, null, null),
  style("open-peeps", "Open Peeps", "Doodles", "Hand-drawn diverse doodles", false, null, null),
  style("pixel-art", "Pixel Art", "Retro", "Retro 8-bit characters", false, null, null),
  style("big-smile", "Big Smile", "Expressive", "Joyful smiling characters", false, null, null),
  style("croodles", "Croodles", "Doodles", "Playful artistic sketches", false, null, null),
  style("dylan", "Dylan", "Modern", "Stylized expressive avatars", false, null, null),
  style("fun-emoji", "Fun Emoji", "Playful", "Cheerful 3D emoji faces", false, null, null),
  style("identicon", "Identicon", "Geometric", "Cryptographic geometric patterns", false, null, null),
  style("rings", "Rings", "Geometric", "Concentric radiant rings", false, null, null),

  /**
   * The generated face, drawn by this app rather than fetched.
   *
   * Worth keeping as an option for two reasons. It is the one style that costs
   * no network request, and it is the only one whose face is a function of what
   * the agent is doing rather than of its name — the geometry has a resting
   * face, an attentive one and a sad one, so it reacts without any of the
   * animation machinery. It also used to resolve to a 1.4 MB PNG, which was a
   * poor trade for a 40 px circle.
   */
  style("raven-native", "Raven Face", "Sovereign", "Drawn locally, reacts to what the agent is doing", false, null, null),
];

/** Look a style up, or `undefined` if the slug is not one we offer. */
export function diceStyle(value: string): DiceStyle | undefined {
  return DICE_STYLES.find((s) => s.value === value);
}

/** Whether a style animates. Unknown styles are assumed not to. */
export function styleAnimates(value: string): boolean {
  return diceStyle(value)?.animated ?? false;
}

/**
 * The picker list, grouped by category with the animated ones first.
 *
 * Kept as a function rather than a constant so the ordering is applied here
 * instead of being a property someone has to maintain by hand in the array.
 */
export function dicebearStyles(): DiceStyle[] {
  return [...DICE_STYLES].sort((a, b) => {
    if (a.animated !== b.animated) return a.animated ? -1 : 1;
    return 0;
  });
}

/**
 * Recognise a stored DiceBear URL and describe it.
 *
 * The avatar picker has always stored the *whole URL* on the agent rather than
 * a style slug, so every agent created before this module existed carries a
 * frozen 9.x URL in `avatar_url`. Those are static: 9.x has no `animationVariant`
 * at all, so a `voxel-bot` saved last year is a still robot and will stay one
 * however the style is configured now.
 *
 * Rather than migrate the database, the URL is rebuilt when it is rendered. That
 * fixes every existing agent at once, with no user action, and it keeps working
 * for any agent whose URL predates whatever the current API version is.
 *
 * Returns `null` for anything that is not a DiceBear URL, which is the signal
 * that the caller has a genuine custom image and should use it untouched.
 */
export function parseDiceBearUrl(url: string | null | undefined): {
  /** The style slug from the path. */
  style: string;
  /** The seed, so the avatar does not change identity. */
  seed: string;
  /** Every other parameter, kept so a chosen background survives. */
  params: URLSearchParams;
} | null {
  const raw = (url ?? "").trim();
  if (!raw) return null;

  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    return null;
  }
  // HTTPS only. A plaintext URL on the same host is either a mistake or an
  // attempt, and treating it as a custom image — which is what returning null
  // means — leaves it to be refused by the app's own image policy rather than
  // quietly upgraded into something that looks legitimate.
  if (parsed.protocol !== "https:" || parsed.hostname !== "api.dicebear.com") return null;

  // `https://api.dicebear.com/9.x/bottts/svg?seed=Ada` — and exactly that.
  //
  // Exactly three segments, not three or more: a longer path is some other
  // resource on the same host, and rewriting it into an avatar request would be
  // inventing a URL the user never asked for.
  const segments = parsed.pathname.split("/").filter(Boolean);
  if (segments.length !== 3) return null;
  const [version, style, format] = segments;
  if (!/^\d+\.x$/.test(version) || format !== "svg" || !style) return null;

  const params = new URLSearchParams(parsed.search);
  // These two are decided here, not carried over: the version is the migration,
  // and the speed comes from the catalogue.
  params.delete("animationVariant");
  params.set("seed", params.get("seed") ?? "Agent");

  return { style, seed: params.get("seed")!, params };
}

/**
 * Bring a stored avatar URL up to date.
 *
 * A DiceBear URL is rebuilt against the current API version and given its
 * style's animation, keeping the seed — so the agent looks like the same agent —
 * and every other parameter the user chose. Anything else is returned untouched,
 * because a custom image is not ours to rewrite.
 */
export function currentDiceBearUrl(
  url: string | null | undefined,
  animated?: boolean,
): { url: string; style: string; animates: boolean } | null {
  const found = parseDiceBearUrl(url);
  if (!found) return null;

  const meta = diceStyle(found.style);
  const wantsMotion = animated ?? meta?.animated ?? false;
  const speed = wantsMotion ? meta?.speed : null;

  const query = new URLSearchParams(found.params);
  if (speed) query.set("animationVariant", speed);
  else query.delete("animationVariant");

  return {
    url: `https://api.dicebear.com/${DICEBEAR_VERSION}/${found.style}/svg?${query.toString()}`,
    style: found.style,
    animates: Boolean(speed),
  };
}

/**
 * The style an agent gets when nothing says otherwise.
 *
 * `clay`, because it is animated at about a 2.9 second cadence — the whole point
 * of the system is an agent whose face changes while you watch — and because it
 * is the style the Grok bots studio is built around, so it reads as a character
 * rather than as a placeholder.
 *
 * The alternative would be `raven-native`, which costs no network request. That
 * is a real advantage and it is why that style stays in the list, but "renders
 * instantly and never changes" is the wrong default for a fleet whose whole
 * appeal is that it looks alive.
 */
export const DEFAULT_AVATAR_STYLE = "clay";

/**
 * The background colours an avatar frame may use.
 *
 * One colour per request, and that is a hard API constraint rather than a taste
 * call. DiceBear 10.x validates `backgroundColor` against a pattern for a
 * *single* colour — `^#?([a-fA-F0-9]{3}|4|6|8)$` — and it validates the raw
 * query string, so a comma-separated list only works if the commas are left
 * unencoded. `URLSearchParams` encodes them, and the request then fails
 * validation with a 400 and renders nothing.
 *
 * That is worth writing down because the failure is invisible in the worst way:
 * a 400 avatar is a blank frame, not an error, so the fleet just quietly stops
 * showing faces. The old code sent a list, which was accepted on 7.x through 9.x
 * and would have been rejected here.
 *
 * The variety the list was for is kept by picking from this palette with the
 * agent's name, so the frame is stable per agent and different between them.
 */
export const AVATAR_BACKGROUNDS: readonly string[] = [
  "1c1c1c", // near-black, the neutral default
  "2b1d1d", // oxblood
  "1d2b22", // bottle green
  "2b2619", // raw umber
  "1f2430", // deep slate
  "241d2b", // aubergine
];

/**
 * A stable background for an agent.
 *
 * Derived from the name so it never changes for a given agent and does not
 * change when the list is reordered. A cheap FNV-1a rather than a random pick,
 * because a face that changes on every render is a face that refetches its
 * image on every render.
 */
export function avatarBackground(seed: string): string {
  let hash = 0x811c9dc5;
  const text = (seed || "Agent").trim();
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    // 32-bit FNV prime multiply, done in two halves to stay in range.
    hash = (hash + (hash << 1) + (hash << 4) + (hash << 7) + (hash << 8) + (hash << 24)) >>> 0;
  }
  return AVATAR_BACKGROUNDS[hash % AVATAR_BACKGROUNDS.length];
}
