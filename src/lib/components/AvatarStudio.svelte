<script lang="ts">
  /**
   * The avatar studio: see your agent in every state it will ever be in.
   *
   * Grok Bot's studio is built around one idea — *"pick a shape, an expression
   * and a colour, play every animation"* — and the third clause is the part
   * worth having. An avatar is normally only ever seen in the one state you
   * happened to load the app in, which makes the other five a guess. Here they
   * are all on screen at once and moving, so the question "will I be able to tell
   * that one is waiting on me?" is answered before the agent exists rather than
   * during an incident.
   *
   * Two things are chosen by hand and one is not:
   *
   *  - **Shape and expression** are the agent's identity, and are picked here.
   *    This is the only place a user can set them without renaming the agent.
   *  - **Colour** is derived from the name, so a roster stays varied without
   *    anyone maintaining it. It is shown, not chosen, because a colour nobody
   *    can mis-set is worth more than one more axis to fiddle with.
   *
   * Only the natively drawn face is studio-able. A DiceBear style has its own
   * animation inside its SVG and no expressions to choose from, so there is
   * nothing here to compose — the picker is where those are chosen.
   */
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { t } from "$lib/i18n";
  import { avatarProfile, SILHOUETTES, type AvatarMood, type Expression } from "$lib/avatar";
  import { Dices, X } from "@lucide/svelte";

  interface Props {
    seed: string;
    /** The expression to start on. */
    expression?: Expression;
    silhouette?: string;
    onApply?: (silhouette: string, expression: Expression) => void;
    onClose?: () => void;
  }

  let { seed, expression, silhouette, onApply, onClose }: Props = $props();

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
  let shape = $state<string>("round");
  let face = $state<Expression>("neutral");

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
  let propShape = $state<string | undefined>(undefined);
  let propFace = $state<Expression | undefined>(undefined);

  $effect(() => {
    if (propShape === undefined && silhouette) propShape = silhouette;
    if (propFace === undefined && expression) propFace = expression;
    const p = avatarProfile(seed || "Agent");
    if (!lockedShape) shape = propShape ?? p.silhouette;
    if (!lockedFace) face = propFace ?? p.resting;
  });

  function chooseShape(next: string) {
    shape = next;
    lockedShape = true;
  }

  function chooseFace(next: Expression) {
    face = next;
    lockedFace = true;
  }

  /**
   * Every roster state, on screen and moving at once.
   *
   * Order is the order a user meets them in: nothing, something happening,
   * something needing you, something stopped, something finished. `waiting` sits
   * third on purpose — it is the state that has to be findable, so it is placed
   * where the eye lands after "working" rather than at the end.
   */
  const STATES: { mood: AvatarMood; note: string }[] = [
    { mood: "idle", note: "Nothing needs you" },
    { mood: "working", note: "Working on its own" },
    { mood: "waiting", note: "Needs a decision" },
    { mood: "failed", note: "Stopped and could not continue" },
    { mood: "responded", note: "Finished, unread" },
    { mood: "sleeping", note: "Nothing happening" },
  ];

  const EXPRESSION_CHOICES: Expression[] = [
    "neutral", "attentive", "curious", "pleased", "sad", "sleepy",
  ];

  function randomize() {
    chooseShape(SILHOUETTES[Math.floor(Math.random() * SILHOUETTES.length)]);
    chooseFace(EXPRESSION_CHOICES[Math.floor(Math.random() * EXPRESSION_CHOICES.length)]);
  }

  /** Back to what the name decides, which is what "reset" has to mean. */
  function reset() {
    lockedShape = false;
    lockedFace = false;
  }
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
    <!-- Every state, live. This is the reason the panel exists. -->
    <section class="raven-studio__states" aria-label={t("studio.states")}>
      {#each STATES as s (s.mood)}
        <div class="raven-studio__state">
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
              silhouette={shape}
              expression={face}
              class="size-14"
              decorative
            />
          </div>
          <span class="text-[10px] font-bold text-[var(--text-primary)]">
            {t(`studio.state.${s.mood}`)}
          </span>
          <span class="text-[9px] text-[var(--text-muted)] text-center leading-tight">{s.note}</span>
        </div>
      {/each}
    </section>

    <section class="raven-studio__controls">
      <div class="space-y-1.5">
        <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("studio.shape")}
        </span>
        <div class="flex flex-wrap gap-1.5">
          {#each SILHOUETTES as s (s)}
            <button
              type="button"
              class="raven-studio__chip"
              class:raven-studio__chip--on={shape === s}
              aria-pressed={shape === s}
              onclick={() => chooseShape(s)}
            >
              <RavenAvatar name={seed} style="raven-native" expression={face} silhouette={s} class="size-7" decorative />
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
          {#each EXPRESSION_CHOICES as e (e)}
            <button
              type="button"
              class="raven-studio__chip"
              class:raven-studio__chip--on={face === e}
              aria-pressed={face === e}
              onclick={() => chooseFace(e)}
            >
              <RavenAvatar name={seed} style="raven-native" expression={e} silhouette={shape} class="size-7" decorative />
              <span class="text-[9px]">{t(`studio.expr.${e}`)}</span>
            </button>
          {/each}
        </div>
      </div>

      <!--
        Colour is shown rather than chosen. It comes from the name, which is
        what keeps a roster varied without anyone maintaining it, and a colour a
        user cannot set wrong is worth more than one more axis.
      -->
      <div class="space-y-1">
        <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("studio.colour")}
        </span>
        <div class="flex items-center gap-2">
          <span
            class="size-5 rounded-full ring-1 ring-white/20"
            style="background: hsl({profile.hue} 62% 42%)"
          ></span>
          <span class="text-[11px] text-[var(--text-secondary)]">{t("studio.colourFromName")}</span>
          <Badge variant="outline" class="text-[9px] ml-auto">hue {profile.hue}</Badge>
        </div>
      </div>
    </section>
  </div>

  {#if onApply}
    <footer class="raven-studio__foot">
      <Button size="sm" class="h-8 px-4" onclick={() => onApply(shape, face)}>
        {t("studio.apply")}
      </Button>
    </footer>
  {/if}
</div>