<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { getDiceBearUrl, dicebearStyles, isNativeAvatarStyle } from "$lib/utils";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import AvatarStudio from "$lib/components/AvatarStudio.svelte";
  import OpenBotStudio from "$lib/components/OpenBotStudio.svelte";
  import { DEFAULT_AVATAR_STYLE } from "$lib/diceStyles";
  import type { ColourId } from "$lib/avatar";
  import { avatarAllLabel, avatarCategoryLabel, avatarStyleDescription } from "$lib/catalogI18n";
  import {
    OPENBOT_GALLERY,
    openBotColourForName,
    openBotDefinition,
    openBotDefinitionForName,
    openBotStillUrl,
    openBotStyleId,
    openBotUrl,
    parseOpenBotStyle,
    parseOpenBotUrl,
  } from "$lib/openBotAvatar";
  import { cn } from "$lib/utils.js";
  import { t } from "$lib/i18n";
  import { Sparkles, RefreshCw, Image, Wand2, Check, Palette, Dices } from "@lucide/svelte";

  interface Props {
    seed: string;
    style?: string;
    customUrl?: string | null;
    onSelect: (url: string, style: string) => void;
  }

  let { seed = "Agent", style = DEFAULT_AVATAR_STYLE, customUrl = null, onSelect }: Props = $props();

  // svelte-ignore state_referenced_locally
  let selectedStyle = $state(style || DEFAULT_AVATAR_STYLE);
  // svelte-ignore state_referenced_locally
  let previewSeed = $state(seed || "Agent");
  // svelte-ignore state_referenced_locally
  let customImageUrl = $state(customUrl || "");
  // svelte-ignore state_referenced_locally
  let useCustom = $state(
    Boolean(customUrl && !customUrl.includes("dicebear.com") && !customUrl.startsWith("openbot://")),
  );
  let selectedCategory = $state("All");

  /**
   * The OpenBot character state.
   *
   * The style is the durable marker; the URL carries the colour. Both are
   * read here so reopening the picker on an OpenBot agent starts on the same
   * character instead of the name-derived default.
   */
  // svelte-ignore state_referenced_locally
  const initialOpenBot =
    openBotDefinition(parseOpenBotStyle(style)) ??
    openBotDefinition(parseOpenBotUrl(customUrl)?.id) ??
    openBotDefinitionForName(seed || "Agent");
  let openBotId = $state(initialOpenBot.id);
  // svelte-ignore state_referenced_locally
  let openBotColour = $state(
    parseOpenBotUrl(customUrl)?.colour ?? openBotColourForName(seed || "Agent"),
  );
  let openBotOpen = $state(false);
  /**
   * Whether the current selection is an OpenBot character.
   *
   * Kept explicitly rather than re-derived from `selectedStyle` plus the
   * custom URL: choosing a DiceBear style after an OpenBot leaves the old
   * `openbot://` URL in the custom field, and treating that as still-selected
   * would silently send the wrong style on apply.
   */
  // svelte-ignore state_referenced_locally
  let openBotMode = $state(Boolean(parseOpenBotStyle(style) || parseOpenBotUrl(customUrl)));
  const openBotSelected = $derived(openBotMode);
  const openBotCharacter = $derived(openBotDefinition(openBotId) ?? initialOpenBot);
  const openBotPreview = $derived(
    openBotStillUrl(openBotCharacter, openBotColour, "idle"),
  );

  /**
   * The studio is open.
   *
   * Only for the locally drawn face. A DiceBear style carries its animation
   * inside its own SVG and offers no expression to compose, so there would be
   * nothing here to arrange — the state strip would just be the same six
   * pictures the picker already shows.
   */
  let studioOpen = $state(false);
  // What the studio last applied, so the hero preview shows the same face.
  let previewSilhouette = $state<string | null>(null);
  let previewExpression = $state<any>(null);
  let previewColour = $state<ColourId | null>(null);

  /**
   * Whether to ask for the animation.
   *
   * Three inputs, in order: an explicit choice by the user, then the stored
   * preference, then whatever the style does on its own. The style's own default
   * is the right answer for someone who has never opened this dialog, which is
   * nearly everyone.
   *
   * Persisted rather than held, because "stop everything moving" is a decision
   * about how the app looks and should survive a restart — and because the
   * alternative is a user turning motion off in the system settings and still
   * seeing a fleet of faces that ignore it.
   */
   const ANIMATE_KEY = "raven-avatar-animate";
  let animate = $state(true);
  $effect(() => {
    const stored = localStorage.getItem(ANIMATE_KEY);
    if (stored !== null) animate = stored !== "off";
  });
  function setAnimate(next: boolean) {
    animate = next;
    localStorage.setItem(ANIMATE_KEY, next ? "on" : "off");
  }

  /**
   * Follow a style change from the owner, not the value we already had.
   *
   * Comparing the prop with `selectedStyle` looks equivalent but is not: after a
   * local pick, the parent's prop is still the old value until it saves and
   * re-renders, so that comparison would quietly flip the selection back. Track
   * the last prop we acted on instead.
   */
  // svelte-ignore state_referenced_locally
  let lastStyleProp = style;
  $effect(() => {
    const nextStyle = style;
    if (nextStyle === lastStyleProp) return;
    lastStyleProp = nextStyle;
    if (!nextStyle || useCustom) return;
    selectedStyle = nextStyle;
    const id = parseOpenBotStyle(nextStyle);
    if (id && openBotDefinition(id)) {
      openBotId = id;
      openBotMode = true;
    } else {
      openBotMode = false;
    }
  });

  $effect(() => {
    if (seed && seed !== previewSeed) {
      previewSeed = seed;
    }
  });

  $effect(() => {
    if (customUrl !== null && customUrl !== undefined) {
      customImageUrl = customUrl;
      const parts = parseOpenBotUrl(customUrl);
      useCustom = Boolean(
        customUrl && !customUrl.includes("dicebear.com") && !customUrl.startsWith("openbot://"),
      );
      if (parts) {
        openBotMode = true;
        openBotId = openBotDefinition(parts.id)?.id ?? openBotId;
        if (parts.colour) openBotColour = parts.colour;
      }
    }
  });

  /** The hero goes through RavenAvatar, so DiceBear, custom and OpenBot agree. */
  const previewStyle = $derived(
    useCustom
      ? DEFAULT_AVATAR_STYLE
      : openBotSelected
        ? openBotStyleId(openBotCharacter.id)
        : selectedStyle,
  );
  /**
   * The stored OpenBot URL for the current character, including its render
   * choices. Rebuild only when the id does not match (a new character, or no
   * stored URL yet), so the hero and studio keep depth/follow/finish.
   */
  const openBotCurrentUrl = $derived(
    parseOpenBotUrl(customImageUrl)?.id.toLowerCase() === openBotCharacter.id
      ? customImageUrl.trim()
      : null,
  );
  const previewImage = $derived(
    useCustom
      ? customImageUrl.trim() || null
      : openBotSelected
        ? openBotCurrentUrl ?? openBotUrl(openBotCharacter.id, openBotColour)
        : null,
  );

  const allStyles = dicebearStyles();

  const categories = ["All", ...new Set(allStyles.map((s) => s.category))];

  let filteredStyles = $derived(
    selectedCategory === "All"
      ? allStyles
      : allStyles.filter((s) => s.category === selectedCategory)
  );

  function pick(s: string) {
    if (parseOpenBotStyle(s)) {
      openOpenBot(s);
      return;
    }
    openBotMode = false;
    openBotOpen = false;
    selectedStyle = s;
    useCustom = false;
    // Picking the drawn face is a request to design it, so the studio opens.
    if (isNativeAvatarStyle(s)) studioOpen = true;
    // "" for a style drawn by this app rather than fetched. The avatar falls
    // back to the style slug, so an empty URL is a valid thing to store — and
    // storing a stale remote URL for a local style is not.
    onSelect(getDiceBearUrl(previewSeed || "Agent", s), s);
  }

  function openStudio() {
    studioOpen = true;
  }

  /** Open the OpenBot studio on the current character, without committing a look. */
  function openOpenBot(styleValue?: string | null) {
    const id = parseOpenBotStyle(styleValue) ?? openBotId;
    if (openBotDefinition(id)) openBotId = id;
    openBotOpen = true;
    openBotMode = true;
    studioOpen = false;
    useCustom = false;
    selectedStyle = openBotStyleId(openBotId);
  }

  /** Apply is the commit point for an OpenBot character. */
  function applyOpenBot(url: string, styleValue: string) {
    const id = parseOpenBotStyle(styleValue) ?? openBotId;
    const parts = parseOpenBotUrl(url);
    if (openBotDefinition(id)) openBotId = id;
    if (parts?.colour) openBotColour = parts.colour;
    selectedStyle = openBotStyleId(openBotId);
    useCustom = false;
    openBotMode = true;
    customImageUrl = url;
    openBotOpen = false;
    onSelect(url, openBotStyleId(openBotId));
  }

  function onCustomInput() {
    const value = customImageUrl.trim();
    const parts = parseOpenBotUrl(value);
    if (parts) {
      useCustom = false;
      openBotOpen = false;
      openBotMode = true;
      const id = openBotDefinition(parts.id)?.id;
      if (id) openBotId = id;
      if (parts.colour) openBotColour = parts.colour;
      selectedStyle = openBotStyleId(openBotId);
      return;
    }
    openBotMode = false;
    useCustom = Boolean(value && !value.includes("dicebear.com"));
  }

  /** The custom-image toggle must not leave an `openbot://` URL in that field. */
  function toggleCustom() {
    if (!useCustom && parseOpenBotUrl(customImageUrl)) {
      customImageUrl = "";
      openBotMode = false;
    }
    useCustom = !useCustom;
  }

  function randomizeSeed() {
    const randomSeeds = [
 "Apollo", "Nexus", "Quantum", "Cyber", "Valkyrie", "Aegis", "Titan", "Specter",
 "Vortex", "Atlas", "Echo", "Cipher", "Phoenix", "Helios", "Shadow", "Vector",
 "Krypton", "Apex", "Chronos", "Sentinel"
    ];
    previewSeed = randomSeeds[Math.floor(Math.random() * randomSeeds.length)] + "-" + Math.floor(Math.random() * 900 + 100);
    if (!useCustom && !openBotSelected) {
      const url = getDiceBearUrl(previewSeed, selectedStyle);
      onSelect(url, selectedStyle);
    }
  }

  function confirm() {
    if (openBotSelected && !useCustom) {
      onSelect(
        openBotCurrentUrl ?? openBotUrl(openBotCharacter.id, openBotColour),
        openBotStyleId(openBotCharacter.id),
      );
      return;
    }
    if (useCustom && customImageUrl.trim()) {
      onSelect(customImageUrl.trim(), "custom");
    } else {
      const url = getDiceBearUrl(previewSeed || "Agent", selectedStyle);
      onSelect(url, selectedStyle);
    }
  }
