/**
 * Raven avatar: a deterministic SVG face that changes with what the agent is
 * doing, built to Grok Bot's avatar specification.
 *
 * Three traits come from the agent's name, and they never change:
 *
 *  - **shape** — the body outline. This is identity, and it is the reason only
 *    silhouette-preserving states may play on the roster: an agent is
 *    recognised by its outline, so an animation that swaps the outline hands
 *    back a different creature for the duration and the user loses track of
 *    who they are looking at.
 *  - **colour** — one of twelve, stable per agent.
 *  - **resting expression** — the face the agent wears when nothing is
 *    happening.
 *
 * What *does* change is the mood, and only the animation state. A mood maps to
 * a presentation: which expression to wear, which motion to play, whether to
 * breathe, whether to draw orbit rings. The precedence runs from the state
 * that needs a person soonest to the one that needs them least — failure beats
 * a pending question, which beats running work, which beats a reply nobody has
 * read. Reading it the other way round would hide a failure behind the turn
 * that produced it.
 *
 * ## Why Grok's specification and not an invented one
 *
 * The two axes are separated in Grok's design and that separation is the whole
 * trick: **the expression is identity and the animation state is what the
 * engine plays over it.** An earlier version of this file returned the mood's
 * expression from `faceFor`, which meant an agent's face was *different* while
 * it worked than while it sat idle. That reads as two different agents — you
 * cannot recognise the one you were watching a moment ago, and recognising
 * your agents is the entire point of a fleet. So `faceFor` now returns the
 * agent's own resting face for every mood except `waiting`, which earns an
 * override because an agent blocked on you must read as paying attention.
 *
 * ## The full set
 *
 * 8 shapes, 16 expressions, 12 colours and 15 animation states, matching
 * Grok Bot. Everything is inline SVG rather than an `<img src>`: no network
 * request, renders at any size, inherits the theme's tokens, and animates. A
 * remote avatar URL is a fixed bitmap, which is why it could not react to
 * anything.
 *
 * Deterministic hashing is FNV-1a rather than the byte-sum the backend used, so
 * the traits are actually well distributed instead of clustering.
 */

/**
 * The roster moods — what a user reads off the fleet list.
 *
 * These are Grok Bot's six, and `sleeping` is the one addition, because a
 * sleeping agent is not a state a roster is in — it is what an avatar does when
 * nothing is happening.
 *
 *   Grok        here        why
 *   Idle        idle        nothing is happening
 *   Working     working     work is happening now
 *   Waiting     waiting     it needs a decision from you
 *   Blocked     failed      it stopped and could not continue
 *   Thinking    thinking    a longer run, distinguished by its motion
 *   Done        responded   it finished and nobody has read it
 *   —           sleeping    no activity, and it should look asleep
 *
 * `thinking` was previously folded into `working` on the argument that a user
 * cannot influence the difference and three pixels is not worth a state. That
 * was the right call when the only difference was a slightly different face.
 * Grok gives `Thinking` its own motion — Orbit, rings that keep turning for a
 * long run — and *that* earns the state, because "leave this one alone, it is
 * working on something big" is a different instruction to a user than "this one
 * is about to answer".
 */
export type AvatarMood =
  | "idle"
  | "working"
  | "waiting"
  | "failed"
  | "responded"
  | "thinking"
  | "sleeping";

/**
 * The animation state the engine plays *over* a face. All fifteen of Grok's.
 *
 * `changesBody` marks the three that replace the outline. They are honest in
 * the studio, where you are deliberately previewing artwork, and wrong on a
 * roster, where an agent that becomes an egg for two seconds is an agent you
 * cannot recognise. `ANIMATIONS` lists them all; `rosterSafeMotion` refuses
 * the three.
 */
export type MotionState =
  | "idle"
  | "thinking"
  | "wink"
  | "wideEyes"
  | "alert"
  | "notification"
  | "exclamation"
  | "sleep"
  | "egg"
  | "hexagon"
  | "play"
  | "orbit"
  | "swirl"
  | "burst"
  | "comet";

/**
 * The fifteen animation states, in Grok's order, with the notes a studio
 * preview needs.
 *
 * `changesBody` is the roster-safety flag described on `MotionState`.
 */
