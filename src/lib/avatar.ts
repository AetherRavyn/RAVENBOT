/**
 * Raven avatar: a deterministic SVG face that changes with what the agent is
 * doing.
 *
 * Three traits come from the agent's name, and they never change:
 *
 *  - **silhouette** — the body shape. This is identity, and it is the reason
 *    only silhouette-preserving states may play: an agent is recognised by
 *    its outline, so an animation that swaps the outline hands back a
 *    different creature for the duration and the user loses track of who they
 *    are looking at.
 *  - **hue** — the colour, stable per agent.
 *  - **resting expression** — the face the agent wears when nothing is
 *    happening.
 *
 * What *does* change is the mood, and only the expression. A mood maps to a
 * presentation: which expression to wear, whether to breathe, whether to draw
 * orbit rings. The precedence runs from the state that needs a person soonest
 * to the one that needs them least — failure beats a pending question, which
 * beats running work, which beats a reply nobody has read. Reading it the other
 * way round would hide a failure behind the turn that produced it.
 *
 * Replacing DiceBear with a generated SVG is what makes this possible at all:
 * a remote URL is a fixed image, so it cannot react to anything without a round
 * trip per mood change.
 *
 * Deterministic hashing is FNV-1a rather than the byte-sum the backend used, so
 * the traits are actually well distributed instead of clustering.
 */

/** The moods an agent's face can express. */
export type AvatarMood =
  | "idle"
  | "working"
  | "waiting"
  | "failed"
  | "responded"
  | "sleeping";

/** Expression shapes, drawn as paths on the face. */
export type Expression = "neutral" | "attentive" | "pleased" | "sad" | "curious" | "sleepy";

/** Body silhouettes. These are the agent's identity and must not animate. */
export const SILHOUETTES = ["round", "hex", "droplet", "capsule", "shield", "crystal"] as const;
export type Silhouette = (typeof SILHOUETTES)[number];

/**
 * FNV-1a, 32-bit.
 *
 * The previous hash was `bytes.fold(0, |a, b| a + b * 31)` on the bot name,
 * which barely mixes: anagrams collide, and a one-character difference in a
 * long name often gives the same bucket, so several agents in an office
 * ended up with the same face.
 */
export function stableIndex(seed: string, buckets: number): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < seed.length; i += 1) {
    hash ^= seed.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0) % buckets;
}

/** Hues in degrees. Evenly spaced so neighbouring agents never match. */
const HUES = [8, 32, 48, 96, 150, 186, 212, 244, 276, 318] as const;

export interface AvatarProfile {
  silhouette: Silhouette;
  hue: number;
  resting: Expression;
}

/** The stable traits for an agent. Same name always gives the same face. */
export function avatarProfile(seed: string): AvatarProfile {
  return {
    silhouette: SILHOUETTES[stableIndex(`${seed}:silhouette`, SILHOUETTES.length)],
    hue: HUES[stableIndex(`${seed}:hue`, HUES.length)],
    resting: EXPRESSIONS[stableIndex(`${seed}:resting`, EXPRESSIONS.length)],
  };
}

const EXPRESSIONS: Expression[] = ["neutral", "attentive", "pleased", "curious"];

/**
 * How a mood reads on the face.
 *
 * `breathe` is a scale amplitude as a fraction of the avatar's size, applied to
 * the whole rendered avatar so it can never edit the silhouette — 0 holds still.
 */
export interface MoodPresentation {
  expression: Expression;
  /** Orbit rings, drawn around the body rather than in place of it. */
  rings: boolean;
  breathe: number;
  /** Whether this mood needs an animation clock rather than a held pose. */
  busy: boolean;
}

const MOODS: Readonly<Record<AvatarMood, MoodPresentation>> = {
  idle: { expression: "neutral", rings: false, breathe: 0, busy: false },
  // Working looks attentive rather than decorated: the agent is paying
  // attention, which is what the face should say.
  working: { expression: "attentive", rings: true, breathe: 0.03, busy: true },
  // A pending question is curious — it is waiting for an answer.
  waiting: { expression: "curious", rings: false, breathe: 0, busy: false },
  failed: { expression: "sad", rings: false, breathe: 0, busy: false },
  responded: { expression: "pleased", rings: false, breathe: 0, busy: false },
  sleeping: { expression: "sleepy", rings: false, breathe: 0.02, busy: true },
};

export function moodPresentation(mood: AvatarMood): MoodPresentation {
  return MOODS[mood] ?? MOODS.idle;
}

/** The expression to draw: the mood's, or the agent's resting face when idle. */
export function faceFor(profile: AvatarProfile, mood: AvatarMood): Expression {
  if (mood === "idle") return profile.resting;
  return moodPresentation(mood).expression;
}

/**
 * Derive every agent's mood from what is happening to it.
 *
 * Precedence, most urgent first:
 *
 *   failed  >  waiting  >  working  >  responded
 *
 * A failed run has already stopped, so it outranks a question still waiting to
 * be asked, which outranks work running on its own, which outranks a reply
 * nobody has read yet. A failure hidden behind the turn that produced it is the
 * one case where a user would trust a status indicator that is lying.
 *
 * An agent with nothing to report gets no entry, so the result is sparse and a
 * missing key means idle rather than unknown.
 */
export function computeMoods(input: {
  agentIds: readonly string[];
  running: ReadonlySet<string>;
  failed: ReadonlySet<string>;
  needsYou: ReadonlySet<string>;
  replied: ReadonlySet<string>;
  sleeping?: ReadonlySet<string>;
}): Record<string, AvatarMood> {
  const out: Record<string, AvatarMood> = {};
  for (const id of input.agentIds) {
    if (input.sleeping?.has(id)) out[id] = "sleeping";
    else if (input.failed.has(id)) out[id] = "failed";
    else if (input.needsYou.has(id)) out[id] = "waiting";
    else if (input.running.has(id)) out[id] = "working";
    else if (input.replied.has(id)) out[id] = "responded";
  }
  return out;
}

/** The `--accent` and `--accent-dim` a hue produces, as HSL parts. */
export function hueVars(hue: number): { accent: string; dim: string; deep: string } {
  return {
    accent: `${hue} 72% 62%`,
    dim: `${hue} 40% 44%`,
    deep: `${hue} 55% 28%`,
  };
}

/** Initials for the fallback when SVG is not wanted. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}
