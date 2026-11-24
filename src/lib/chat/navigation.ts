/**
 * Keyboard message navigation.
 *
 * `Alt+↑` / `Alt+↓` move the transcript one message at a time. The index rule
 * is pure and tested; the DOM half only finds the rows that are actually on
 * screen and asks the browser to center the chosen one.
 */

/** Next row index, clamped at the ends rather than wrapping. */
export function adjacentMessageIndex(
  current: number,
  total: number,
  direction: 1 | -1,
): number {
  if (total <= 0) return -1;
  if (current < 0) return direction > 0 ? 0 : total - 1;
  return Math.max(0, Math.min(total - 1, current + direction));
}

/**
 * Center the previous or next message row in `container`.
 *
 * Rows without an id are skipped: the live streaming row has no persisted
 * message to navigate to. `current` is the first row whose bottom is still
 * below the top of the scrollport, so pressing down moves forward from what
 * the reader can actually see rather than from the top of the feed.
 */
export function navigateMessage(
  container: HTMLElement | null,
  direction: 1 | -1,
  smooth: boolean,
  onBoundary?: (direction: 1 | -1, row: HTMLElement | null) => void,
): HTMLElement | null {
  if (!container) return null;
  const rows = Array.from(
    container.querySelectorAll<HTMLElement>(".message-entry[id]"),
  );
  if (rows.length === 0) return null;

  const top = container.getBoundingClientRect().top;
  let current = rows.findIndex(
    (row) => row.getBoundingClientRect().bottom > top + 8,
  );
  if (current < 0) current = rows.length - 1;

  const target = adjacentMessageIndex(current, rows.length, direction);
  if (target === current) {
    // A virtual transcript only mounts a window of rows. At the window edge
    // there is no next DOM row to scroll to, so hand the direction back to the
    // view, which can mount the adjacent message and reveal it.
    onBoundary?.(direction, rows[current] ?? null);
    return null;
  }
  const row = rows[target];
  row?.scrollIntoView({
    block: "center",
    behavior: smooth ? "smooth" : "auto",
  });
  return row ?? null;
}

/** True when the key event came from a field that owns its own arrow keys. */
export function isEditableTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    Boolean(target.closest("input, textarea, select, [contenteditable='true']"))
  );
}
