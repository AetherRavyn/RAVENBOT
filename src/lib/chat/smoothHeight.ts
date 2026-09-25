// OpenBot createSmoothHeightResize equivalent: while streaming, bubble content
// grows one word at a time and each growth jolts the layout. This animates the
// CONTAINER's height between old and new content height with the Web Animations
// API, reading the currently-animating height as the next start so interrupted
// tweens never snap. Reduced motion and first measurement are skipped.
import { prefersReducedMotion } from "$lib/a11y";

const RESIZE_MS = 240;
const RESIZE_EASE = "cubic-bezier(0.23, 1, 0.32, 1)";

export function smoothHeight(node: HTMLElement) {
  let animation: Animation | undefined;
  let previous: number | undefined;

  const content = node.firstElementChild instanceof HTMLElement ? node.firstElementChild : node;

  const observer = new ResizeObserver(() => {
    const next = content.getBoundingClientRect().height;
    const prev = previous;
    previous = next;
    if (prev === undefined || prev === next || prefersReducedMotion()) return;

    const currentHeight = Number.parseFloat(getComputedStyle(node).height);
    const start = animation && Number.isFinite(currentHeight) ? currentHeight : prev;
    animation?.cancel();
    node.dataset.resizing = "true";
    const running = node.animate([{ height: `${start}px` }, { height: `${next}px` }], {
      duration: RESIZE_MS,
      easing: RESIZE_EASE,
    });
    animation = running;
    running.finished.then(() => {
      if (animation === running) {
        animation = undefined;
        delete node.dataset.resizing;
      }
    }).catch(() => undefined);
  });
  observer.observe(content);

  return {
    destroy() {
      observer.disconnect();
      animation?.cancel();
    },
  };
}
