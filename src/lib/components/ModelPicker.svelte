<script lang="ts">
  import { Input } from "$lib/components/ui/input";
  import { Loader2, Check, RefreshCw, Key } from "@lucide/svelte";
  import type { CatalogProvider, CatalogModel } from "$lib/model-catalog";

  interface Props {
    providers: CatalogProvider[];
    provider: string;
    model: string;
    models: CatalogModel[];
    /** Provider ids that already have credentials saved. */
    configured?: string[];
    loading?: boolean;
    /** Generic load error for the selected provider. */
    error?: string | null;
    /** Optional human reason a provider is unavailable (e.g. "Ollama not reachable"). */
    reasonFor?: (provider: CatalogProvider) => string | null;
    /** Optional note shown at the bottom of the models pane. */
    footerNote?: string;
    onSelectProvider: (id: string) => void;
    onSelectModel: (id: string) => void;
    onRefresh?: () => void;
    onAddKey?: () => void;
    heightClass?: string;
  }

  let {
    providers,
    provider,
    model,
    models,
    configured = [],
    loading = false,
    error = null,
    reasonFor,
    footerNote,
    onSelectProvider,
    onSelectModel,
    onRefresh,
    onAddKey,
    heightClass = "max-h-[23rem]",
  }: Props = $props();

  let providerSearch = $state("");
  let modelSearch = $state("");

  let activeProvider = $derived(providers.find((p) => p.id === provider));
  let visibleProviders = $derived(
    providerSearch.trim()
      ? providers.filter(
          (p) =>
            p.name.toLowerCase().includes(providerSearch.toLowerCase()) ||
            p.id.toLowerCase().includes(providerSearch.toLowerCase()),
        )
      : providers,
  );
  let visibleModels = $derived(
    modelSearch.trim()
      ? models.filter(
          (m) =>
            m.name.toLowerCase().includes(modelSearch.toLowerCase()) ||
            m.id.toLowerCase().includes(modelSearch.toLowerCase()),
        )
      : models,
  );

  function isReady(p: CatalogProvider): boolean {
    return p.keyless || configured.includes(p.id);
  }
  function reason(p: CatalogProvider): string | null {
    if (reasonFor) return reasonFor(p);
    if (!isReady(p)) return "API key needed";
    return null;
  }
  function needsKey(p: CatalogProvider | undefined): boolean {
    return Boolean(p && !p.keyless && !configured.includes(p.id));
  }
</script>

<div class="grid grid-cols-[10.5rem_1fr] gap-2 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] overflow-hidden">
  <!-- Provider rail -->
  <div class="border-r border-[var(--hairline)] bg-[var(--surface-2)]lack/20 p-1.5 space-y-1 {heightClass} overflow-y-auto">
    <div class="relative pb-1">
      <Input bind:value={providerSearch} placeholder="Search…" class="h-7 pl-7 text-[11px]" />
      <svg class="absolute left-2 top-1/2 -translate-y-1/2 size-3 text-[var(--text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>
    </div>
    {#each visibleProviders as p (p.id)}
      {@const ready = isReady(p)}
      {@const why = reason(p)}
      <button
        type="button"
        onclick={() => onSelectProvider(p.id)}
        title={`${p.description}${why ? ` · ${why}` : ""}`}
        class="w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-left transition-colors cursor-pointer {provider === p.id ? 'bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/40' : 'hover:bg-[var(--surface-3)]'}"
      >
        <span class="text-sm shrink-0">{p.icon || "🔌"}</span>
        <span class="min-w-0 flex-1">
          <span class="block text-[11px] font-semibold text-[var(--text-primary)] truncate">{p.name}</span>
          <span class="block text-[9px] font-mono {ready ? 'text-success/80' : 'text-warning/80'}">
            {p.keyless ? "local" : ready ? "ready" : "key needed"}
          </span>
        </span>
        <span class="size-1.5 rounded-full shrink-0 {ready ? 'bg-success' : 'bg-warning'}"></span>
      </button>
    {:else}
      <div class="px-2 py-4 text-center text-[11px] text-[var(--text-muted)]">No matches</div>
    {/each}
  </div>

  <!-- Models pane -->
  <div class="min-w-0 flex flex-col {heightClass}">
    <div class="flex items-center justify-between gap-2 px-3 pt-2.5">
      <span class="text-xs font-bold text-white truncate">{activeProvider?.name ?? "Provider"}</span>
      <span class="flex items-center gap-2 shrink-0">
        {#if onRefresh}
          <button type="button" onclick={onRefresh} class="text-[10px] text-[var(--brand)] hover:opacity-80 cursor-pointer flex items-center gap-1">
            {#if loading}<Loader2 class="size-3 animate-spin" />{/if}
            Refresh
          </button>
        {/if}
        {#if needsKey(activeProvider) && onAddKey}
          <button type="button" onclick={onAddKey} class="text-[10px] font-bold text-warning bg-warning/15 border border-warning/30 rounded-md px-2 py-0.5 hover:bg-warning/25 cursor-pointer flex items-center gap-1">
            <Key class="size-3" /> Add key
          </button>
        {/if}
      </span>
    </div>
    <p class="px-3 text-[10px] text-[var(--text-muted)] truncate">{activeProvider?.description}</p>

    {#if error}
      <div class="mx-3 mt-2 text-[10px] rounded-lg px-2 py-1.5 text-warning bg-warning/30 border border-warning/20">
        {error}
      </div>
    {/if}

    {#if models.length > 6}
      <div class="relative px-3 pt-2">
        <Input bind:value={modelSearch} placeholder="Search {models.length} models…" class="h-7 pl-7 text-[11px]" />
        <svg class="absolute left-5 top-1/2 -translate-y-1/2 size-3 text-[var(--text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>
      </div>
    {/if}

    <div class="flex-1 overflow-y-auto px-1.5 py-1.5 mt-1 space-y-0.5">
      {#if loading && models.length === 0}
        <div class="flex items-center gap-2 px-2 py-3 text-[11px] text-[var(--text-tertiary)]">
          <Loader2 class="size-3.5 animate-spin" /> Fetching models…
        </div>
      {:else}
        {#each visibleModels as m}
          {@const isCurrent = model === m.id}
          <button
            type="button"
            onclick={() => onSelectModel(m.id)}
            title={m.id}
            class="w-full flex items-center justify-between gap-2 px-2 py-1.5 rounded-lg text-left transition-colors cursor-pointer {isCurrent ? 'bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/40' : 'hover:bg-[var(--surface-3)]'}"
          >
            <span class="text-[11px] text-[var(--text-secondary)] truncate">{m.name}</span>
            <span class="flex items-center gap-1 shrink-0">
              {#if m.is_free}<span class="text-[9px] text-success bg-success/10 border border-success/20 px-1 rounded">Free</span>{/if}
              {#if m.supports_vision}<span class="text-[9px] text-[var(--brand-text)] bg-[var(--brand-soft)] border border-[var(--brand)]/20 px-1 rounded">Vision</span>{/if}
              {#if isCurrent}<Check class="size-3 text-[var(--brand)]" />{/if}
            </span>
          </button>
        {:else}
          <div class="px-2 py-4 text-center text-[11px] text-[var(--text-muted)]">No models match "{modelSearch}".</div>
        {/each}
      {/if}
    </div>

    {#if footerNote}
      <p class="px-3 py-1.5 border-t border-[var(--hairline)] text-[10px] text-[var(--text-muted)]">{footerNote}</p>
    {/if}
  </div>
</div>
