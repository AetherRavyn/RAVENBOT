<script lang="ts">
  /**
   * The agent's face, as generated SVG.
   *
   * Built to Grok Bot's avatar specification: 8 shapes, 16 expressions, 12
   * colours, 15 animation states. The outline and the colour come from the
   * agent's name and never change; the expression is the agent's own resting
   * face, and the animation state is what the engine plays *over* it.
   *
   * The split between those last two is the whole trick. An agent that wears a
   * different face while it works reads as a different creature, and the point
   * of a shape is that you can point at it. So the expression is identity and
   * the state is what the engine plays — see `lib/avatar.ts` for the long
   * argument.
   *
   * Everything is inline SVG rather than an `<img src>`: there is no network
   * request, it renders at any size, it inherits the theme's tokens, and it can
   * animate. A remote avatar URL is a fixed bitmap, which is why it could not
   * react to anything.
   */
  import {
    avatarProfile,
    colourVars,
    faceFor,
    moodPresentation,
    initials,
    normaliseColour,
    normaliseExpression,
    normaliseShape,
    normaliseState,
    rosterSafeMotion,
    type AvatarMood,
    type ColourId,
    type Expression,
    type Shape,
  } from "$lib/avatar";
  import { getDiceBearUrl, isNativeAvatarStyle } from "$lib/utils";
  import { styleAnimates, currentDiceBearUrl, DEFAULT_AVATAR_STYLE } from "$lib/diceStyles";

  interface Props {
    /** The agent's name. The only thing the face is derived from. */
    name: string;
    /** What the agent is doing. Defaults to idle. */
    mood?: AvatarMood;
    /** A custom image URL, used instead of any style. */
    imageUrl?: string | null;
    /**
     * A DiceBear style slug, or `raven-native` for the locally drawn face.
     *
     * The animated styles need no help from here: DiceBear 10.x puts the CSS
     * keyframes inside the SVG, so the `<img>` plays them and a visitor who
     * prefers reduced motion sees a still avatar.
     */
    style?: string | null;
    /**
     * Override the derived shape.
     *
     * Only the studio needs this — everywhere else the shape comes from the
     * name, which is what makes it identity. Exposed here rather than by a
     * forked component so the studio previews the *real* avatar, and not a
     * second implementation that can drift from it.
     */
    shape?: string | null;
    /**
     * Override the derived resting expression. The studio's other axis.
     */
    expression?: Expression | string | null;
    /**
     * Override the derived colour, as one of the twelve palette ids.
     *
     * Also studio-only. Left unset, an agent's colour comes from its name,
     * which is what keeps a roster varied without anyone maintaining it.
     */
    colour?: ColourId | string | null;
    /**
     * Play a named animation state instead of the mood's.
     *
     * This is how the studio previews all fifteen, including the three that a
     * roster must never play because they replace the outline. Passing one of
     * those is an explicit request from a component that is showing artwork on
     * purpose, so it is honoured — and `rosterSafeMotion` is applied to the
     * mood's state below, which is the path a live agent takes.
     */
    state?: string | null;
    /**
     * Whether to ask for the style's animation.
     *
     * Defaults to whatever the style does on its own. Set false to force a
     * still avatar — which is the state a user who has turned motion off should
     * see, rather than a set of avatars that ignore the setting.
     */
    animated?: boolean;
    /** Tailwind size class for the frame. */
    class?: string;
    /** Whether the avatar is decorative beside a name, or the only label. */
    decorative?: boolean;
  }

  let {
    name,
    mood = "idle",
    imageUrl = null,
    style = DEFAULT_AVATAR_STYLE,
    shape: shapeProp,
    expression,
    colour: colourProp,
    state: stateProp,
    animated,
    class: customClass = "",
    decorative = false,
  }: Props = $props();

  const profile = $derived(avatarProfile(name || "Agent"));

  /**
   * The face: the agent's resting one, unless the mood overrides it.
   *
   * The `expression` prop replaces the *resting* face, not the mood's. That
   * distinction is load-bearing: it is how the studio previews a chosen face
   * while still showing that an agent waiting on you looks attentive. An
   * override applied to the mood would pin every state to the same drawing and
   * the studio would preview the opposite of what the fleet does.
   */
  const face = $derived(
    normaliseExpression(
      moodPresentation(mood).expression ?? expression ?? faceFor(profile, mood),
    ),
  );

  // The shape is identity, so an override wins; otherwise the name's, unchanged.
  const shape = $derived(normaliseShape(shapeProp ?? profile.shape));
  const colour = $derived(normaliseColour(colourProp ?? profile.colour));
  const vars = $derived(colourVars(colour));
  const moodStyle = $derived(moodPresentation(mood));

  /**
   * The animation state actually rendered.
   *
   * Three inputs, in precedence order: an explicit request (the studio
   * previewing one of the fifteen), the mood's state passed through the
   * roster-safety check, and finally `idle`. The safety check is what stops an
   * agent on a live roster from turning into an egg for two seconds.
   */
  const motion = $derived(
    stateProp ? normaliseState(stateProp) : rosterSafeMotion(moodStyle.state),
  );

  /**
   * The URL to render, and whether it is a DiceBear style or a custom image.
   *
   * Three sources, in order: an explicit image URL, a DiceBear style, and the
   * locally drawn face. A broken image of any kind falls back to the generated
   * face rather than a broken-image icon, because a fleet list full of broken
   * icons is worse than one where every face is at least a face.
   */
  let imageFailed = $state(false);
  /**
   * The style actually in use.
   *
   * Stored agent data can hold a null or an empty string, and both mean "no style
   * was ever chosen" — which is most agents, since the column is newer than some
   * of them. That has to mean *the default style*, not the generated face: an
   * agent nobody configured should look like every other agent, and quietly
   * opting it out of the animated styles is the sort of thing that looks like a
   * bug in a fleet where one face is still and the rest are not. The generated
   * face stays available as a style someone picks.
   */
  const styleId = $derived((style ?? "").trim() || DEFAULT_AVATAR_STYLE);

  /**
   * A stored DiceBear URL, brought up to date.
   *
   * The picker has always saved the whole URL on the agent, so anything created
   * before the animation option existed carries a 9.x URL — and 9.x cannot
   * animate at all. Upgrading it here rather than in a migration fixes every
   * existing agent at once, keeps the seed so nobody's agent changes face, and
   * keeps working for URLs older than the current API.
   */
  const upgraded = $derived(currentDiceBearUrl(imageUrl, animated));

  /**
   * Where the picture comes from, in priority order.
   *
   * A stored DiceBear URL outranks the `style` prop, because it is what the
   * agent's own record says and it names a specific style. A genuinely custom
   * image outranks both, since a user who uploaded a picture meant it. Only when
   * there is neither does the `style` prop apply, and when *that* is the native
   * style the face is drawn here.
   */
  const source = $derived(
    upgraded
      ? "dice"
      : imageUrl
        ? "custom"
        : isNativeAvatarStyle(styleId)
          ? "generated"
          : "dice",
  );
  const diceUrl = $derived(
    source === "dice" && upgraded
      ? upgraded.url
      : source === "dice"
        ? getDiceBearUrl(name, styleId, "", animated)
        : "",
  );
  const showImage = $derived(source !== "generated" && !imageFailed);
  const showFace = $derived(!showImage);
  /**
   * Whether the picture carries its own animation.
   *
   * Decides whether the wrapper adds motion of its own, so an already-animating
   * face is not also breathed — two motions on unrelated periods read as a fault
   * rather than as life.
   */
  const imageAnimates = $derived(
    source === "dice" && (animated ?? (upgraded ? upgraded.animates : styleAnimates(styleId))),
  );

  /**
   * The eight bodies, as paths in a 100×100 box centred on 50,50.
   *
   * Every shape is drawn to the same optical size so switching between them does
   * not change the avatar's apparent scale — the agent should appear to change
   * expression, not to resize. Circle, pebble, squircle, capsule, hexagon and
   * droplet are directly Grok's; triangle and cloud are drawn to the same bounds.
   */
  const SHAPE_PATHS: Record<Shape, string> = {
    circle: "M50 8 A42 42 0 1 1 49.9 8 Z",
    pebble:
      "M50 9 C71 8 89 26 88 48 C87 70 71 90 50 91 C29 92 11 74 12 52 C13 30 29 10 50 9 Z",
    squircle: "M50 8 H74 A26 26 0 0 1 92 34 V66 A26 26 0 0 1 74 92 H26 A26 26 0 0 1 8 66 V34 A26 26 0 0 1 26 8 Z",
    capsule: "M50 8 A30 30 0 0 1 80 38 L80 70 A30 30 0 0 1 20 70 L20 38 A30 30 0 0 1 50 8 Z",
    triangle:
      "M50 11 C57 11 61 15 64 22 L88 73 C92 82 87 90 77 90 L23 90 C13 90 8 82 12 73 L36 22 C39 15 43 11 50 11 Z",
    hexagon: "M50 7 L87 29 L87 71 L50 93 L13 71 L13 29 Z",
    cloud:
      "M50 89 C36 89 26 80 25 69 C16 68 9 60 9 51 C9 42 16 34 26 33 C27 23 36 16 46 17 C52 10 62 11 67 18 C76 15 85 22 85 31 C91 34 93 42 89 49 C87 58 79 63 70 63 C69 79 61 89 50 89 Z",
    droplet: "M50 7 C67 28 88 46 88 62 A38 38 0 0 1 12 62 C12 46 33 28 50 7 Z",
  };

  /**
   * The bodies that the `egg`, `hexagon` and `play` states settle into.
   *
   * Only reachable when a component asks for the state by name — the studio,
   * previewing artwork on purpose. The roster path routes through
   * `rosterSafeMotion`, which never returns one of these.
   */
  const STATE_BODY: Record<string, string> = {
    egg: "M50 12 C74 12 90 38 90 64 A40 40 0 0 1 10 64 C10 38 26 12 50 12 Z",
    hexagon: SHAPE_PATHS.hexagon,
    play: "M24 14 L80 50 L24 86 Z",
  };
  const stateBody = $derived(STATE_BODY[motion] ?? "");

  const body = $derived(SHAPE_PATHS[shape]);

  /**
   * Face geometry, as data rather than markup.
   *
   * An earlier draft held these as SVG strings and injected them with
   * `{@html}` — which is the thing the markdown renderer was just rewritten to
   * stop doing, and doing it here for the sake of three lines of markup would be
   * indefensible. Each shape is a descriptor the template renders as an element,
   * so the file contains no raw SVG injection at all.
   *
   * The parts are kept in separate groups — eyes, brows, mouth, extras — rather
   * than one flat list because the `wink` and `wideEyes` states animate the eyes
   * as a unit, and CSS can only target an element that is its own group.
   */
  type Part = {
    kind: "circle" | "ellipse" | "path";
    cx: number;
    cy: number;
    r: number;
    rx: number;
    ry: number;
    d: string;
    /** Stroke width. Zero means "filled, not stroked". */
    width: number;
    filled: boolean;
    opacity: number;
  };

  interface Face {
    eyes: Part[];
    brows: Part[];
    mouth: Part[];
    /** Blush, sparkles — anything that is not an eye, brow or mouth. */
    extra: Part[];
  }

  const L = 37;
  const R = 63;
  const E = 46;

  /**
   * Constructors for the parts, so the sixteen expressions below read as a
   * drawing rather than as a wall of field names.
   *
   * Every field is filled by every constructor — the type is flat rather than a
   * discriminated union because Svelte's generated markup does not narrow a
   * union across an `{#if}` block, and a flat record that is fully populated is
   * the version that type-checks without a cast in eight places.
   */
  function part(p: Partial<Part>): Part {
    return {
      kind: "path",
      cx: 0,
      cy: 0,
      r: 0,
      rx: 0,
      ry: 0,
      d: "",
      width: 3,
      filled: true,
      opacity: 1,
      ...p,
    };
  }
  const dot = (cx: number, cy = E, r = 4.6): Part => part({ kind: "circle", cx, cy, r, filled: true });
  const ring = (cx: number, cy = E, r = 6.2, width = 2.8): Part =>
    part({ kind: "circle", cx, cy, r, filled: false, width });
  const line = (d: string, width = 3): Part => part({ kind: "path", d, width });
  const oval = (cx: number, cy: number, rx: number, ry: number): Part =>
    part({ kind: "ellipse", cx, cy, rx, ry, filled: true });

  /**
   * The sixteen expressions.
   *
   * Eyes sit at y=46 with the mouth at y≈66, and everything is drawn in the
   * single `--av-face` colour, which the theme picks from the body's luminance.
   * That is what lets Ink and Cream sit in the same palette as Bright Blue
   * without either one going invisible.
   */
  const FACES: Record<Expression, Face> = {
    neutral: {
      eyes: [dot(L), dot(R)],
      brows: [],
      mouth: [line("M42 64 Q50 68 58 64")],
      extra: [],
    },
    attentive: {
      eyes: [dot(L, 46, 5.4), dot(R, 46, 5.4)],
      brows: [line("M30 36 Q37 33 44 36", 2.6), line("M56 36 Q63 33 70 36", 2.6)],
      mouth: [line("M41 64 Q50 70 59 64")],
      extra: [],
    },
    surprised: {
      eyes: [ring(L, 45), ring(R, 45)],
      brows: [line("M30 34 Q37 31 44 34", 2.4), line("M56 34 Q63 31 70 34", 2.4)],
      mouth: [oval(50, 66, 4.4, 5.6)],
      extra: [],
    },
    excited: {
      eyes: [dot(L, 45, 5.2), dot(R, 45, 5.2)],
      brows: [line("M30 35 Q37 31 44 35", 2.6), line("M56 35 Q63 31 70 35", 2.6)],
      mouth: [part({ kind: "path", d: "M38 61 Q50 77 62 61 Z", width: 0 })],
      extra: [],
    },
    happy: {
      eyes: [line("M32 48 Q37 41 42 48", 3.6), line("M58 48 Q63 41 68 48", 3.6)],
      brows: [],
      mouth: [line("M39 61 Q50 74 61 61", 3.6)],
      extra: [],
    },
    laughing: {
      eyes: [line("M32 49 Q37 42 42 49", 3.6), line("M58 49 Q63 42 68 49", 3.6)],
      brows: [],
      mouth: [oval(50, 65, 11, 7)],
      extra: [],
    },
    angry: {
      eyes: [dot(L), dot(R)],
      brows: [line("M30 34 L44 41", 3), line("M56 41 L70 34", 3)],
      mouth: [line("M42 70 Q50 63 58 70")],
      extra: [],
    },
    sad: {
      eyes: [dot(L, 48, 3.9), dot(R, 48, 3.9)],
      brows: [line("M30 40 Q37 44 44 40", 2.6), line("M56 40 Q63 44 70 40", 2.6)],
      mouth: [line("M42 69 Q50 62 58 69")],
      extra: [],
    },
    scared: {
      eyes: [ring(L, 44, 6.8, 2.6), ring(R, 44, 6.8, 2.6)],
      brows: [line("M30 32 Q37 29 44 32", 2.4), line("M56 32 Q63 29 70 32", 2.4)],
      mouth: [line("M40 66 q5 -6 10 0 t10 0", 2.6)],
      extra: [],
    },
    suspicious: {
      eyes: [dot(36, 47, 5), ring(64, 44, 4.4, 2.6)],
      brows: [line("M54 33 Q63 29 70 34", 2.8)],
      mouth: [line("M42 66 Q52 70 60 64", 2.8)],
      extra: [],
    },
    confused: {
      eyes: [dot(L, 47), dot(R, 47)],
      // One brow up, one down. Asymmetry is the entire expression.
      brows: [line("M31 37 L43 34", 2.6), line("M57 40 L69 37", 2.6)],
      mouth: [line("M41 66 q4 -4 8 0 q4 4 8 -2", 2.6)],
      extra: [],
    },
    curious: {
      eyes: [dot(L, 47, 4.5), dot(R, 45, 6.2)],
      brows: [line("M56 33 Q63 29 70 33", 2.6)],
      mouth: [oval(50, 66, 3.8, 4.8)],
      extra: [],
    },
    proud: {
      // Half-lids: an arc with the lid drawn across the top of it.
      eyes: [line("M31 47 Q37 41 43 47", 3), line("M57 47 Q63 41 69 47", 3)],
      brows: [line("M30 38 Q37 35 44 38", 2.2), line("M56 38 Q63 35 70 38", 2.2)],
      mouth: [line("M41 66 Q52 70 61 63", 3)],
      extra: [],
    },
    shy: {
      // Eyes drifted outward, which reads as looking away rather than blinking.
      eyes: [dot(39, 48, 3.6), dot(61, 48, 3.6)],
      brows: [],
      mouth: [line("M44 65 Q50 68 56 65", 2.6)],
      extra: [
        part({ kind: "ellipse", cx: 28, cy: 58, rx: 3.6, ry: 2.2, opacity: 0.45 }),
        part({ kind: "ellipse", cx: 72, cy: 58, rx: 3.6, ry: 2.2, opacity: 0.45 }),
      ],
    },
    unimpressed: {
      eyes: [line("M32 47 L42 47", 3.4), line("M58 47 L68 47", 3.4)],
      brows: [line("M31 40 L43 37", 2.2), line("M57 37 L69 40", 2.2)],
      mouth: [line("M42 66 L58 66", 3.4)],
      extra: [],
    },
    sleepy: {
      eyes: [line("M32 48 Q38 53 44 48", 3.2), line("M56 48 Q62 53 68 48", 3.2)],
      brows: [],
      mouth: [oval(50, 66, 4, 4.6)],
      extra: [],
    },
  };

  const faceGeom = $derived(FACES[face] ?? FACES.neutral);
