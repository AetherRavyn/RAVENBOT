<script lang="ts">
  /**
   * The OpenBot studio: choose a character, colour it, and see all six work
   * states before applying.
   *
   * The selected state is a *preview* only. Applying stores the definition and
   * colour, never the preview state — the live state comes from the fleet
   * mood, because an avatar frozen in "blocked" is worse than no avatar.
   */
  import OpenBotAvatar from "$lib/components/OpenBotAvatar.svelte";
  import { Button } from "$lib/components/ui/button";
  import { t } from "$lib/i18n";
  import {
    OPENBOT_COLOURS,
    OPENBOT_GALLERY,
    OPENBOT_STATES,
    normaliseOpenBotColour,
    openBotCuratedColour,
    openBotDefinition,
    openBotDefinitionForName,
    openBotStillUrl,
    openBotStyleId,
    openBotUrl,
    type OpenBotState,
    type OpenBotTone,
  } from "$lib/openBotAvatar";
  import { X } from "@lucide/svelte";

  type Finish = "flat" | "fluffy" | "glass";

  interface Props {
    seed: string;
    definitionId?: string | null;
    colour?: string | null;
    onApply?: (url: string, style: string) => void;
    onClose?: () => void;
  }

  let { seed, definitionId = null, colour = null, onApply, onClose }: Props = $props();

  // The studio opens with the character and colour it was given; a click
  // then owns them. `initial` is intentionally a snapshot of the opening
  // props, not a live binding that would undo a choice.
  // svelte-ignore state_referenced_locally
  const initial = openBotDefinition(definitionId) ?? openBotDefinitionForName(seed || "Agent");
  let chosenId = $state(initial.id);
  // svelte-ignore state_referenced_locally
  let chosenColour = $state(
    normaliseOpenBotColour(colour, openBotCuratedColour(initial.id)),
  );
  const definition = $derived(openBotDefinition(chosenId) ?? initial);
  const bodyColour = $derived(
    normaliseOpenBotColour(chosenColour, openBotCuratedColour(definition.id)),
  );

  let previewState = $state<OpenBotState>("idle");
  let tone = $state<OpenBotTone>("neutral");
  let depth = $state(6);
  let followPointer = $state(false);
  let finish = $state<Finish>("flat");
  let animate = $state(true);

  function choose(id: string) {
    if (id === chosenId) return;
    chosenId = id;
    // Each character has an upstream colour; switching to a character whose
    // colour the user has not touched should show that colour rather than the
    // previous character's.
    chosenColour = openBotCuratedColour(id);
  }

  function finishLabel(option: Finish): string {
    if (option === "flat") return t("openbot.flat");
    if (option === "fluffy") return t("openbot.fluffy");
    return t("openbot.glass");
  }

  function apply() {
    onApply?.(
      openBotUrl(definition.id, bodyColour),
      openBotStyleId(definition.id),
    );
  }
</script>

