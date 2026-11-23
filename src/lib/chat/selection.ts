/**
 * Turn a text selection into a message quote.
 *
 * The transcript is a list of rows, not one document. A selection that starts
 * in one message and ends in another is not a quote — it is an accident — and
 * so is a selection that drags across a link, button, code copy control, or
 * citation. Both are rejected here rather than in the component so the rule is
 * testable without a browser.
 */

/** The longest passage a quote may carry. A selection larger than this is a copy, not a quote. */
export const MAX_SELECTION_QUOTE = 2_000;

/** A rectangle in viewport coordinates, matching `DOMRect` without depending on it. */
export interface ViewportRect {
  top: number;
  right: number;
  bottom: number;
  left: number;
  width: number;
  height: number;
}

/** A selected passage, and the message row that owns it. */
export interface SelectionQuote {
  messageId: string;
  text: string;
  /** The selection's bounding box, in viewport coordinates. */
  rect: ViewportRect;
}

export interface ToolbarSize {
  width: number;
  height: number;
}

export interface ViewportSize {
  width: number;
  height: number;
}

export interface ToolbarPlacement {
  top: number;
  left: number;
  placement: "top" | "bottom";
}

/** Interactive descendants are controls, not prose. A quote that spans one is not a quote. */
const INTERACTIVE_SELECTOR =
  'a, button, input, textarea, select, [contenteditable="true"], [role="button"]';

const VIEWPORT_MARGIN = 12;
const TOOLBAR_GAP = 8;

/** The nearest message row around a node, or null when the node is outside `root`. */
export function messageRowFor(node: Node | null, root: HTMLElement | null): HTMLElement | null {
  if (!node || !root) return null;
  const element = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  const row = element?.closest<HTMLElement>("[data-message-id]") ?? null;
  if (!row || !root.contains(row)) return null;
  return row;
}

/** Whether the range contains or starts inside an interactive control. */
function crossesInteractiveContent(range: Range, row: HTMLElement): boolean {
  const start = range.startContainer;
  const startElement = start.nodeType === Node.ELEMENT_NODE ? (start as Element) : start.parentElement;
  if (startElement?.closest(INTERACTIVE_SELECTOR)) return true;
  const end = range.endContainer;
  const endElement = end.nodeType === Node.ELEMENT_NODE ? (end as Element) : end.parentElement;
  if (endElement?.closest(INTERACTIVE_SELECTOR)) return true;
  const fragment = range.cloneContents();
  return Boolean(fragment.querySelector(INTERACTIVE_SELECTOR));
}

/** A safe zero rectangle for environments where Range has no layout (jsdom, for one). */
function rangeRect(range: Range): ViewportRect {
  const rect = typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : null;
  if (!rect) return { top: 0, right: 0, bottom: 0, left: 0, width: 0, height: 0 };
  return {
    top: rect.top,
    right: rect.right,
    bottom: rect.bottom,
    left: rect.left,
    width: rect.width,
    height: rect.height,
  };
}

/**
 * Read a passage from the current selection, or null when it is not one quote.
 *
 * A selection is a quote when it is non-empty, stays in one row, does not
 * cross a control, and is short enough to quote. The raw selection text keeps
 * its line breaks; the caller decides how to render them.
 */
export function selectionQuote(
  selection: Selection | null,
  root: HTMLElement | null,
): SelectionQuote | null {
  if (!selection || !root || selection.isCollapsed || selection.rangeCount !== 1) return null;
  const range = selection.getRangeAt(0);
  if (range.collapsed) return null;

  const startRow = messageRowFor(range.startContainer, root);
  const endRow = messageRowFor(range.endContainer, root);
  if (!startRow || startRow !== endRow) return null;
  if (crossesInteractiveContent(range, startRow)) return null;

  const messageId = startRow.getAttribute("data-message-id")?.trim() ?? "";
  if (!messageId) return null;

  const text = selection.toString().replace(/\u00a0/gu, " ").trim();
  if (!text || text.length > MAX_SELECTION_QUOTE) return null;

  return { messageId, text, rect: rangeRect(range) };
}

/**
 * Place the toolbar above the selection, or below it when there is no room.
 *
 * Both axes are clamped to the viewport, so a selection at the top or the edge
 * of a narrow window still gets a reachable toolbar.
 */
export function selectionToolbarPosition(
  rect: ViewportRect,
  toolbar: ToolbarSize,
  viewport: ViewportSize,
): ToolbarPlacement {
  const maximumLeft = Math.max(VIEWPORT_MARGIN, viewport.width - toolbar.width - VIEWPORT_MARGIN);
  const centered = rect.left + (rect.width - toolbar.width) / 2;
  const left = Math.min(Math.max(centered, VIEWPORT_MARGIN), maximumLeft);

  const above = rect.top - TOOLBAR_GAP - toolbar.height;
  const below = rect.bottom + TOOLBAR_GAP;
  const belowFits = below + toolbar.height <= viewport.height - VIEWPORT_MARGIN;
  const aboveFits = above >= VIEWPORT_MARGIN;
  // Prefer above the selection, where a toolbar reads as belonging to it.
  // Flip below only when there is no room above; if neither fits, below wins
  // because the top of the viewport is usually the harder edge to reach.
  const placement = aboveFits || !belowFits ? "top" : "bottom";
  const idealTop = placement === "bottom" ? below : above;
  const maximumTop = Math.max(VIEWPORT_MARGIN, viewport.height - toolbar.height - VIEWPORT_MARGIN);
  const top = Math.min(Math.max(idealTop, VIEWPORT_MARGIN), maximumTop);

  return { top, left, placement };
}

/** Turn a multi-line selection back into a Markdown blockquote. */
export function quoteAsMarkdown(text: string): string {
  const lines = text.replace(/\r\n?/gu, "\n").split("\n");
  return lines.map((line) => `> ${line}`.trimEnd()).join("\n");
}
