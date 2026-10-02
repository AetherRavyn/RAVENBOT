<script lang="ts">
  /**
   * The avatar studio: see your agent in every state it will ever be in.
   *
   * Grok Bot's studio is built around one idea — *"pick a shape, an expression
   * and a colour, play every animation"* — and the last clause is the part worth
   * having. An avatar is normally only ever seen in the one state you happened
   * to load the app in, which makes the other fourteen a guess. Here they are
   * all on screen at once and moving, so the question "will I be able to tell
   * that one is waiting on me?" is answered before the agent exists rather than
   * during an incident.
   *
   * Three things are chosen by hand and one is not:
   *
   *  - **Shape, expression and colour** are the agent's identity, and are picked
   *    here. This is the only place a user can set them without renaming the
   *    agent.
   *  - Everything else is *not* chosen — it is what the runtime decides, which
   *    is exactly why it is worth previewing.
   *
   * The two strips are deliberately different things. The **states** strip is
   * the six roster moods the runtime can actually put an agent into; the
   * **animations** strip is all fifteen animation states. They overlap, and the
   * overlap is the point: it is how you see that "long run" and "spinning rings"
   * are the same thing said in two vocabularies.
   *
   * Only the natively drawn face is studio-able. A DiceBear style has its own
   * animation inside its SVG and no expressions to choose from, so there is
   * nothing here to compose — the picker is where those are chosen.
   */
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { t } from "$lib/i18n";
  import {
    avatarProfile,
    COLOURS,
    EXPRESSIONS,
    SHAPES,
    ANIMATIONS,
    SILHOUETTES,
    motionFor,
    type AvatarMood,
    type ColourId,
    type Expression,
    type MotionState,
    type Shape,
  } from "$lib/avatar";
  import { Dices, X } from "@lucide/svelte";

  interface Props {
    seed: string;
    /** The expression to start on. */
    expression?: Expression;
    silhouette?: string;
    /** The colour to start on. Defaults to the one the name derives. */
    colour?: ColourId;
    onApply?: (silhouette: string, expression: Expression, colour: ColourId) => void;
    onClose?: () => void;
  }

  let { seed, expression, silhouette, colour: colourProp, onApply, onClose }: Props = $props();

  const profile = $derived(avatarProfile(seed || "Agent"));

  /**
   * Initialised to the props, but *kept* in state and re-seeded by the effect
   * below.
   *
   * Reading a prop in a `$state` initialiser captures one value and never
   * updates, which would freeze the studio on whatever it was opened with. The
   * effect re-derives from the name whenever it changes and unless the user has
   * picked something themselves, so an explicit choice survives typing and an
   * untouched panel tracks the name.
   */
  let shape = $state<string>("circle");
  let face = $state<Expression>("neutral");
  let tint = $state<ColourId>("blue");

  /**
   * Whether the *user* has picked, as opposed to a prop having supplied it.
   *
   * Three things want to set these — a prop, the agent's name, and a click — and
   * the precedence has to be explicit, because the obvious version gets it wrong
   * in a way nothing shows: seeding from the prop on the first tick and then
   * re-deriving from the name on the next one honours the prop for exactly one
   * frame, so the studio opens with the caller's shape and silently switches to
   * the name-derived one.
   *
   * So the prop is remembered once, the name supplies a default, and a click
   * locks the choice against both.
   */
  let lockedShape = $state(false);
  let lockedFace = $state(false);
  let lockedColour = $state(false);
  let propShape = $state<string | undefined>(undefined);
  let propFace = $state<Expression | undefined>(undefined);
  let propColour = $state<ColourId | undefined>(undefined);

  $effect(() => {
    if (propShape === undefined && silhouette) propShape = silhouette;
    if (propFace === undefined && expression) propFace = expression;
    if (propColour === undefined && colourProp) propColour = colourProp;
    const p = avatarProfile(seed || "Agent");
    if (!lockedShape) shape = propShape ?? p.shape;
    if (!lockedFace) face = propFace ?? p.resting;
    if (!lockedColour) tint = propColour ?? p.colour;
  });

  function chooseShape(next: string) {
    shape = next;
    lockedShape = true;
  }

  function chooseFace(next: Expression) {
    face = next;
    lockedFace = true;
  }

  function chooseColour(next: ColourId) {
    tint = next;
    lockedColour = true;
  }

  /**
   * Every roster state, on screen and moving at once.
   *
   * Order is the order a user meets them in: nothing, something happening,
   * something long-running, something needing you, something stopped, something
   * finished. `waiting` sits fourth on purpose — it is the state that has to be
   * findable, so it is placed where the eye lands after the two that do not need
   * you, rather than at the end.
   */
  const STATES: { mood: AvatarMood; note: string }[] = [
    { mood: "idle", note: "Nothing needs you" },
    { mood: "working", note: "Working on its own" },
    { mood: "thinking", note: "Long run — leave it be" },
    { mood: "waiting", note: "Needs a decision" },
    { mood: "failed", note: "Stopped and could not continue" },
    { mood: "responded", note: "Finished, unread" },
    { mood: "sleeping", note: "Nothing happening" },
  ];

  function randomize() {
    chooseShape(SHAPES[Math.floor(Math.random() * SHAPES.length)]);
    chooseFace(EXPRESSIONS[Math.floor(Math.random() * EXPRESSIONS.length)]);
    chooseColour(COLOURS[Math.floor(Math.random() * COLOURS.length)].id);
  }

  /** Back to what the name decides, which is what "reset" has to mean. */
  function reset() {
    lockedShape = false;
    lockedFace = false;
    lockedColour = false;
  }

  /** The animation strip is selected too, so it is state like any other. */
  let anim = $state<MotionState>("thinking");
  const setAnim = (a: MotionState) => (anim = a);
