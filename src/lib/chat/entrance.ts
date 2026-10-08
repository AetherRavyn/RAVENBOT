// OpenBot message entrance: only rows appended LIVE fade+rise in; bulk history
// loads stay frozen. CSS mount animations can't express that (removing the
// suppressor class after a load re-triggers every row), so this WAAPI action
// runs exactly once per row — on mount — and checks whether it mounted inside
// a container flagged .entries-static. Reduced motion skips it entirely.
import { prefersReducedMotion } from "$lib/a11y";

export function entrance(node: HTMLElement) {
  if (prefersReducedMotion()) return;
  if (node.closest(".entries-static")) return;
  node.animate(
    [{ opacity: 0, transform: "translateY(4px)" }, { opacity: 1, transform: "translateY(0)" }],
    { duration: 160, easing: "cubic-bezier(0.23, 1, 0.32, 1)" },
  );
}