export const ANIMATIONS: ReadonlyArray<{
  id: MotionState;
  /** Why a reader would want to tell this state from another. */
  note: string;
  changesBody: boolean;
}> = [
  { id: "idle", note: "Nothing needs you.", changesBody: false },
  { id: "thinking", note: "Work is happening. Three dots.", changesBody: false },
  { id: "wink", note: "A quick one-eye acknowledgement.", changesBody: false },
  { id: "wideEyes", note: "Eyes open wide, holding attention.", changesBody: false },
  { id: "alert", note: "Something needs a second look.", changesBody: false },
  { id: "notification", note: "It needs you. A blue dot appears.", changesBody: false },
  { id: "exclamation", note: "It stopped and could not continue.", changesBody: false },
  { id: "sleep", note: "Asleep. Nothing happening.", changesBody: false },
  { id: "egg", note: "The body settles into an egg.", changesBody: true },
  { id: "hexagon", note: "The body settles into a hexagon.", changesBody: true },
  { id: "play", note: "The body settles into a play mark.", changesBody: true },
  { id: "orbit", note: "Longer-running work. Rings spinning; leave it be.", changesBody: false },
  { id: "swirl", note: "Energy circling the body.", changesBody: false },
  { id: "burst", note: "It just finished something.", changesBody: false },
  { id: "comet", note: "Moving fast, tail behind it.", changesBody: false },
];

/**
 * The states Grok defines that a roster must never play.
 *
 * Kept as an export rather than deleted because it is a real constraint and
 * the test suite asserts against it: these three replace the outline, which is
 * identity. They remain fully available in the studio.
 */
export const UNREACHABLE_MOTIONS = ["egg", "hexagon", "play"] as const;

/** The eight body shapes. Grok's set. These are identity and must not animate. */
export const SHAPES = [
  "circle",
  "pebble",
  "squircle",
  "capsule",
  "triangle",
  "hexagon",
  "cloud",
  "droplet",
] as const;
export type Shape = (typeof SHAPES)[number];

/**
 * The old shape ids, and where each one goes.
 *
 * Shape is stored on the agent, so renaming the list without a map would hand
 * every existing agent a fallback face the first time the app loaded. The map
 * is six-to-eight and injective, so no two legacy shapes collide on the way
 * across.
 */
const LEGACY_SHAPES: Readonly<Record<string, Shape>> = {
  round: "circle",
  hex: "hexagon",
  shield: "pebble",
  crystal: "squircle",
  droplet: "droplet",
  capsule: "capsule",
};

/** Map any historical or unknown shape id onto the current eight. */
export function normaliseShape(id: string | null | undefined): Shape {
  if (!id) return "circle";
  if ((SHAPES as readonly string[]).includes(id)) return id as Shape;
  return LEGACY_SHAPES[id] ?? "circle";
}

/** Map any historical or unknown state id onto the current fifteen. */
export function normaliseState(id: string | null | undefined): MotionState {
  // Anything at all can arrive here — a stored record is a string, but a prop
  // from a component is not, and a normaliser that throws on a number is a
  // blank screen rather than a default face.
  if (typeof id !== "string" || id.trim() === "") return "idle";
  const camel = id.trim().replace(/[-_\s]+(.)?/g, (_, c: string | undefined) =>
    c ? c.toUpperCase() : "",
  );
  const ids = ANIMATIONS.map((a) => a.id);
  if (ids.includes(camel as MotionState)) return camel as MotionState;
  // Accept the spaced forms the docs use ("Wide eyes", "Long eyes").
  const loose = ANIMATIONS.find((a) => a.id.toLowerCase() === id.trim().toLowerCase().replace(/\s+/g, ""));
  return loose?.id ?? "idle";
}

/** The sixteen expressions. Grok's set. */
export const EXPRESSIONS = [
  "neutral",
  "attentive",
  "surprised",
  "excited",
  "happy",
  "laughing",
  "angry",
  "sad",
  "scared",
  "suspicious",
  "confused",
  "curious",
  "proud",
  "shy",
  "unimpressed",
  "sleepy",
] as const;
export type Expression = (typeof EXPRESSIONS)[number];

