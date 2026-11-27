<script lang="ts">
  /**
   * An OpenBot character, mounted as a live canvas (or a CSS 3D scene) and
   * falling back to one SVG frame where no canvas exists.
   *
   * The package owns the character: the definition is geometry, the colour is
   * a body fill, and the six states are poses. RAVENBOT owns only the mapping
   * from its seven fleet moods to those six states and the storage format.
   *
   * The fallback is not decoration. A jsdom test, an SSR render, or an old
   * WebKit build without a usable canvas should still show the right
   * character in the right state, so the component renders
   * `openBotStillUrl(...)` as an `<img>` until a live host is known good.
   * Nothing here inserts raw markup.
   */
  import { onMount } from "svelte";
  import { mountAvatar, type AvatarOptions, type MountedAvatar } from "open-bot-avatar/dom";
  import type { AvatarMaterial } from "open-bot-avatar/dom";
  import {
    openBotStateForMood,
    openBotToneForMood,
    openBotStillUrl,
    resolveOpenBotColour,
    resolveOpenBotDefinition,
    type OpenBotState,
    type OpenBotTone,
  } from "$lib/openBotAvatar";
  import type { AvatarMood } from "$lib/avatar";

  interface Props {
    /** The agent's name. Used for the accessible name and stable defaults. */
    name: string;
    /** The live fleet mood. Maps onto one of the six OpenBot states. */
    mood?: AvatarMood;
    /** A stored `openbot://<id>?color=<hex>` URL. */
    imageUrl?: string | null;
    /** An explicit definition id, for the studio. */
    definitionId?: string | null;
    /** An explicit body colour, for the studio. */
    colour?: string | null;
    /** A preview override. Never stored by the studio. */
    state?: OpenBotState | null;
    /** The renderer's mood axis. */
    moodTone?: OpenBotTone;
    followPointer?: boolean;
    depth?: number;
    material?: "flat" | "fluffy" | "glass";
    renderer?: "canvas" | "scene";
    /** Force one fixed pose. */
    still?: boolean;
    /** `false` forces a still pose; `true`/unset lets the state animate. */
    animated?: boolean;
    class?: string;
    /** Decorative beside a name: no role, no accessible name. */
    decorative?: boolean;
  }

  let {
    name,
    mood = "idle",
    imageUrl = null,
    definitionId = null,
    colour = null,
    state: stateProp = null,
    moodTone,
    followPointer = false,
    depth = 6,
    material = "flat",
    renderer = "canvas",
    still = false,
    animated,
    class: customClass = "",
    decorative = false,
  }: Props = $props();

  const definition = $derived(resolveOpenBotDefinition({ name, definitionId, imageUrl }));
  const resolvedColour = $derived(resolveOpenBotColour({ name, colour, imageUrl }));
  const liveState = $derived<OpenBotState>(stateProp ?? openBotStateForMood(mood));
  const tone = $derived<OpenBotTone>(moodTone ?? openBotToneForMood(mood));
  const isStill = $derived(still || animated === false);
  const label = $derived((name || definition.name || "OpenBot").trim() || "OpenBot");
  const stillUrl = $derived(openBotStillUrl(definition, resolvedColour, liveState, tone));

  let host = $state<HTMLDivElement | null>(null);
  let live = $state(false);
  let failed = $state(false);
  let mounted: MountedAvatar | null = null;
  let mountedOptions: AvatarOptions | null = null;
  let destroyed = false;

  /**
   * The imported material value, not its name. The package deliberately takes
   * a value so a bundler can leave unused finishes out; a name would draw
   * flat and log a warning.
   */
  let materialValue = $state<AvatarMaterial>("flat");

  /**
   * Whether a live host can draw here.
   *
   * `ResizeObserver` is the honest proxy for a real layout engine: the mount
   * needs layout to size its canvas, and jsdom/SSR do not have it. The scene
   * renderer is DOM-only, so it skips the canvas check; the canvas renderer
   * additionally needs a 2D context. WebGL2 is *not* required here — the
   * package draws flat without it, and the component must not pretend
   * otherwise.
   */
  function canLiveRender(): boolean {
    if (typeof window === "undefined" || typeof document === "undefined") return false;
    if (typeof ResizeObserver === "undefined") return false;
    if (renderer === "scene") return true;
    // jsdom defines HTMLCanvasElement but has no 2D implementation, and asking
    // its stub for a context prints a "not implemented" error. The constructor
    // check is the cheap way to tell a real canvas from that stub.
    if (typeof CanvasRenderingContext2D === "undefined") return false;
    try {
      const canvas = document.createElement("canvas");
      return typeof canvas.getContext === "function" && canvas.getContext("2d") !== null;
    } catch {
      return false;
    }
  }

  function options(): AvatarOptions {
    return {
      definition,
      color: resolvedColour,
      state: liveState,
      mood: tone,
      followPointer,
      depth,
      material: materialValue,
      renderer,
      still: isStill,
      // The package follows the system preference; the app never overrides a
      // person's reduced-motion setting from here.
      reducedMotion: "system",
      label: decorative ? "" : label,
    };
  }

  /** Only the options that actually changed, so the package does not redraw geometry needlessly. */
  function changedOptions(previous: AvatarOptions, next: AvatarOptions): Partial<AvatarOptions> {
    const changed: Partial<AvatarOptions> = {};
    if (previous.definition !== next.definition) changed.definition = next.definition;
    if (previous.color !== next.color) changed.color = next.color;
    if (previous.state !== next.state) changed.state = next.state;
    if (previous.mood !== next.mood) changed.mood = next.mood;
    if (previous.followPointer !== next.followPointer) changed.followPointer = next.followPointer;
    if (previous.depth !== next.depth) changed.depth = next.depth;
    if (previous.material !== next.material) changed.material = next.material;
    if (previous.renderer !== next.renderer) changed.renderer = next.renderer;
    if (previous.still !== next.still) changed.still = next.still;
    if (previous.reducedMotion !== next.reducedMotion) changed.reducedMotion = next.reducedMotion;
    if (previous.label !== next.label) changed.label = next.label;
    return changed;
  }

  function teardown() {
    mounted?.destroy();
    mounted = null;
    mountedOptions = null;
  }

  onMount(() => {
    live = canLiveRender();
    return () => {
      destroyed = true;
      teardown();
    };
  });

  // Load the chosen finish once. A failure is a console warning and a flat
  // body, never a blank avatar or a user-facing error.
  $effect(() => {
    const wanted = material;
    if (wanted === "flat") {
      materialValue = "flat";
      return;
    }
    let cancelled = false;
    const load: Promise<AvatarMaterial> =
      wanted === "fluffy"
        ? import("open-bot-avatar/materials/fluffy").then((module) => module.fluffy)
        : import("open-bot-avatar/materials/glass").then((module) => module.glass);
    load
      .then((value) => {
        if (cancelled || destroyed) return;
        materialValue = value;
      })
      .catch((error) => {
        if (cancelled) return;
        console.warn(`openBotAvatar: could not load the ${wanted} finish; drawing flat`, error);
        materialValue = "flat";
      });
    return () => {
      cancelled = true;
    };
  });

  // Mount or tear down the live host. This effect reads every option through
  // `options()`, so it re-runs on a prop change; the guard makes that a cheap
  // no-op and the second effect below sends only the diff.
  $effect(() => {
    if (!host || !live || failed) {
      teardown();
      return;
    }
    if (mounted) return;
    const initial = options();
    try {
      mounted = mountAvatar(host, initial);
      mountedOptions = initial;
    } catch (error) {
      // An environment that claims layout but cannot draw must not throw into
      // a roster. Drop to the identical still frame.
      console.warn("openBotAvatar: live render failed; showing a still frame", error);
      failed = true;
      teardown();
    }
  });

  // Push only what changed into an already mounted avatar.
  $effect(() => {
    const next = options();
    if (!mounted || !mountedOptions) return;
    const changed = changedOptions(mountedOptions, next);
    if (Object.keys(changed).length === 0) return;
    mounted.update(changed);
    mountedOptions = { ...mountedOptions, ...changed };
  });
</script>

<div
  class="openbot-avatar {customClass}"
  data-source="openbot"
  data-openbot-definition={definition.id}
  data-openbot-state={liveState}
  aria-hidden={decorative ? "true" : undefined}
>
  {#if live && !failed}
    <div
      bind:this={host}
      class="openbot-avatar__host"
      role={decorative ? undefined : "img"}
      aria-label={decorative ? undefined : label}
      aria-hidden={decorative ? "true" : undefined}
    ></div>
  {:else}
    <!--
      The same character and state as a single SVG frame. `alt=""` plus
      aria-hidden keeps a decorative avatar out of the accessibility tree; a
      standalone one carries the agent's name.
    -->
    <img
      class="openbot-avatar__fallback"
      src={stillUrl}
      alt={decorative ? "" : label}
      aria-hidden={decorative ? "true" : undefined}
      loading="lazy"
      decoding="async"
    />
  {/if}
</div>

