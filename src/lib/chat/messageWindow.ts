/**
 * Bounded transcript windows.
 *
 * A conversation is stored in full but mounted in a tail. The window is
 * described by an end offset so that incoming messages cannot shift rows
 * under a reader who is looking at older history: `pinnedEnd = 0` follows the
 * live tail, while a positive value freezes the window at that message (used
 * by search and reply jumps).
 */
export interface TranscriptWindow {
  /** First mounted message index. */
  start: number;
  /** One past the last mounted message index. */
  end: number;
}

export function transcriptWindow(
  total: number,
  size: number,
  pinnedEnd = 0,
): TranscriptWindow {
  const safeTotal = Math.max(0, total);
  const safeSize = Math.max(0, size);
  const end =
    pinnedEnd === 0
      ? safeTotal
      : Math.min(safeTotal, Math.max(0, pinnedEnd));
  return { start: Math.max(0, end - safeSize), end };
}

/**
 * Keep the reader on the same message when older rows are prepended.
 *
 * Returns the scrollTop that preserves the viewport: the content height added
 * above the reader is added to their current offset. Without this, clicking
 * "Load earlier" moves them to a different part of the conversation.
 */
export function anchoredScrollTop(
  previousTop: number,
  previousHeight: number,
  nextHeight: number,
): number {
  return Math.max(0, previousTop + Math.max(0, nextHeight - previousHeight));
}