/** The one renamed expression, and its old id. */
const LEGACY_EXPRESSIONS: Readonly<Record<string, Expression>> = { pleased: "happy" };

/** Map any historical or unknown expression id onto the current sixteen. */
export function normaliseExpression(id: string | null | undefined): Expression {
  if (!id) return "neutral";
  if ((EXPRESSIONS as readonly string[]).includes(id)) return id as Expression;
  return LEGACY_EXPRESSIONS[id] ?? "neutral";
}

/**
 * Back-compat alias.
 *
 * The shape list used to be called silhouettes, and components and tests
 * import `SILHOUETTES`. The word "silhouette" is still the more accurate one
 * for the *identity* concept — it is the outline, not the whole shape set — so
 * the alias stays rather than rippling a rename through the codebase for
 * nothing.
 */
export const SILHOUETTES = SHAPES;
export type Silhouette = Shape;

/** The twelve colours, with the exact hexes from Grok's palette. */
export const COLOURS = [
  { id: "ink", hex: "#0a0a0c" },
  { id: "brown", hex: "#8b5e3c" },
  { id: "red", hex: "#e8483f" },
  { id: "orange", hex: "#f08a24" },
  { id: "amber", hex: "#f0b429" },
  { id: "green", hex: "#3ecf8e" },
  { id: "turquoise", hex: "#2fbfa0" },
  { id: "blue", hex: "#3b93f0" },
  { id: "purple", hex: "#8b5cf6" },
  { id: "pink", hex: "#e152b0" },
  { id: "grey", hex: "#a3a3a3" },
  { id: "cream", hex: "#f1efe9" },
] as const;
export type ColourId = (typeof COLOURS)[number]["id"];

const COLOUR_BY_ID: Readonly<Record<string, (typeof COLOURS)[number]>> = Object.fromEntries(
  COLOURS.map((c) => [c.id, c]),
);

/** Map any historical or unknown colour id onto the current twelve. */
export function normaliseColour(id: string | null | undefined): ColourId {
  if (!id) return "blue";
  return id in COLOUR_BY_ID ? (id as ColourId) : "blue";
}

/** The hex for a colour id. */
export function colourHex(id: ColourId | string | null | undefined): string {
  return COLOUR_BY_ID[normaliseColour(id)].hex;
}

/**
 * FNV-1a, 32-bit.
 *
 * The previous hash was `bytes.fold(0, |a, b| a + b * 31)` on the bot name,
 * which barely mixes: anagrams collide, and a one-character difference in a
 * long name often gives the same bucket, so several agents in an office ended
 * up with the same face.
 */
export function stableIndex(seed: string, buckets: number): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < seed.length; i += 1) {
    hash ^= seed.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0) % buckets;
}

export interface AvatarProfile {
  /** The body outline. Identity. */
  shape: Shape;
  /** Legacy alias for `shape`, kept so stored records and old props still read. */
  silhouette: Shape;
  /** One of the twelve palette entries. */
  colour: ColourId;
  /** The hex behind `colour`. */
  hex: string;
  /**
   * The colour as an HSL hue in degrees.
   *
   * Kept because a good deal of the app, and several tests, still think in hue.
   * It is derived from `hex` rather than chosen independently, so there is one
   * source of truth for an agent's colour.
   */
  hue: number;
  /** The face the agent wears when nothing is happening. */
  resting: Expression;
}

/** The stable traits for an agent. Same name always gives the same face. */
export function avatarProfile(seed: string): AvatarProfile {
  const shape = SHAPES[stableIndex(`${seed}:shape`, SHAPES.length)];
  const colour = COLOURS[stableIndex(`${seed}:colour`, COLOURS.length)];
  return {
    shape,
    silhouette: shape,
    colour: colour.id,
    hex: colour.hex,
    hue: Math.round(hexToHsl(colour.hex).h),
    resting: EXPRESSIONS[stableIndex(`${seed}:resting`, EXPRESSIONS.length)],
  };
}

