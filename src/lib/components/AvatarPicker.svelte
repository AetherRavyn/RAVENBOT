<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { getDiceBearUrl, dicebearStyles } from "$lib/utils";
  import { cn } from "$lib/utils.js";
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

<div class="flex flex-col gap-4 p-1 text-zinc-100">
  <!-- Avatar Preview Hero Area -->
  <div class="flex flex-col sm:flex-row items-center justify-between gap-4 p-4 rounded-2xl bg-[#0e0e18]/90 border border-purple-500/25 relative overflow-hidden shadow-inner">
    <div class="absolute inset-0 bg-gradient-to-r from-purple-900/15 via-transparent to-indigo-900/10 pointer-events-none"></div>

    <!-- Live Avatar Circle -->
    <div class="flex items-center gap-4 relative z-10">
      <div class="relative group shrink-0">
        <div class="size-20 rounded-2xl p-1 ring-2 ring-purple-500/60 shadow-[0_0_30px_rgba(147,51,234,0.35)] transition-all duration-300 group-hover:ring-purple-400 group-hover:scale-105 bg-[#12101e] overflow-hidden">
          <img
            src={previewUrl}
            alt="Avatar preview"
            class="size-full rounded-xl object-cover"
            loading="eager"
          />
        </div>
        <div class="absolute -bottom-1.5 -right-1.5 size-6 rounded-full bg-purple-600 text-white flex items-center justify-center shadow-lg ring-2 ring-[#0e0e18]">
          <Sparkles class="size-3" />
        </div>
      </div>

      <div class="flex flex-col">
        <div class="flex items-center gap-2">
          <span class="font-bold text-sm text-white">Agent Identity Preview</span>
          {#if !useCustom}
            <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-purple-950/80 text-purple-300 border border-purple-500/30">
              {selectedStyle}
            </span>
          {:else}
            <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-emerald-950/80 text-emerald-300 border border-emerald-500/30">
              custom url
            </span>
          {/if}
        </div>
        <span class="text-xs text-zinc-400 mt-0.5">
          Seed: <span class="font-mono text-purple-300">{previewSeed || "Agent"}</span>
        </span>
      </div>
    </div>

    <!-- Quick Randomize Seed Button -->
    <div class="relative z-10">
      <Button
        variant="outline"
        size="sm"
        onclick={randomizeSeed}
        class="h-8 gap-1.5 text-xs bg-[#151522] border-purple-500/30 text-purple-200 hover:bg-purple-950/50 hover:text-white hover:border-purple-400"
      >
        <Wand2 class="size-3.5 text-purple-400" />
        Randomize Look
      </Button>
    </div>
  </div>

  <!-- Category Filter Chips -->
  <div class="space-y-1.5">
    <div class="flex items-center justify-between">
      <Label class="text-[11px] font-bold uppercase tracking-wider text-zinc-400">
        DiceBear 9.x Style Library ({allStyles.length} Styles)
      </Label>
    </div>

    <div class="flex items-center gap-1.5 overflow-x-auto pb-1 no-scrollbar">
      {#each categories as cat}
        <button
          type="button"
          class="px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all shrink-0 cursor-pointer {selectedCategory === cat ? 'bg-purple-600 text-white shadow-sm' : 'bg-[#12121e] border border-[#232336] text-zinc-400 hover:text-zinc-200 hover:bg-[#181827]'}"
          onclick={() => (selectedCategory = cat)}
        >
          {cat}
        </button>
      {/each}
    </div>
  </div>

  <!-- Style Presets Grid -->
  <div class="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-6 gap-2 max-h-40 overflow-y-auto pr-1">
    {#each filteredStyles as s}
      {@const isSelected = selectedStyle === s.value && !useCustom}
      <button
        type="button"
        class={cn(
          "group relative rounded-xl border p-2 transition-all text-center flex flex-col items-center gap-1.5 focus:outline-none cursor-pointer",
          isSelected
            ? "border-purple-500 bg-purple-950/40 shadow-[0_0_15px_rgba(147,51,234,0.25)] ring-1 ring-purple-500/60"
            : "border-[#1e1e2d] bg-[#0d0d16] hover:border-purple-500/40 hover:bg-[#131320]"
        )}
        onclick={() => pick(s.value)}
        title={s.description}
      >
        <div class="relative size-10 rounded-full overflow-hidden bg-[#161624] ring-1 ring-border/50 transition-transform group-hover:scale-105">
          <img
            src={getDiceBearUrl(previewSeed || "Agent", s.value)}
            alt={s.label}
            class="size-full object-cover"
            loading="lazy"
          />
        </div>
        <span class="text-[10px] font-medium text-zinc-300 truncate w-full group-hover:text-white">
          {s.label}
        </span>
        {#if isSelected}
          <div class="absolute top-1 right-1 size-3.5 rounded-full bg-purple-600 text-white flex items-center justify-center shadow">
            <Check class="size-2.5 stroke-[3]" />
          </div>
        {/if}
      </button>
    {/each}
  </div>

  <!-- Custom Seed & Direct Image URL Controls -->
  <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1 border-t border-[#1e1e2d]">
    <!-- Seed Customizer -->
    <div class="space-y-1">
      <Label for="avatar-seed" class="text-[11px] font-bold text-zinc-400 uppercase tracking-wider">
        Avatar Seed Name
      </Label>
      <Input
        id="avatar-seed"
        bind:value={previewSeed}
        placeholder="e.g. Chief, Nova, Architect..."
        class="h-8 font-mono text-xs bg-[#141420] border-[#252538] text-zinc-200"
        oninput={() => {
          if (!useCustom) onSelect(getDiceBearUrl(previewSeed || "Agent", selectedStyle), selectedStyle);
        }}
      />
    </div>

    <!-- Custom URL Input -->
    <div class="space-y-1">
      <div class="flex items-center justify-between">
        <Label for="custom-url" class="text-[11px] font-bold text-zinc-400 uppercase tracking-wider">
          Custom Image URL
        </Label>
        {#if useCustom}
          <span class="text-[10px] text-purple-400 font-mono">Active</span>
        {/if}
      </div>
      <div class="flex gap-1.5">
        <Input
          id="custom-url"
          bind:value={customImageUrl}
          placeholder="https://.../photo.png"
          class="h-8 text-xs bg-[#141420] border-[#252538] text-zinc-200 flex-1"
          oninput={() => (useCustom = Boolean(customImageUrl.trim()))}
        />
        <Button
          variant={useCustom ? "default" : "outline"}
          size="sm"
          class="h-8 px-2.5 text-xs shrink-0 {useCustom ? 'bg-purple-600 text-white' : 'bg-[#181826] border-[#2b2b3e] text-zinc-300'}"
          onclick={() => (useCustom = !useCustom)}
        >
          <Image class="size-3.5" />
        </Button>
      </div>
    </div>
  </div>

  <!-- Confirm / Save Selection -->
  <Button
    class="w-full h-9 gap-2 bg-purple-600 hover:bg-purple-500 text-white font-medium shadow-md shadow-purple-950/50 mt-1"
    onclick={confirm}
  >
    <Check class="size-4" />
    Use Selected Avatar
  </Button>
</div>