<div class="openbot-studio" role="dialog" aria-label={t("openbot.title")}>
  <header class="openbot-studio__bar">
    <div class="min-w-0">
      <h3 class="text-sm font-bold text-[var(--text-primary)] truncate">
        {t("openbot.title")}
      </h3>
      <p class="text-[11px] text-[var(--text-tertiary)] leading-snug">
        {t("openbot.description")}
      </p>
    </div>
    {#if onClose}
      <Button
        size="sm"
        variant="ghost"
        class="h-7 px-2 shrink-0"
        aria-label={t("openbot.close")}
        onclick={onClose}
      >
        <X class="size-3.5" />
      </Button>
    {/if}
  </header>

  <div class="openbot-studio__body">
    <div class="openbot-studio__main">
      <!-- The live hero: the chosen character in the previewed state. -->
      <div class="openbot-studio__hero">
        <div class="openbot-studio__hero-frame">
          <OpenBotAvatar
            name={definition.name}
            definitionId={definition.id}
            colour={bodyColour}
            state={previewState}
            moodTone={tone}
            depth={depth}
            followPointer={followPointer}
            material={finish}
            animated={animate}
            decorative
          />
        </div>
        <div class="min-w-0">
          <div class="text-sm font-bold text-[var(--text-primary)] truncate">
            {definition.name}
          </div>
          <div class="text-[11px] text-[var(--text-tertiary)]">
            {t(`openbot.state.${previewState}`)} · {bodyColour}
          </div>
        </div>
      </div>

      <!--
        The six-state strip, exactly the reference order. Each tile is a real
        OpenBotAvatar, so the pose is the one the live roster will draw; the
        selected state is repeated at hero size above.
      -->
      <section class="openbot-studio__section" aria-label={t("openbot.states")}>
        <span class="openbot-studio__label">{t("openbot.states")}</span>
        <div class="openbot-studio__states" role="group" aria-label={t("openbot.states")}>
          {#each OPENBOT_STATES as state (state)}
            <button
              type="button"
              class="openbot-studio__state"
              class:openbot-studio__state--on={previewState === state}
              aria-pressed={previewState === state}
              onclick={() => (previewState = state)}
            >
              <span class="openbot-studio__state-frame">
                <OpenBotAvatar
                  name={definition.name}
                  definitionId={definition.id}
                  colour={bodyColour}
                  state={state}
                  moodTone={tone}
                  depth={depth}
                  followPointer={false}
                  material={finish}
                  animated={animate}
                  decorative
                />
              </span>
              <span class="openbot-studio__state-label">{t(`openbot.state.${state}`)}</span>
            </button>
          {/each}
        </div>
      </section>

      <!-- Gallery: every bundled definition, as a still data URL. -->
      <section class="openbot-studio__section" aria-label={t("openbot.character")}>
        <span class="openbot-studio__label">
          {t("openbot.character")} · {OPENBOT_GALLERY.length}
        </span>
        <div class="openbot-studio__gallery">
          {#each OPENBOT_GALLERY as character (character.id)}
            {@const selected = character.id === definition.id}
            <button
              type="button"
              class="openbot-studio__tile"
              class:openbot-studio__tile--on={selected}
              aria-pressed={selected}
              onclick={() => choose(character.id)}
            >
              <img
                src={openBotStillUrl(character, openBotCuratedColour(character.id))}
                alt=""
                loading="lazy"
                decoding="async"
              />
              <span class="openbot-studio__tile-name">{character.name}</span>
            </button>
          {/each}
        </div>
        <p class="openbot-studio__note">{t("openbot.galleryNote")}</p>
      </section>
    </div>

    <div class="openbot-studio__controls">
      <!-- Body colour: the curated palette plus a native colour input. -->
      <section class="openbot-studio__section">
        <span class="openbot-studio__label">{t("openbot.colour")}</span>
        <div class="openbot-studio__swatches">
          {#each OPENBOT_COLOURS as swatch (swatch)}
            <button
              type="button"
              class="openbot-studio__swatch"
              class:openbot-studio__swatch--on={bodyColour === swatch}
              aria-pressed={bodyColour === swatch}
              aria-label={swatch}
              title={swatch}
              style="background: {swatch}"
              onclick={() => (chosenColour = swatch)}
            ></button>
          {/each}
          <input
            type="color"
            class="openbot-studio__colour-input"
            aria-label={t("openbot.colour")}
            bind:value={chosenColour}
          />
        </div>
      </section>

      <section class="openbot-studio__section">
        <span class="openbot-studio__label">{t("openbot.mood")}</span>
        <div class="openbot-studio__segmented" role="group" aria-label={t("openbot.mood")}>
          {#each [{ id: "neutral", label: t("openbot.neutral") }, { id: "happy", label: t("openbot.happy") }] as option (option.id)}
            <button
              type="button"
              class="openbot-studio__segment"
              class:openbot-studio__segment--on={tone === option.id}
              aria-pressed={tone === option.id}
              onclick={() => (tone = option.id as OpenBotTone)}
            >
              {option.label}
            </button>
          {/each}
        </div>
      </section>

      <section class="openbot-studio__section">
        <span class="openbot-studio__label">{t("openbot.finish")}</span>
        <div class="openbot-studio__segmented" role="group" aria-label={t("openbot.finish")}>
          {#each ["flat", "fluffy", "glass"] as option (option)}
            <button
              type="button"
              class="openbot-studio__segment"
              class:openbot-studio__segment--on={finish === option}
              aria-pressed={finish === option}
              onclick={() => (finish = option as Finish)}
            >
              {finishLabel(option as Finish)}
            </button>
          {/each}
        </div>
      </section>

      <section class="openbot-studio__section">
        <label class="openbot-studio__label" for="openbot-volume">
          {t("openbot.volume")} · {depth}
        </label>
        <input
          id="openbot-volume"
          type="range"
          min="0"
          max="14"
          step="1"
          class="openbot-studio__range"
          aria-label={t("openbot.volume")}
          bind:value={depth}
        />
      </section>

      <label class="openbot-studio__check">
        <input type="checkbox" bind:checked={followPointer} />
        <span>{t("openbot.followPointer")}</span>
      </label>

      <label class="openbot-studio__check">
        <input type="checkbox" bind:checked={animate} />
        <span>{t("openbot.animate")}</span>
      </label>
    </div>
  </div>

  <footer class="openbot-studio__foot">
    {#if onClose}
      <Button size="sm" variant="outline" class="h-8" onclick={onClose}>
        {t("openbot.close")}
      </Button>
    {/if}
    <Button size="sm" class="h-8 px-4" onclick={apply}>
      {t("openbot.apply")}
    </Button>
  </footer>
</div>