</script>

<div class="raven-studio" role="dialog" aria-label={t("studio.title")}>
  <header class="raven-studio__bar">
    <Dices class="size-4 text-[var(--brand-text)] shrink-0" />
    <span class="text-xs font-bold text-[var(--text-primary)] flex-1 truncate">{t("studio.title")}</span>
    <Button size="sm" variant="ghost" class="h-7 text-[11px] gap-1.5" onclick={randomize}>
      {t("studio.randomize")}
    </Button>
    <Button size="sm" variant="ghost" class="h-7 text-[11px]" onclick={reset}>{t("studio.reset")}</Button>
    {#if onClose}
      <Button size="sm" variant="ghost" class="h-7 px-2" aria-label={t("ui.close")} onclick={onClose}>
        <X class="size-3.5" />
      </Button>
    {/if}
  </header>

  <div class="raven-studio__body">
    <div class="raven-studio__left">
      <!--
        The hero. The shape, expression and colour being edited, in whatever
        state is selected below, at a size you can actually judge. Everything
        else in this panel is 28 pixels tall and cannot answer "does this read
        as attentive" on its own.
      -->
      <div class="raven-studio__hero">
        <RavenAvatar
          name={seed}
          style="raven-native"
          shape={shape}
          expression={face}
          colour={tint}
          state={anim}
          class="raven-studio__hero-avatar"
          decorative
        />
        <div class="raven-studio__hero-meta">
          <span class="raven-studio__hero-name">
            {t(`studio.shape.${shape as Shape}`)} · {t(`studio.expr.${face}`)} · {t(`studio.colour.${tint}`)}
          </span>
          <span class="raven-studio__hero-state">{t(`studio.anim.${anim}`)}</span>
        </div>
      </div>

      <!-- Every roster state, live. This is the reason the panel exists. -->
      <section class="raven-studio__states" aria-label={t("studio.states")}>
        {#each STATES as s (s.mood)}
          <button
            type="button"
            class="raven-studio__state"
            aria-pressed={anim === motionFor(s.mood)}
            onclick={() => setAnim(motionFor(s.mood))}
          >
            <div class="raven-studio__state-frame">
              <!--
                The shape and expression *being edited*, not the ones derived from
                the name. Passing only the name here made the whole panel inert —
                the pickers changed, the strip did not — and nothing about the
                markup said so.
              -->
              <RavenAvatar
                name={seed}
                style="raven-native"
                mood={s.mood}
                shape={shape}
                expression={face}
                colour={tint}
                class="size-14"
                decorative
              />
            </div>
            <span class="text-[10px] font-bold text-[var(--text-primary)]">
              {t(`studio.state.${s.mood}`)}
            </span>
            <span class="text-[9px] text-[var(--text-muted)] text-center leading-tight">{s.note}</span>
          </button>
        {/each}
      </section>
    </div>

    <div class="raven-studio__right">
      <!-- Shape, expression, colour. Identity. -->
      <div class="raven-studio__controls">
        <div class="space-y-1.5">
          <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
            {t("studio.shape")}
          </span>
          <div class="flex flex-wrap gap-1.5">
            {#each SHAPES as s (s)}
              <button
                type="button"
                class="raven-studio__chip"
                class:raven-studio__chip--on={shape === s}
                aria-pressed={shape === s}
                aria-label={t(`studio.shape.${s}`)}
                onclick={() => chooseShape(s)}
              >
                <RavenAvatar
                  name={seed}
                  style="raven-native"
                  expression={face}
                  shape={s}
                  colour={tint}
                  class="size-7"
                  decorative
                />
                <span class="text-[9px]">{t(`studio.shape.${s}`)}</span>
              </button>
            {/each}
          </div>
        </div>

        <div class="space-y-1.5">
          <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
            {t("studio.expression")}
          </span>
          <div class="flex flex-wrap gap-1.5">
            {#each EXPRESSIONS as e (e)}
              <button
                type="button"
                class="raven-studio__chip raven-studio__chip--face"
                class:raven-studio__chip--on={face === e}
                aria-pressed={face === e}
                aria-label={t(`studio.expr.${e}`)}
                onclick={() => chooseFace(e)}
              >
                <RavenAvatar
                  name={seed}
                  style="raven-native"
                  expression={e}
                  shape={shape}
                  colour={tint}
                  class="size-7"
                  decorative
                />
                <span class="text-[9px]">{t(`studio.expr.${e}`)}</span>
              </button>
            {/each}
          </div>
        </div>

        <div class="space-y-1">
          <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
            {t("studio.colour")}
          </span>
          <div class="flex flex-wrap gap-1.5">
            {#each COLOURS as c (c.id)}
              <button
                type="button"
                class="raven-studio__swatch"
                class:raven-studio__swatch--on={tint === c.id}
                aria-pressed={tint === c.id}
                aria-label={t(`studio.colour.${c.id}`)}
                title={t(`studio.colour.${c.id}`)}
                style="background: {c.hex}"
                onclick={() => chooseColour(c.id)}
              ></button>
            {/each}
          </div>
        </div>
      </div>

      <!-- All fifteen animation states. The other reason the panel exists. -->
      <div class="raven-studio__controls raven-studio__controls--anim">
        <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("studio.animations")}
        </span>
        <div class="raven-studio__anims">
          {#each ANIMATIONS as a (a.id)}
            <button
              type="button"
              class="raven-studio__anim"
              class:raven-studio__anim--on={anim === a.id}
              aria-pressed={anim === a.id}
              title={a.note}
              onclick={() => setAnim(a.id)}
            >
              <RavenAvatar
                name={seed}
                style="raven-native"
                shape={shape}
                expression={face}
                colour={tint}
                state={a.id}
                class="size-8"
                decorative
              />
              <span class="text-[9px]">{t(`studio.anim.${a.id}`)}</span>
            </button>
          {/each}
        </div>
        <p class="raven-studio__hint">{t("studio.animationsHelp")}</p>
      </div>
    </div>
  </div>

  {#if onApply}
    <footer class="raven-studio__foot">
      <Button size="sm" class="h-8 px-4" onclick={() => onApply(shape, face, tint)}>
        {t("studio.apply")}
      </Button>
    </footer>
  {/if}
</div>