/**
 * How a mood reads on the face.
 *
 * `breathe` is a scale amplitude as a fraction of the avatar's size, applied to
 * the whole rendered avatar so it can never edit the outline — 0 holds still.
 */
export interface MoodPresentation {
  /** The animation state the engine plays. Grok's vocabulary. */
  state: MotionState;
  /** Orbit rings, drawn around the body rather than in place of it. */
  rings: boolean;
  breathe: number;
  /** Whether this mood needs an animation clock rather than a held pose. */
  busy: boolean;
  /**
   * The one thing a mood is allowed to change about the face.
   *
   * Almost nothing, and that is the point. The face is identity; a mood that
   * rewrites it turns a roster into a strobe. Exactly one override earns its
   * place — see `MOODS` below for the argument on each.
   */
  expression?: Expression;
}

const MOODS: Readonly<Record<AvatarMood, MoodPresentation>> = {
  // Idle is the resting pose: the agent's own face, not moving.
  idle: { state: "idle", rings: false, breathe: 0, busy: false },
  // Rings, because "work is happening and you do not need to look" is exactly
  // what a ring means, and because it keeps the face available to be recognised.
  working: { state: "thinking", rings: false, breathe: 0.03, busy: true },
  // Orbit is Grok's long-run motion: rings that keep turning, and the user is
  // told to leave it alone. That is a genuinely different instruction from
  // `working`, which is why `thinking` is its own mood now.
  thinking: { state: "orbit", rings: true, breathe: 0.02, busy: true },
  // The one override. An agent waiting on you has to read as *paying attention*
  // — otherwise it is indistinguishable from one that is idle and simply has not
  // got round to you, which is the exact confusion this state exists to end.
  waiting: { state: "notification", rings: false, breathe: 0, busy: true, expression: "attentive" },
  failed: { state: "exclamation", rings: false, breathe: 0, busy: true },
  responded: { state: "burst", rings: false, breathe: 0, busy: true },
  sleeping: { state: "sleep", rings: false, breathe: 0.02, busy: true },
};

export function moodPresentation(mood: AvatarMood): MoodPresentation {
  return MOODS[mood] ?? MOODS.idle;
}

/**
 * The expression to draw.
 *
 * The agent's **own**, for every mood except the one that earns an override. A
 * resting face chosen from the name is what makes an agent recognisable across
 * states; a mood that rewrote it would mean watching an agent work makes it a
 * different creature, and you cannot point at a colleague and say "that one".
 *
 * `waiting` is the exception and the reasoning is above in `MOODS`.
 */
export function faceFor(profile: AvatarProfile, mood: AvatarMood): Expression {
  return moodPresentation(mood).expression ?? profile.resting;
}

/**
 * The motion state for a mood.
 *
 * Separate from `faceFor` on purpose. They answer different questions — "which
 * creature is this" and "what is it doing" — and a roster needs both, from the
 * same avatar, at the same time.
 */
export function motionFor(mood: AvatarMood): MotionState {
  return moodPresentation(mood).state;
}

/**
 * Refuse a motion that would replace an agent's outline.
 *
 * A roster and a studio have different rules and this is where they diverge. The
 * studio exists to preview artwork, so `egg`, `hexagon` and `play` are honest
 * there. On a roster they are a bug: for the length of the animation the agent
 * is a different creature, and the whole reason an avatar carries a shape is
 * that you can point at it. Those three fall back to their nearest safe motion
 * rather than to `idle`, so a state never silently stops meaning anything.
 */
export function rosterSafeMotion(state: MotionState): MotionState {
  switch (state) {
    case "egg":
    case "hexagon":
      return "thinking";
    case "play":
      return "burst";
    default:
      return state;
  }
}

/**
 * Derive every agent's mood from what is happening to it.
 *
 * Precedence, most urgent first:
 *
 *   failed  >  waiting  >  thinking  >  working  >  responded
 *
 * A failed run has already stopped, so it outranks a question still waiting to
 * be asked, which outranks a long run in progress, which outranks a short one,
 * which outranks a reply nobody has read yet. A failure hidden behind the turn
 * that produced it is the one case where a user would trust a status indicator
 * that is lying.
 *
 * `thinking` is a subset of `running` rather than a separate set: the caller
 * decides which of its running agents are long runs, because only it knows what
 * it asked for. An agent with nothing to report gets no entry, so the result is
 * sparse and a missing key means idle rather than unknown.
 */