</script>

<span
  class="raven-avatar {customClass}"
  data-mood={mood}
  data-state={motion}
  data-shape={shape}
  data-colour={colour}
  data-source={source}
  data-animates={imageAnimates || undefined}
  style="--av-face: {vars.base}; --av-deep: {vars.deep}; --av-glow: {vars.glow}; --av-ink: {vars.face}; --av-breathe: {moodStyle.breathe}"
  role={decorative ? undefined : "img"}
  aria-label={decorative ? undefined : name}
  aria-hidden={decorative ? "true" : undefined}
>
  {#if showImage}
    <!--
      One `<img>`, whether the source is a DiceBear style or a user's own image.

      Not inlined: an SVG loaded this way is an isolated document, so it cannot
      be reached into, and the browser still caches it and still defers it. That
      matters here — inlining would mean fetching each avatar before it could be
      painted, and it would put remote markup into the page for a URL a user can
      set. The cost is that the tempo cannot be retimed from outside the SVG, so
      mood is carried by the wrapper instead.
    -->
    <img
      src={diceUrl || imageUrl}
      alt=""
      loading="lazy"
      decoding="async"
      onerror={() => (imageFailed = true)}
    />
  {:else if showFace}
    <svg viewBox="0 0 100 100" class="raven-avatar-svg" focusable="false">
      <!--
        A single shared gradient id: two avatars on one page would otherwise
        each need a unique one, and duplicate ids in a document resolve to the
        first.
      -->
      <defs>
        <radialGradient id="raven-av-shine" cx="34%" cy="26%" r="70%">
          <stop offset="0%" stop-color="#fff" stop-opacity="0.32" />
          <stop offset="100%" stop-color="#fff" stop-opacity="0" />
        </radialGradient>
      </defs>

      <!-- Effects that belong behind the body: a tail, rays, a pulsing ring. -->
      <g class="raven-av-behind" fill="none" stroke="var(--av-ink)" stroke-linecap="round">
        {#if motion === "comet"}
          <path class="raven-av-tail" d="M46 22 L2 50 L46 78 Z" fill="var(--av-ink)" stroke="none" opacity="0.28" />
        {:else if motion === "burst"}
          <g class="raven-av-rays" stroke-width="3.4">
            {#each [0, 45, 90, 135, 180, 225, 270, 315] as a (a)}
              <line
                x1={50 + 44 * Math.cos(((a - 90) * Math.PI) / 180)}
                y1={50 + 44 * Math.sin(((a - 90) * Math.PI) / 180)}
                x2={50 + 51 * Math.cos(((a - 90) * Math.PI) / 180)}
                y2={50 + 51 * Math.sin(((a - 90) * Math.PI) / 180)}
              />
            {/each}
          </g>
        {:else if motion === "alert"}
          <circle class="raven-av-pulse" cx="50" cy="50" r="42" stroke-width="3" />
        {:else if motion === "swirl"}
          <path class="raven-av-swirl" d="M50 8 A42 42 0 0 1 92 50" stroke-width="3.4" />
          <path class="raven-av-swirl" d="M50 92 A42 42 0 0 1 8 50" stroke-width="3.4" />
        {/if}
      </g>

      <!--
        Orbit rings sit behind the body, so they read as activity around it
        rather than a different animal. Grok's `orbit` state: rings that keep
        turning, for a run long enough that the user should leave it alone.
      -->
      {#if moodStyle.rings || motion === "orbit"}
        <g class="raven-avatar-rings" fill="none" stroke="var(--av-ink)" stroke-width="1.8" opacity="0.45">
          <ellipse cx="50" cy="50" rx="47" ry="19" transform="rotate(-24 50 50)" />
          <ellipse cx="50" cy="50" rx="47" ry="19" transform="rotate(34 50 50)" />
        </g>
      {/if}

      <g class="raven-avatar-body">
        <path d={body} fill="var(--av-face)" />
        <path d={body} fill="url(#raven-av-shine)" />
        <path class="raven-av-outline" d={body} fill="none" stroke="var(--av-glow)" stroke-width="1.6" opacity="0.5" />
      </g>

      <!--
        The three states that settle into a different body. Only ever drawn
        because a component asked for the state by name — see `rosterSafeMotion`.
        Drawn over the normal body so the CSS can fade one into the other.
      -->
      {#if stateBody}
        <path class="raven-av-altbody" d={stateBody} fill="var(--av-face)" />
        <path class="raven-av-altbody" d={stateBody} fill="url(#raven-av-shine)" />
      {/if}

      <!-- The face, in the one colour the body's luminance picks. -->
      <g class="raven-avatar-face" color="var(--av-ink)" stroke="currentColor">
        {#if faceGeom.extra.length}
          <g class="raven-av-part raven-av-extra" fill="currentColor" stroke="none">
            {#each faceGeom.extra as p, i (i)}
              <ellipse cx={p.cx} cy={p.cy} rx={p.rx} ry={p.ry} opacity={p.opacity ?? 1} />
            {/each}
          </g>
        {/if}

        <g class="raven-av-part raven-av-brows" fill="none" stroke-width="2.6">
          {#each faceGeom.brows as p, i (i)}
            <path d={p.d} stroke-width={p.width} />
          {/each}
        </g>

        <g class="raven-av-part raven-av-eyes" fill="currentColor">
          <!--
            The two eyes are separate groups so `wink` can close exactly one.
            A flat list of parts would leave CSS with no way to address them.
          -->
          <g class="raven-av-eye raven-av-eye--l">
            {#each faceGeom.eyes.slice(0, 1) as p, i (i)}
              {#if p.kind === "circle"}
                <circle
                  cx={p.cx}
                  cy={p.cy}
                  r={p.r}
                  fill={p.filled ? "currentColor" : "none"}
                  stroke={p.filled ? "none" : "currentColor"}
                  stroke-width={p.width ?? 0}
                />
              {:else if p.kind === "ellipse"}
                <ellipse cx={p.cx} cy={p.cy} rx={p.rx} ry={p.ry} />
              {:else}
                <path d={p.d} fill="none" stroke="currentColor" stroke-width={p.width} stroke-linecap="round" />
              {/if}
            {/each}
          </g>
          <g class="raven-av-eye raven-av-eye--r">
            {#each faceGeom.eyes.slice(1) as p, i (i)}
              {#if p.kind === "circle"}
                <circle
                  cx={p.cx}
                  cy={p.cy}
                  r={p.r}
                  fill={p.filled ? "currentColor" : "none"}
                  stroke={p.filled ? "none" : "currentColor"}
                  stroke-width={p.width ?? 0}
                />
              {:else if p.kind === "ellipse"}
                <ellipse cx={p.cx} cy={p.cy} rx={p.rx} ry={p.ry} />
              {:else}
                <path d={p.d} fill="none" stroke="currentColor" stroke-width={p.width} stroke-linecap="round" />
              {/if}
            {/each}
          </g>
        </g>

        <g class="raven-av-part raven-av-mouth" fill="none" stroke-width="3">
          {#each faceGeom.mouth as p, i (i)}
            {#if p.kind === "path"}
              <path
                d={p.d}
                fill={p.width === 0 ? "currentColor" : "none"}
                stroke={p.width === 0 ? "none" : "currentColor"}
                stroke-width={p.width}
                opacity={p.opacity ?? 1}
              />
            {:else if p.kind === "ellipse"}
              <ellipse
                cx={p.cx}
                cy={p.cy}
                rx={p.rx}
                ry={p.ry}
                fill={p.filled ? "currentColor" : "none"}
              />
            {:else}
              <circle cx={p.cx} cy={p.cy} r={p.r} fill={p.filled ? "currentColor" : "none"} />
            {/if}
          {/each}
        </g>
      </g>

      <!-- Effects that belong in front of the face: dots, a badge, a "z". -->
      <g class="raven-av-front">
        {#if motion === "thinking" || motion === "wink"}
          <!-- Grok's Thinking is three dots. -->
          <g class="raven-av-dots" fill="var(--av-ink)">
            <circle cx="40" cy="79" r="3.1" />
            <circle cx="50" cy="79" r="3.1" />
            <circle cx="60" cy="79" r="3.1" />
          </g>
        {/if}

        {#if motion === "notification"}
          <!--
            "It needs your attention. A blue dot appears." The one place an
            avatar is allowed to be a colour it was not assigned: this dot is
            brand, not identity, and identity is what makes it legible as a
            badge rather than as the agent turning blue.
          -->
          <g class="raven-av-badge">
            <circle cx="79" cy="21" r="9" fill="var(--brand)" />
            <circle cx="79" cy="21" r="9" fill="none" stroke="var(--surface-0, #141414)" stroke-width="2.5" />
            <circle cx="79" cy="21" r="3.6" fill="#fff" />
          </g>
        {/if}

        {#if motion === "exclamation"}
          <g class="raven-av-alert">
            <rect x="76.2" y="9" width="5.2" height="14" rx="2.6" fill="hsl(var(--danger))" />
            <circle cx="78.8" cy="29.5" r="3" fill="hsl(var(--danger))" />
          </g>
        {/if}

        {#if motion === "sleep"}
          <g class="raven-av-zzz" fill="var(--av-ink)">
            <path d="M68 30 h11 l-11 11 h11" fill="none" stroke="var(--av-ink)" stroke-width="2.6" stroke-linejoin="round" />
            <path d="M80 16 h8 l-8 8 h8" fill="none" stroke="var(--av-ink)" stroke-width="2.2" stroke-linejoin="round" />
          </g>
        {/if}
      </g>
    </svg>
  {:else}
    <span class="raven-avatar-initials">{initials(name)}</span>
  {/if}
</span>