</script>

<div class="flex flex-col gap-4 p-1 min-w-0 text-[var(--text-primary)]">
  <!-- Avatar Preview Hero Area -->
  <div class="flex flex-col sm:flex-row items-center justify-between gap-4 p-4 rounded-2xl bg-[var(--surface-1)] border border-[var(--hairline)] relative overflow-hidden shadow-inner">

    <!-- Live Avatar Circle -->
    <div class="flex items-center gap-4 relative z-10">
      <div class="relative group shrink-0">
        <div class="size-20 rounded-2xl p-1 ring-2 ring-[var(--brand)]/60 transition-all duration-300 group-hover:ring-[var(--brand)] group-hover:scale-105 bg-[var(--surface-2)] overflow-hidden">
          <RavenAvatar
            name={previewSeed || "Agent"}
            style={previewStyle}
            imageUrl={previewImage}
            animated={animate}
            class="size-full rounded-xl"
            decorative
          />
        </div>
        <div class="absolute -bottom-1.5 -right-1.5 size-6 rounded-full bg-[var(--brand)] text-[var(--text-on-light)] flex items-center justify-center shadow-lg ring-2 ring-[var(--surface-1)]">
          <Sparkles class="size-3" />
        </div>
      </div>

      <div class="flex flex-col">
        <div class="flex items-center gap-2">
          <span class="font-bold text-sm text-[var(--text-primary)]">{t("avatar.preview")}</span>
          {#if openBotSelected && !useCustom}
            <span class="text-[11px] font-mono px-2 py-0.5 rounded bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/30">
              {openBotCharacter.name}
            </span>
          {:else if !useCustom}
            <span class="text-[11px] font-mono px-2 py-0.5 rounded bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/30">
              {selectedStyle}
            </span>
          {:else}
            <span class="text-[11px] font-mono px-2 py-0.5 rounded bg-success/15 text-success border border-success/30">
              custom url
            </span>
          {/if}
        </div>
        <span class="text-xs text-[var(--text-tertiary)] mt-0.5">
          {t("avatar.seedLabel")} <span class="font-mono text-[var(--brand-text)]">{previewSeed || t("ui.fallbackAgent")}</span>
        </span>
      </div>
    </div>

    <!-- Quick Randomize Seed Button -->
    <div class="relative z-10">
      <Button
        variant="outline"
        size="sm"
        onclick={randomizeSeed}
        class="h-8 gap-1.5 text-xs bg-[var(--surface-2)] border-[var(--brand)]/30 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)]"
      >
        <Wand2 class="size-3.5 text-[var(--brand-text)]" />
        {t("avatar.randomize")}
      </Button>
    </div>
  </div>

  <!-- Category Filter Chips -->
  <div class="space-y-1.5">
    <div class="flex items-center justify-between">
      <Label class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
        {t("avatar.styleLibrary", { n: allStyles.length })}
      </Label>
    </div>

    <label class="flex items-start gap-2 text-[11px] text-[var(--text-secondary)] cursor-pointer">
      <input
        type="checkbox"
        checked={animate}
        onchange={(e) => setAnimate(e.currentTarget.checked)}
        class="mt-0.5 accent-[var(--brand)]"
      />
      <span>
        <span class="font-medium">{t("avatar.animate")}</span>
        <span class="block text-[11px] text-[var(--text-tertiary)] leading-relaxed mt-0.5">
          {t("avatar.animateHelp")}
        </span>
      </span>
    </label>

    <div class="flex items-center gap-1.5 overflow-x-auto pb-1 no-scrollbar min-w-0">
      {#each categories as cat}
        <button
          type="button"
          class="px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all shrink-0 cursor-pointer {selectedCategory === cat ? 'bg-[var(--brand)] text-[var(--text-on-light)] shadow-sm' : 'bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-on-light)] hover:bg-[var(--surface-3)]'}"
          aria-pressed={selectedCategory === cat}
          onclick={() => (selectedCategory = cat)}
        >
          {cat === "All" ? avatarAllLabel() : avatarCategoryLabel(cat)}
        </button>
      {/each}
    </div>
  </div>

  <!--
    OpenBot characters. One tile, not one canvas per character: the preview is
    a still SVG data URL, and picking the tile opens the studio where all
    definitions and the six states live.
  -->
  <section class="space-y-1.5" aria-label={t("openbot.title")}>
    <div class="flex items-center justify-between gap-2">
      <span class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
        {t("openbot.title")}
      </span>
      <span class="text-[11px] text-[var(--text-muted)]">{t("openbot.character")} · {OPENBOT_GALLERY.length}</span>
    </div>
    <button
      type="button"
      class={cn(
        "w-full flex items-center gap-3 rounded-xl border p-2.5 text-left transition-all cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60",
        openBotSelected && !useCustom
          ? "border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/60"
          : "border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)]/40 hover:bg-[var(--surface-2)]",
      )}
      aria-pressed={openBotSelected && !useCustom}
      onclick={() => openOpenBot()}
    >
      <span class="size-10 shrink-0 rounded-full overflow-hidden bg-[var(--surface-3)] ring-1 ring-border/50">
        <img src={openBotPreview} alt="" loading="lazy" decoding="async" class="size-full object-contain" />
      </span>
      <span class="min-w-0 flex-1">
        <span class="block text-xs font-semibold text-[var(--text-primary)] truncate">{openBotCharacter.name}</span>
        <span class="block text-[11px] text-[var(--text-tertiary)] leading-snug">{t("openbot.galleryNote")}</span>
      </span>
      <Dices class="size-4 shrink-0 text-[var(--brand-text)]" />
    </button>
  </section>

  {#if openBotOpen}
    <OpenBotStudio
      seed={previewSeed || "Agent"}
      definitionId={openBotId}
      colour={openBotColour}
      imageUrl={openBotCurrentUrl}
      animated={animate}
      onApply={applyOpenBot}
      onClose={() => (openBotOpen = false)}
    />
  {/if}

  <!--
    The native studio. Opened by picking the drawn face, or by the button —
    only appears for that style, because there is nothing to arrange for a
    remote one.
  -->
  {#if studioOpen || isNativeAvatarStyle(selectedStyle)}
    <div class="flex items-center justify-between gap-2">
      <span class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
        {t("studio.title")}
      </span>
      {#if !studioOpen}
        <Button size="sm" variant="outline" class="h-7 gap-1.5 text-[11px]" onclick={openStudio}>
          <Dices class="size-3" /> {t("studio.open")}
        </Button>
      {/if}
    </div>
  {/if}

  {#if studioOpen}
    <AvatarStudio
      seed={previewSeed || "Agent"}
      expression={previewExpression}
      silhouette={previewSilhouette ?? undefined}
      colour={previewColour ?? undefined}
      onApply={(sil, expr, col) => {
        previewSilhouette = sil;
        previewExpression = expr;
        previewColour = col;
        selectedStyle = "raven-native";
        studioOpen = false;
        onSelect("", "raven-native");
      }}
      onClose={() => (studioOpen = false)}
    />
  {/if}

  <!-- Style Presets Grid -->
  <div class="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-6 gap-2 max-h-40 overflow-y-auto no-scrollbar pr-1 min-w-0" role="radiogroup" aria-label={t("avatar.styleLibrary", { n: allStyles.length })}>
    {#each filteredStyles as s}
      {@const isSelected = selectedStyle === s.value && !useCustom}
      <button
        type="button"
        class={cn(
 "group relative rounded-xl border p-2 transition-all text-center flex flex-col items-center gap-1.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 cursor-pointer",
          isSelected
            ? "border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/60"
            : "border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)]/40 hover:bg-[var(--surface-2)]"
        )}
        role="radio"
        aria-checked={isSelected}
        onclick={() => pick(s.value)}
        title={avatarStyleDescription(s.value)}
      >
        <div class="relative size-10 rounded-full overflow-hidden bg-[var(--surface-3)] ring-1 ring-border/50 transition-transform group-hover:scale-105">
          <!--
            Through the component rather than a bare `img`, because one style is
            drawn locally and has no URL, and because the component is what the
            agent list will actually render — a preview from anything else is a
            preview of something that is not what you get.
          -->
          <RavenAvatar
            name={previewSeed || "Agent"}
            style={s.value}
            colour={previewColour ?? undefined}
            animated={animate}
            class="size-full"
            decorative
          />
          {#if s.animated}
            <!--
              A still thumbnail of a moving avatar undersells it, so the tile says
              so, and says how fast. The cadence is the reason to pick one over
              another, so it belongs where the choice is made.
            -->
            <span
              class="absolute bottom-0 right-0 size-3.5 rounded-full bg-[var(--surface-0)]/85 ring-1 ring-[var(--hairline)] flex items-center justify-center"
              title={t("avatar.animatesEvery", { s: s.loopSeconds ?? 0 })}
            >
              <span class="size-1 rounded-full bg-[var(--brand-text)] animate-pulse"></span>
            </span>
          {/if}
        </div>
        <span class="text-[11px] font-medium text-[var(--text-secondary)] truncate w-full group-hover:text-[var(--text-primary)]">
          {s.label}
        </span>
        {#if isSelected}
          <div class="absolute top-1 right-1 size-3.5 rounded-full bg-[var(--brand)] text-[var(--text-on-light)] flex items-center justify-center shadow">
            <Check class="size-2.5 stroke-[3]" />
          </div>
        {/if}
      </button>
    {/each}
  </div>

  <!-- Custom Seed & Direct Image URL Controls -->
  <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1 border-t border-[var(--hairline)]">
    <!-- Seed Customizer -->
    <div class="space-y-1">
      <Label for="avatar-seed" class="text-[11px] font-bold text-[var(--text-tertiary)] uppercase tracking-wider">
        {t("avatar.seedName")}
      </Label>
      <Input
        id="avatar-seed"
        bind:value={previewSeed}
        placeholder={t("avatar.seedPh")}
        class="h-8 font-mono text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
        oninput={() => {
          if (!useCustom && !openBotSelected) onSelect(getDiceBearUrl(previewSeed || "Agent", selectedStyle), selectedStyle);
        }}
      />
    </div>

    <!-- Custom URL Input -->
    <div class="space-y-1">
      <div class="flex items-center justify-between">
        <Label for="custom-url" class="text-[11px] font-bold text-[var(--text-tertiary)] uppercase tracking-wider">
          {t("avatar.customUrl")}
        </Label>
        {#if useCustom}
          <span class="text-[11px] text-[var(--brand-text)] font-mono">{t("avatar.active")}</span>
        {/if}
      </div>
      <div class="flex gap-1.5">
        <Input
          id="custom-url"
          bind:value={customImageUrl}
          placeholder="https://.../photo.png"
          class="h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] flex-1"
          oninput={onCustomInput}
        />
        <Button
          variant={useCustom ? "default" : "outline"}
          size="sm"
          class="h-8 px-2.5 text-xs shrink-0 {useCustom ? 'bg-[var(--brand)] text-[var(--text-on-light)]' : 'bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)]'}"
          aria-label={t("avatar.customUrl")}
          aria-pressed={useCustom}
          onclick={toggleCustom}
        >
          <Image class="size-3.5" />
        </Button>
      </div>
    </div>
  </div>

  <!-- Confirm / Save Selection -->
  <Button
    class="w-full h-9 gap-2 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium shadow-md  mt-1"
    onclick={confirm}
  >
    <Check class="size-4" />
    {t("avatar.useSelected")}
  </Button>
</div>
