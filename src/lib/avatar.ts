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

/**
 * The roster moods — what a user reads off the fleet list.
 *
 * These are Grok Bot's six, which is a better list than the one this started
 * with because it is the vocabulary a roster actually needs: every state is
 * something a person can *do something about*. `working` is happening on its
 * own, `blocked` needs you, `done` is finished, `thinking` is a longer run.
 * A vocabulary that cannot be acted on is decoration.
 *
 *   Grok        here        why
 *   Idle        idle        nothing is happening
 *   Working     working     work is happening now
 *   Waiting     waiting     it needs a decision from you
 *   Blocked     failed      it stopped and could not continue
 *   Thinking    working     a longer run, distinguished by its motion
 *   Done        responded   it finished and nobody has read it
 *
 * `thinking` is folded into `working` because a roster that distinguishes
 * "working" from "thinking" for an agent the user cannot influence is three
 * pixels of difference. It is kept as a *motion* below, which is where the
 * distinction earns its keep: a long run should not look like a quick one.
 *
 * `sleeping` has no Grok equivalent because a sleeping agent is not a state a
 * roster is in — it is what an avatar does when nothing is happening.
 */
export type AvatarMood =
  | "idle"
  | "working"
  | "waiting"
  | "failed"
  | "responded"
  | "sleeping";

/**
 * The motion the engine plays *over* a face.
 *
 * This axis is the fix for a real bug, and it is Grok Bot's model.
 *
 * `faceFor` used to return the mood's expression, so an agent's face was
 * different while it worked than while it sat idle. That reads as two different
 * agents: you cannot recognise the one you were watching a moment ago, and the
 * whole point of a fleet is recognising your agents. So the axes are separated —
 * **the expression is identity, derived from the name and fixed; the motion
 * state is what the mood is doing, played over the top.**
 *
 * The states are named as Grok names them, so the vocabulary is the one people
 * have already seen. Not all fifteen are reachable: a silhouette this simple
 * cannot carry "swirl" or "comet" honestly, and a motion that looks like nothing
 * is worse than no motion. `unreachable` below records which, so the omission is
 * a decision rather than an oversight.
 */
export type MotionState =
  | "idle"
  | "thinking"
  | "orbit"
  | "alert"
  | "notification"
  | "exclamation"
  | "sleep"
  | "burst";

/**
 * The states Grok defines that this renderer deliberately does not use.
 *
 * `swirl`, `play`, `egg`, `hexagon`, `comet`, `wink` and `wideEyes` each need
 * either a second element or a shape change to read at all. A 24-pixel avatar
 * in a sidebar has no room for that, and an animation nobody can see is worse
 * than a still frame because it costs a repaint.
 */
export const UNREACHABLE_MOTIONS = [
  "swirl",
  "play",
  "egg",
  "hexagon",
  "comet",
  "wink",
  "wideEyes",
] as const;

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
  /** The motion state the engine plays. Grok's vocabulary. */
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
  working: { state: "thinking", rings: true, breathe: 0.03, busy: true },
  // The one override. An agent waiting on you has to read as *paying attention*
  // — otherwise it is indistinguishable from one that is idle and simply has not
  // got round to you, which is the exact confusion this state exists to end.
  waiting: { state: "notification", rings: false, breathe: 0, busy: false, expression: "attentive" },
  failed: { state: "exclamation", rings: false, breathe: 0, busy: false },
  responded: { state: "burst", rings: false, breathe: 0, busy: false },
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
