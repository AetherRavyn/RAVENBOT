export interface RailEntry {
  id: string;
  kind: "user" | "agent";
  label: string;
}

/** Clamp a scroll offset to a 0–1 fraction of the scrollable height. */
export function fraction(offset: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) return 0;
  const value = offset / total;
  return Math.min(1, Math.max(0, Number.isFinite(value) ? value : 0));
}

/** The viewport band on the rail, as fractions of the transcript. */
export function viewportBand(
  scrollTop: number,
  clientHeight: number,
  scrollHeight: number,
): { top: number; height: number } {
  return {
    top: fraction(scrollTop, scrollHeight),
    height: fraction(clientHeight, scrollHeight),
  };
}
