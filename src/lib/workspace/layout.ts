//! Layout decisions that depend on the width the conversation actually has.
//!
//! The browser dock is `clamp(460px, 46vw, 820px)` in CSS. A viewport-only
//! breakpoint ignored the sidebar, so at 1280px the dock could still leave the
//! conversation below its 424px minimum. Keep the arithmetic in one place and
//! test the cases that used to collapse.

export const BROWSER_COLUMN_MIN_WINDOW = 1120;
export const BROWSER_COLUMN_MIN_CONVERSATION = 424;
export const BROWSER_COLUMN_MIN_DOCK = 460;
export const BROWSER_COLUMN_MAX_DOCK = 820;
export const WORKSPACE_RAIL_WIDTH = 64;

/** The CSS `clamp(460px, 46vw, 820px)` dock width for a window. */
export function browserDockWidth(windowWidth: number): number {
  return Math.min(
    BROWSER_COLUMN_MAX_DOCK,
    Math.max(BROWSER_COLUMN_MIN_DOCK, windowWidth * 0.46),
  );
}

/**
 * True when the dock must overlay the conversation instead of taking a grid
 * column. `sidebarWidth` is the effective width (0 when the sidebar is hidden).
 */
export function shouldOverlayBrowser(
  windowWidth: number,
  sidebarWidth: number,
  railWidth = WORKSPACE_RAIL_WIDTH,
): boolean {
  if (windowWidth < BROWSER_COLUMN_MIN_WINDOW) return true;
  const conversation =
    windowWidth - railWidth - sidebarWidth - browserDockWidth(windowWidth);
  return conversation < BROWSER_COLUMN_MIN_CONVERSATION;
}
