<script lang="ts">
  /**
   * The agent's face, as generated SVG.
   *
   * The silhouette and hue come from the agent's name and never change; only
   * the expression does, and it follows what the agent is doing. The rings and
   * the breathing are drawn *around* and *over* the body rather than replacing
   * it, because the outline is how a user knows which agent they are looking
   * at.
   *
   * Everything is inline SVG rather than an `<img src>`: there is no network
   * request, it renders at any size, it inherits the theme's tokens, and it can
   * animate. A remote avatar URL is a fixed bitmap, which is why it could not
   * react to anything.
   */
  import { avatarProfile, faceFor, moodPresentation, hueVars, initials, type AvatarMood } from "$lib/avatar";
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
     *
     * Defaults to `clay` — an animated style at roughly a 2.9 s cadence, so an
     * agent's face visibly changes while you are looking at it. `raven-native`
     * is the one style with no network request, and remains the right choice
     * offline.
     */
    style?: string | null;
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
    animated,
    class: customClass = "",
    decorative = false,
  }: Props = $props();

  const profile = $derived(avatarProfile(name || "Agent"));
  const face = $derived(faceFor(profile, mood));
  const moodStyle = $derived(moodPresentation(mood));
  const vars = $derived(hueVars(profile.hue));

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
   * The silhouette path, in a 100×100 box centred on 50,50.
   *
   * Every shape is drawn to the same bounding circle so switching between them
   * does not change the avatar's size — the agent should appear to change
   * expression, not to resize.
   */
  const SILHOUETTE_PATHS: Record<string, string> = {
    round: "M50 8 A42 42 0 1 1 49.9 8 Z",
    hex: "M50 7 L88 28 L88 72 L50 93 L12 72 L12 28 Z",
    droplet: "M50 8 C68 30 88 46 88 62 A38 38 0 0 1 12 62 C12 46 32 30 50 8 Z",
    capsule: "M50 8 A30 30 0 0 1 80 38 L80 70 A30 30 0 0 1 20 70 L20 38 A30 30 0 0 1 50 8 Z",
    shield: "M50 7 L86 22 L86 52 C86 74 70 88 50 94 C30 88 14 74 14 52 L14 22 Z",
    crystal: "M50 6 L78 30 L78 70 L50 94 L22 70 L22 30 Z",
  };

  const body = $derived(SILHOUETTE_PATHS[profile.silhouette] ?? SILHOUETTE_PATHS.round);

  /**
   * Face geometry, as data rather than markup.
   *
   * An earlier draft held these as SVG strings and injected them with
   * `{@html}` — which is the thing the markdown renderer was just rewritten to
   * stop doing, and doing it here for the sake of three lines of markup would
   * be indefensible. Each shape is a descriptor the template renders as an
   * element, so the file contains no raw SVG injection at all.
   */
  type FaceShape =
    | { kind: "circle"; cx: number; cy: number; r: number; filled: boolean; width: number }
    | { kind: "path"; d: string; width: number };

  const FACES: Record<string, FaceShape[]> = {
    neutral: [
      { kind: "circle", cx: 38, cy: 47, r: 4.2, filled: true, width: 0 },
      { kind: "circle", cx: 62, cy: 47, r: 4.2, filled: true, width: 0 },
      { kind: "path", d: "M42 63 Q50 67 58 63", width: 3 },
    ],
    attentive: [
      // Wider, and with a brow, so it reads as focus rather than excitement.
      { kind: "circle", cx: 37, cy: 46, r: 5, filled: true, width: 0 },
      { kind: "circle", cx: 63, cy: 46, r: 5, filled: true, width: 0 },
      { kind: "path", d: "M30 37 Q37 34 44 37", width: 2.6 },
      { kind: "path", d: "M56 37 Q63 34 70 37", width: 2.6 },
      { kind: "path", d: "M41 64 Q50 70 59 64", width: 3 },
    ],
    pleased: [
      { kind: "path", d: "M33 48 Q38 42 43 48", width: 3.4 },
      { kind: "path", d: "M57 48 Q62 42 67 48", width: 3.4 },
      { kind: "path", d: "M39 60 Q50 72 61 60", width: 3.4 },
    ],
    sad: [
      { kind: "circle", cx: 38, cy: 48, r: 3.8, filled: true, width: 0 },
      { kind: "circle", cx: 62, cy: 48, r: 3.8, filled: true, width: 0 },
      { kind: "path", d: "M31 40 Q38 44 45 40", width: 2.6 },
      { kind: "path", d: "M55 40 Q62 44 69 40", width: 2.6 },
      { kind: "path", d: "M41 68 Q50 61 59 68", width: 3 },
    ],
    curious: [
      // One eye larger: a question asked with the face.
      { kind: "circle", cx: 37, cy: 47, r: 4.4, filled: true, width: 0 },
      { kind: "circle", cx: 63, cy: 45, r: 6, filled: true, width: 0 },
      { kind: "path", d: "M56 34 Q63 30 70 34", width: 2.6 },
      { kind: "circle", cx: 50, cy: 65, r: 4, filled: true, width: 0 },
    ],
    sleepy: [
      { kind: "path", d: "M32 48 Q38 52 44 48", width: 3.2 },
      { kind: "path", d: "M56 48 Q62 52 68 48", width: 3.2 },
      { kind: "circle", cx: 50, cy: 66, r: 4, filled: true, width: 0 },
    ],
  };

  const faceGeom = $derived(FACES[face] ?? FACES.neutral);
