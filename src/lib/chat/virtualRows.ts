/**
 * Variable-height transcript virtualisation.
 *
 * Messages are not rows of a table: a one-line reply and a 400-line code
 * explanation can live next to each other, and both can change height when an
 * image loads or a reasoning panel opens. A fixed row height would either clip
 * the tall rows or leave holes under the short ones.
 *
 * So the window is computed from measured heights when a row has been seen,
 * and a content-aware estimate for rows that have not. The component measures
 * what it mounts with a ResizeObserver and feeds the measurements back here.
 * The pure half stays separate so the range arithmetic is testable without a
 * layout engine.
 */

export interface VirtualRowSize {
  /** Stable message key. */
  key: string;
  /** Measured height when known, estimate otherwise. Always positive. */
  height: number;
}

export interface VirtualSlice {
  /** First mounted row. */
  start: number;
  /** One past the last mounted row. */
  end: number;
  /** Height of the rows above the mounted range. */
  top: number;
  /** Height of the rows below the mounted range. */
  bottom: number;
  /** Full height of the virtual list. */
  total: number;
}

const EMPTY: VirtualSlice = {
  start: 0,
  end: 0,
  top: 0,
  bottom: 0,
  total: 0,
};

function positive(height: number): number {
  return Number.isFinite(height) && height > 0 ? height : 1;
}

/** Prefix sums, with `prefix[i]` equal to the top of row `i`. */
export function prefixHeights(rows: readonly VirtualRowSize[]): number[] {
  const prefix = new Array<number>(rows.length + 1);
  prefix[0] = 0;
  for (let i = 0; i < rows.length; i += 1) {
    prefix[i + 1] = prefix[i] + positive(rows[i].height);
  }
  return prefix;
}

/** Describe a concrete `[start, end)` range with its spacer heights. */
export function describeSlice(
  rows: readonly VirtualRowSize[],
  start: number,
  end: number,
): VirtualSlice {
  const count = rows.length;
  if (count === 0) return { ...EMPTY };
  const prefix = prefixHeights(rows);
  const safeStart = Math.max(0, Math.min(count, Math.floor(start)));
  const safeEnd = Math.max(safeStart, Math.min(count, Math.floor(end)));
  return {
    start: safeStart,
    end: safeEnd,
    top: prefix[safeStart],
    bottom: prefix[count] - prefix[safeEnd],
    total: prefix[count],
  };
}

/**
 * Mount the rows that intersect the viewport plus `overscan` pixels on each
 * side. `viewportTop` is relative to the top of the virtual list, not the
 * scroll container, so callers must subtract any leading content (the "load
 * earlier" row, for example) before calling.
 */
export function sliceRows(
  rows: readonly VirtualRowSize[],
  viewportTop: number,
  viewportHeight: number,
  overscan = 480,
): VirtualSlice {
  const count = rows.length;
  if (count === 0) return { ...EMPTY };
  const prefix = prefixHeights(rows);
  const total = prefix[count];
  const height = Number.isFinite(viewportHeight)
    ? Math.max(0, viewportHeight)
    : 0;
  const margin = Number.isFinite(overscan) ? Math.max(0, overscan) : 0;
  const top = Math.max(0, Math.min(total, viewportTop));
  const from = Math.max(0, top - margin);
  const to = Math.min(total, top + height + margin);

  let start = 0;
  while (start < count && prefix[start + 1] <= from) start += 1;
  // A viewport scrolled below the last row (possible while estimates settle)
  // still needs a mounted row; the last one is the honest choice.
  if (start >= count) start = count - 1;

  let end = start;
  while (end < count && prefix[end] < to) end += 1;
  if (end <= start) end = Math.min(count, start + 1);

  return {
    start,
    end,
    top: prefix[start],
    bottom: total - prefix[end],
    total,
  };
}

/**
 * Mount the tail that covers at least `viewportHeight + overscan` pixels and
 * never fewer than `minRows`. Used for the first frame and while the reader is
 * pinned to the bottom.
 */
export function tailRows(
  rows: readonly VirtualRowSize[],
  viewportHeight: number,
  minRows = 1,
  overscan = 480,
): VirtualSlice {
  const count = rows.length;
  if (count === 0) return { ...EMPTY };
  const prefix = prefixHeights(rows);
  const budget = Math.max(0, viewportHeight) + Math.max(0, overscan);
  const least = Math.max(1, Math.min(count, Math.floor(minRows) || 1));
  let start = count;
  let covered = 0;
  while (start > 0 && (covered < budget || count - start < least)) {
    start -= 1;
    covered += positive(rows[start].height);
  }
  return {
    start,
    end: count,
    top: prefix[start],
    bottom: 0,
    total: prefix[count],
  };
}

/**
 * A cheap height estimate for a message that has not been mounted yet.
 *
 * It is deliberately rough: a settled row's real height replaces it on first
 * paint, and the anchor correction keeps the viewport still while the estimate
 * is replaced. Counting lines is what keeps a 60-line answer from being
 * treated as a 60px row, which is the jump that makes paging feel broken.
 */
export function estimateMessageHeight(message: any, leadingGap = 0): number {
  const content = message?.content;
  const text =
    typeof content === "string"
      ? content
      : typeof content?.text === "string"
        ? content.text
        : "";
  const lines = text ? text.split("\n").length : 1;
  const codeBlocks = Math.min(8, (text.match(/```/g)?.length ?? 0) / 2);
  const attachments = Array.isArray(message?.attachments)
    ? message.attachments.length
    : 0;
  const media = Math.min(6, attachments);
  const tools = Array.isArray(content?.tools) ? Math.min(8, content.tools.length) : 0;
  const body = Math.min(2_400, lines * 20 + codeBlocks * 140 + tools * 24);
  const height = 64 + body + media * 180 + Math.max(0, leadingGap);
  return Math.min(3_200, Math.max(64, height));
}