export function computeMoods(input: {
  agentIds: readonly string[];
  running: ReadonlySet<string>;
  failed: ReadonlySet<string>;
  needsYou: ReadonlySet<string>;
  replied: ReadonlySet<string>;
  sleeping?: ReadonlySet<string>;
  /** Running agents that are long enough to deserve Orbit rather than dots. */
  thinking?: ReadonlySet<string>;
}): Record<string, AvatarMood> {
  const out: Record<string, AvatarMood> = {};
  for (const id of input.agentIds) {
    if (input.sleeping?.has(id)) out[id] = "sleeping";
    else if (input.failed.has(id)) out[id] = "failed";
    else if (input.needsYou.has(id)) out[id] = "waiting";
    else if (input.thinking?.has(id)) out[id] = "thinking";
    else if (input.running.has(id)) out[id] = "working";
    else if (input.replied.has(id)) out[id] = "responded";
  }
  return out;
}

/* ── colour maths ─────────────────────────────────────────────────────────── */

function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const h = hex.replace("#", "");
  const full =
    h.length === 3
      ? h
          .split("")
          .map((c) => c + c)
          .join("")
      : h;
  const n = parseInt(full.slice(0, 6), 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

function hexToHsl(hex: string): { h: number; s: number; l: number } {
  const { r, g, b } = hexToRgb(hex);
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return { h: 0, s: 0, l };
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === rn) h = ((gn - bn) / d + (gn < bn ? 6 : 0)) * 60;
  else if (max === gn) h = ((bn - rn) / d + 2) * 60;
  else h = ((rn - gn) / d + 4) * 60;
  return { h, s: s * 100, l: l * 100 };
}

/** Blend a hex towards white (`amount` > 0) or black (`amount` < 0). */
export function shade(hex: string, amount: number): string {
  const target = amount >= 0 ? 255 : 0;
  const t = Math.abs(amount);
  const { r, g, b } = hexToRgb(hex);
  const mix = (c: number) => Math.round(c + (target - c) * t);
  return `#${[mix(r), mix(g), mix(b)]
    .map((c) => c.toString(16).padStart(2, "0"))
    .join("")}`;
}

/**
 * The `--accent`, `--dim` and `--deep` a colour produces, as HSL parts.
 *
 * Kept for the places that still work in hue. New code should use
 * `colourVars`, which keeps the exact hex rather than re-deriving an
 * approximation from a hue.
 */
export function hueVars(hue: number): { accent: string; dim: string; deep: string } {
  return {
    accent: `${hue} 72% 62%`,
    dim: `${hue} 40% 44%`,
    deep: `${hue} 55% 28%`,
  };
}

export interface ColourVars {
  /** The body fill, exactly as specified. */
  base: string;
  /** A darker fill for the lower half of the body. */
  deep: string;
  /** A lighter rim for the stroke and the gloss. */
  glow: string;
  /**
   * What the face is drawn in.
   *
   * Ink and cream are near-black and near-white, and a face drawn in a fixed
   * ink tone disappears on one and shouts on the other. So this is computed
   * from the body's own luminance, which is the whole reason the palette
   * carries an almost-black and an almost-white in it.
   */
  face: string;
}

const colourVarCache = new Map<ColourId, ColourVars>();

/** The four tokens a colour produces, memoised because it never changes. */
export function colourVars(id: ColourId | string | null | undefined): ColourVars {
  const key = normaliseColour(id);
  const hit = colourVarCache.get(key);
  if (hit) return hit;
  const hex = COLOUR_BY_ID[key].hex;
  const { l } = hexToHsl(hex);
  const vars: ColourVars = {
    base: hex,
    deep: shade(hex, -0.32),
    glow: shade(hex, 0.34),
    face: l > 62 ? "#141414" : l < 22 ? "#f4f4f5" : "#141414",
  };
  colourVarCache.set(key, vars);
  return vars;
}

/** Initials for the fallback when SVG is not wanted. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}