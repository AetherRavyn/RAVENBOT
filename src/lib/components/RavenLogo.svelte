<script lang="ts">
  /**
   * The RAVENBOT mark.
   *
   * The eyes follow the pointer, the mark reacts to a click, and it says what
   * the app is doing. Three details that matter more than they look:
   *
   *  - Pointer tracking writes CSS custom properties rather than moving the
   *    elements, so the browser animates the interpolation on the compositor
   *    and the whole thing is two property writes per pointer move.
   *  - Every animation checks `prefers-reduced-motion` first. An eye that
   *    follows the cursor is exactly the kind of thing that makes a motion
   *    preference worth honouring, and this mark is the app's most persistent
   *    element.
   *  - The work state is a data attribute, not a class. Nothing here branches
   *    on it in JS, so a run that starts does not re-render the mark — CSS
   *    picks up the new attribute and starts the right loop.
   */
  import { prefersReducedMotion } from "$lib/a11y";
  import { t } from "$lib/i18n";

  interface Props {
    size?: "sm" | "md" | "lg" | "xl";
    /** Let the eyes follow the pointer. */
    interactive?: boolean;
    /**
     * What the app is doing right now. The mark is the one element that is
     * always on screen, so it is the one place "something is happening" can be
     * stated without a badge, a toast, or a second glance at the sidebar.
     *
     * The three values are `Activity` minus `responded`, deliberately: this is
     * fed straight from `fleetActivity.busiest`, and a prop that renames one of
     * its values is a prop whose owner has to remember a mapping forever.
     *
     * - `working` — an agent is running. The frame breathes toward the brand
     *   colour and the eyes glance, so a run you started is visibly still
     *   alive from across the window.
     * - `attention` — an agent is parked on a human. Faster, amber: the state
     *   that deserves a glance is the one that is stuck.
     *
     * Idle is silent. A mark that animates while nothing is happening is a
     * mark people learn to stop looking at.
     */
    activity?: "idle" | "working" | "attention";
    class?: string;
  }

  let {
    size = "md",
    interactive = false,
    activity = "idle",
    class: customClass = "",
  }: Props = $props();

  let reacted = $state(false);
  let celebrating = $state(false);
  let root: SVGSVGElement | null = null;

  let reactTimer: ReturnType<typeof setTimeout> | null = null;
  let celebrateTimer: ReturnType<typeof setTimeout> | null = null;
  let recentClicks: number[] = [];

  const sizes: Record<string, string> = {
    sm: "size-8",
    md: "size-10",
    lg: "size-16",
    xl: "size-32",
  };

  function setEyes(x: number, y: number): void {
    root?.style.setProperty("--logo-eye-x", `${(x * 2.4).toFixed(2)}%`);
    root?.style.setProperty("--logo-eye-y", `${(y * 1.8).toFixed(2)}%`);
  }

  function onPointerMove(e: PointerEvent): void {
    if (!interactive || prefersReducedMotion()) return;
    // A touch drag should not drag the eyes around; this is a mouse affordance.
    if (e.pointerType && e.pointerType !== "mouse") return;
    const box = root?.getBoundingClientRect();
    if (!box || box.width === 0) return;
    setEyes(
      clamp((e.clientX - box.left - box.width / 2) / (box.width / 2)),
      clamp((e.clientY - box.top - box.height / 2) / (box.height / 2)),
    );
  }

  function clamp(v: number): number {
    return Math.max(-1, Math.min(1, v));
  }

  function react(): void {
    if (!interactive || prefersReducedMotion()) return;
    if (reactTimer) clearTimeout(reactTimer);
    reacted = false;
    // Force a reflow so the same value re-renders and the animation replays.
    root?.getBoundingClientRect();
    reacted = true;
    reactTimer = setTimeout(() => (reacted = false), 360);
  }

  function onClick(): void {
    react();
    const now = performance.now();
    recentClicks = [...recentClicks.filter((t) => now - t < 900), now];
    if (recentClicks.length < 5) return;
    recentClicks = [];
    if (celebrateTimer) clearTimeout(celebrateTimer);
    celebrating = false;
    root?.getBoundingClientRect();
    celebrating = true;
    celebrateTimer = setTimeout(() => (celebrating = false), 1200);
  }

  function onKeyDown(e: KeyboardEvent): void {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    react();
  }

  $effect(() => {
    return () => {
      if (reactTimer) clearTimeout(reactTimer);
      if (celebrateTimer) clearTimeout(celebrateTimer);
    };
  });
</script>

<!--
  A focusable SVG is the whole point of an interactive mark, and Svelte's rule
  does not model `role="button"` on an element it has no rule for.
-->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<svg
  bind:this={root}
  class="raven-logo {sizes[size]} {customClass}"
  data-reacted={reacted || undefined}
  data-celebrating={celebrating || undefined}
  data-interactive={interactive ? "true" : undefined}
  data-activity={activity === "idle" ? undefined : activity}
  viewBox="0 0 240 240"
  role={interactive ? "button" : undefined}
  tabindex={interactive ? 0 : undefined}
  aria-label={interactive ? t("logo.label") : undefined}
  aria-hidden={interactive ? undefined : "true"}
  onclick={onClick}
  onkeydown={onKeyDown}
  onpointerleave={() => setEyes(0, 0)}
  onpointermove={onPointerMove}
>
  <rect class="app-logo-background" width="240" height="240" rx="50" ry="50" />
  <g class="app-logo-eye-motion">
    <polyline
      class="app-logo-eye app-logo-eye-left"
      points="43.55 93.61 64.69 81.41 36.48 108.04 79.67 83.11 35.93 122.88 91.58 90.74 38.9 132.69 97.66 98.76 42.44 138.88 100.43 105.4 46.9 143.83 101.97 112.04 55.08 149.51 101.83 122.52 73.01 152.43 94.14 140.23"
    />
  </g>
  <g class="app-logo-eye-motion">
    <polyline
      class="app-logo-eye app-logo-eye-right"
      points="145.65 93.61 166.79 81.41 140.83 101.52 175.58 81.46 138.3 109.53 183.18 83.63 137.55 117.43 189.67 87.33 139.67 129.39 197.88 95.78 142.92 136.32 201.52 102.48 149.03 143.86 204.07 112.08 159.14 150.37 203.51 124.75 169.28 152.61 199.82 134.98"
    />
  </g>
</svg>
