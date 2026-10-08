/**
 * Budgeted word-by-word reveal, decoupled from token arrival.
 *
 * The naive version — reveal one word per tick — falls apart when a provider
 * bursts: a paragraph arrives in one chunk and then takes four seconds to
 * appear, so the reply looks like it is being typed by a very slow machine. It
 * also falls apart the other way, if the model streams faster than the tick
 * rate, because the backlog grows without bound and the text lags further and
 * further behind what has been received.
 *
 * So the step is budgeted. Each tick adds `backlog * stepMs / catchUpMs`
 * characters of allowance, which means the shown text settles roughly
 * `catchUpMs` behind the received text no matter how fast the model writes, and
 * a reply that arrives all at once still shows over about that time. Steps land
 * on word boundaries, so a word never appears half-formed.
 *
 * The steps are also kept as a list rather than collapsed into one string,
 * because each step fades in independently: a step still fading must not jump
 * to full opacity when the next one appears.
 */

/** One step of revealed text: how much it showed, and when. */
export interface RevealChunk {
  length: number;
  revealedAt: number;
}

/**
 * The slowest reveal speed, so the last few words of a long backlog do not
 * crawl at a handful of characters per tick.
 */
const MIN_CHARACTERS_PER_SECOND = 120;

/**
 * One word, with any markdown block prefix it carries and the whitespace around
 * it. Sticky so a step can be resumed at an arbitrary offset.
 */
const WORD = /\s*(?:(?:#{1,6}|[-+*>]|\d+[.)])\s+)?\S+\s+/uy;

/** How far one step advances, and the budget it leaves for the next. */
export function nextReveal(input: {
  shownLength: number;
  target: string;
  streaming: boolean;
  budget: number;
  stepMs: number;
  catchUpMs: number;
}): { length: number; budget: number } {
  const { target, stepMs } = input;
  const backlog = target.length - input.shownLength;
  if (backlog <= 0) return { length: target.length, budget: 0 };

  const minimumStep = (MIN_CHARACTERS_PER_SECOND * stepMs) / 1000;
  let budget = input.budget + (backlog * stepMs) / Math.max(input.catchUpMs, stepMs) + minimumStep;
  let length = input.shownLength;

  while (budget > 0) {
    WORD.lastIndex = length;
    const match = WORD.exec(target);
    if (!match) {
      // Mid-stream the last word can still grow, so wait for the space that
      // will end it rather than flashing a partial word.
      if (input.streaming) return { length, budget: 0 };
      return { length: target.length, budget: 0 };
    }
    length += match[0].length;
    budget -= match[0].length;
  }

  // A long word — a URL, usually — overshoots the budget. Owe only what the
  // next step always adds, so the text after it does not stall while a small
  // backlog repays a debt it can never clear.
  return { length, budget: Math.max(budget, -minimumStep) };
}

/** How many characters the fading steps cover. */
export function trailReach(trail: readonly RevealChunk[]): number {
  let reach = 0;
  for (const chunk of trail) reach += chunk.length;
  return reach;
}

/**
 * The number of body characters that come after each sibling block.
 *
 * Returns `undefined` for a block with more text after it than the trail
 * covers, which means it shows with no fade at all. An unknown source length is
 * `Infinity`: the blocks before it then do not fade, because a too-small offset
 * would make words that are already on screen fade a second time.
 */
export function tailOffsets(
  lengths: readonly number[],
  after: number | undefined,
  reach: number,
): (number | undefined)[] {
  const offsets: (number | undefined)[] = lengths.map(() => undefined);
  if (after === undefined) return offsets;
  let rest = after;
  for (let index = lengths.length - 1; index >= 0 && rest < reach; index -= 1) {
    offsets[index] = rest;
    rest += lengths[index] ?? Number.POSITIVE_INFINITY;
  }
  return offsets;
}

/**
 * The characters after a node's children, given the characters after the node.
 *
 * The closing markup of the node — `**`, `](url)` — sits between them, so this
 * measures the difference rather than assuming the children reach the end.
 */
export function tailInside(
  raw: string,
  children: readonly { raw: string }[],
  after: number | undefined,
): number | undefined {
  if (after === undefined) return undefined;
  const inner = children.map((child) => child.raw).join("");
  const start = raw.indexOf(inner);
  return start < 0 ? after : after + raw.length - start - inner.length;
}

/** Value-compare two offset lists, for use as a memo equality function. */
export function sameTailOffsets(
  previous: readonly (number | undefined)[],
  next: readonly (number | undefined)[],
): boolean {
  return previous.length === next.length && previous.every((offset, i) => offset === next[i]);
}

/**
 * Split the end of some rendered text into the reveal steps still fading.
 *
 * `after` is the number of body characters that come *after* this text, so the
 * newest steps — which are the ones near the end of the body — are matched
 * first. The caller renders `prefix` plainly and gives each chunk its own
 * fading wrapper, tagged with `revealedAt` so a step that is 200 ms old looks
 * different from one revealed this tick.
 */
export function splitTrail(
  text: string,
  trail: readonly RevealChunk[],
  after = 0,
): { prefix: string; chunks: { text: string; revealedAt: number }[] } {
  const chunks: { text: string; revealedAt: number }[] = [];
  let end = text.length;
  let skip = after;

  for (let index = trail.length - 1; index >= 0 && end > 0; index -= 1) {
    const step = trail[index];
    if (!step) continue;
    if (skip >= step.length) {
      skip -= step.length;
      continue;
    }
    let start = Math.max(0, end - step.length + skip);
    skip = 0;

    // Keep a word inside one step, or its halves fade at different speeds.
    // Steps end on word boundaries, so a start landing mid-word comes from
    // markup the rendered text does not show — a hidden `**`, for instance.
    // That step then reaches too far back, so hand the word to the older step
    // rather than making a word that is already visible fade again.
    while (start < end && /\S/.test(text.charAt(start - 1)) && /\S/.test(text.charAt(start))) {
      start += 1;
    }
    if (start >= end) continue;

    chunks.unshift({ text: text.slice(start, end), revealedAt: step.revealedAt });
    end = start;
  }

  return { prefix: text.slice(0, end), chunks };
}

/** How long a word stays fading after it is revealed. */
export const REVEAL_FADE_MS = 220;