</script>

<span
  class="raven-avatar {customClass}"
  data-mood={mood}
  data-state={moodStyle.state}
  data-silhouette={profile.silhouette}
  data-source={source}
  data-animates={imageAnimates || undefined}
  style="--av-accent: {vars.accent}; --av-dim: {vars.dim}; --av-deep: {vars.deep}; --av-breathe: {moodStyle.breathe}"
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
      <!-- Orbit rings sit behind the body, so they read as activity around it
           rather than a different animal. -->
      {#if moodStyle.rings}
        <g class="raven-avatar-rings" fill="none" stroke="var(--av-accent)" stroke-width="1.6" opacity="0.5">
          <ellipse cx="50" cy="50" rx="46" ry="18" transform="rotate(-24 50 50)" />
          <ellipse cx="50" cy="50" rx="46" ry="18" transform="rotate(34 50 50)" />
        </g>
      {/if}

      <g class="raven-avatar-body">
        <path d={body} fill="var(--av-deep)" stroke="var(--av-accent)" stroke-width="2" />
        <path d={body} fill="url(#raven-av-shine)" opacity="0.18" />
      </g>

      <!-- A single shared gradient id: two avatars on one page would otherwise
           each need a unique one, and duplicate ids in a document resolve to
           the first. -->
      <defs>
        <radialGradient id="raven-av-shine" cx="34%" cy="28%" r="70%">
          <stop offset="0%" stop-color="#fff" stop-opacity="0.9" />
          <stop offset="100%" stop-color="#fff" stop-opacity="0" />
        </radialGradient>
      </defs>

      <g class="raven-avatar-face" color="var(--av-accent)">
        {#each faceGeom as shape, i (i)}
          {#if shape.kind === "circle"}
            <circle
              cx={shape.cx}
              cy={shape.cy}
              r={shape.r}
              fill={shape.filled ? "currentColor" : "none"}
              stroke={shape.filled ? "none" : "currentColor"}
              stroke-width={shape.width}
            />
          {:else}
            <path
              d={shape.d}
              fill="none"
              stroke="currentColor"
              stroke-width={shape.width}
              stroke-linecap="round"
            />
          {/if}
        {/each}
      </g>
    </svg>
  {:else}
    <span class="raven-avatar-initials">{initials(name)}</span>
  {/if}
</span>
