<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { getDiceBearUrl, dicebearStyles } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import { t } from "$lib/i18n";
  import { Sparkles, RefreshCw, Image, Wand2, Check, Palette } from "@lucide/svelte";

  interface Props {
    seed: string;
    style?: string;
    customUrl?: string | null;
    onSelect: (url: string, style: string) => void;
  }

  let { seed = "Agent", style = "bottts", customUrl = null, onSelect }: Props = $props();

  // svelte-ignore state_referenced_locally
  let selectedStyle = $state(style || "bottts");
  // svelte-ignore state_referenced_locally
  let previewSeed = $state(seed || "Agent");
  // svelte-ignore state_referenced_locally
  let customImageUrl = $state(customUrl || "");
  // svelte-ignore state_referenced_locally
  let useCustom = $state(Boolean(customUrl && !customUrl.includes("dicebear.com")));
  let selectedCategory = $state("All");

  $effect(() => {
    if (style && style !== selectedStyle && !useCustom) {
      selectedStyle = style;
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
      useCustom = Boolean(customUrl && !customUrl.includes("dicebear.com"));
    }
  });

  let previewUrl = $derived(
    useCustom && customImageUrl.trim()
      ? customImageUrl.trim()
      : getDiceBearUrl(previewSeed || "Agent", selectedStyle)
  );

  const allStyles = dicebearStyles();

  const categories = ["All", "Robots & AI", "Characters", "Modern", "Fantasy", "Doodles", "Retro", "Geometric", "Playful"];

  let filteredStyles = $derived(
    selectedCategory === "All"
      ? allStyles
      : allStyles.filter((s) => s.category === selectedCategory)
  );

  function pick(s: string) {
    selectedStyle = s;
    useCustom = false;
    const url = getDiceBearUrl(previewSeed || "Agent", s);
    onSelect(url, s);
  }

  function randomizeSeed() {
    const randomSeeds = [
 "Apollo", "Nexus", "Quantum", "Cyber", "Valkyrie", "Aegis", "Titan", "Specter",
 "Vortex", "Atlas", "Echo", "Cipher", "Phoenix", "Helios", "Shadow", "Vector",
 "Krypton", "Apex", "Chronos", "Sentinel"
    ];
    previewSeed = randomSeeds[Math.floor(Math.random() * randomSeeds.length)] + "-" + Math.floor(Math.random() * 900 + 100);
    if (!useCustom) {
      const url = getDiceBearUrl(previewSeed, selectedStyle);
      onSelect(url, selectedStyle);
    }
  }

  function confirm() {
    if (useCustom && customImageUrl.trim()) {
      onSelect(customImageUrl.trim(), "custom");
    } else {
      const url = getDiceBearUrl(previewSeed || "Agent", selectedStyle);
      onSelect(url, selectedStyle);
    }
  }
</script>

<div class="flex flex-col gap-4 p-1 text-[var(--text-primary)]">
  <!-- Avatar Preview Hero Area -->
  <div class="flex flex-col sm:flex-row items-center justify-between gap-4 p-4 rounded-2xl bg-[var(--surface-1)] border border-[var(--hairline)] relative overflow-hidden shadow-inner">

    <!-- Live Avatar Circle -->
    <div class="flex items-center gap-4 relative z-10">
      <div class="relative group shrink-0">
        <div class="size-20 rounded-2xl p-1 ring-2 ring-[var(--brand)]/60 transition-all duration-300 group-hover:ring-[var(--brand)] group-hover:scale-105 bg-[var(--surface-2)] overflow-hidden">
          <img
            src={previewUrl}
            alt={t("avatar.previewAlt")}
            class="size-full rounded-xl object-cover"
            loading="eager"
          />
        </div>
        <div class="absolute -bottom-1.5 -right-1.5 size-6 rounded-full bg-[var(--brand)] text-[var(--text-on-light)] flex items-center justify-center shadow-lg ring-2 ring-[var(--surface-1)]">
          <Sparkles class="size-3" />
        </div>
      </div>

      <div class="flex flex-col">
        <div class="flex items-center gap-2">
          <span class="font-bold text-sm text-[var(--text-primary)]">{t("avatar.preview")}</span>
          {#if !useCustom}
            <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/30">
              {selectedStyle}
            </span>
          {:else}
            <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-success/15 text-success border border-success/30">
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

    <div class="flex items-center gap-1.5 overflow-x-auto pb-1 no-scrollbar">
      {#each categories as cat}
        <button
          type="button"
          class="px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all shrink-0 cursor-pointer {selectedCategory === cat ? 'bg-[var(--brand)] text-[var(--text-on-light)] shadow-sm' : 'bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-on-light)] hover:bg-[var(--surface-3)]'}"
          aria-pressed={selectedCategory === cat}
          onclick={() => (selectedCategory = cat)}
        >
          {cat}
        </button>
      {/each}
    </div>
  </div>

  <!-- Style Presets Grid -->
  <div class="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-6 gap-2 max-h-40 overflow-y-auto no-scrollbar pr-1" role="radiogroup" aria-label={t("avatar.styleLibrary", { n: allStyles.length })}>
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
        title={s.description}
      >
        <div class="relative size-10 rounded-full overflow-hidden bg-[var(--surface-3)] ring-1 ring-border/50 transition-transform group-hover:scale-105">
          <img
            src={getDiceBearUrl(previewSeed || "Agent", s.value)}
            alt={s.label}
            class="size-full object-cover"
            loading="lazy"
          />
        </div>
        <span class="text-[10px] font-medium text-[var(--text-secondary)] truncate w-full group-hover:text-[var(--text-primary)]">
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
          if (!useCustom) onSelect(getDiceBearUrl(previewSeed || "Agent", selectedStyle), selectedStyle);
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
          <span class="text-[10px] text-[var(--brand-text)] font-mono">{t("avatar.active")}</span>
        {/if}
      </div>
      <div class="flex gap-1.5">
        <Input
          id="custom-url"
          bind:value={customImageUrl}
          placeholder="https://.../photo.png"
          class="h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] flex-1"
          oninput={() => (useCustom = Boolean(customImageUrl.trim()))}
        />
        <Button
          variant={useCustom ? "default" : "outline"}
          size="sm"
          class="h-8 px-2.5 text-xs shrink-0 {useCustom ? 'bg-[var(--brand)] text-[var(--text-on-light)]' : 'bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)]'}"
          aria-label={t("avatar.customUrl")}
          aria-pressed={useCustom}
          onclick={() => (useCustom = !useCustom)}
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
