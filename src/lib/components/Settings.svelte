<script lang="ts">
  import { onMount } from "svelte";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl } from "$lib/utils";
  import { getStoredTheme, applyTheme, subscribeTheme, type ThemeDefinition, THEMES } from "$lib/theme";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
  import ConnectorCenter from "$lib/components/ConnectorCenter.svelte";
  import ModelPicker from "$lib/components/ModelPicker.svelte";
  import * as Select from "$lib/components/ui/select";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import {
    getCatalog, getCachedCatalog, type CatalogProvider,
  } from "$lib/model-catalog";
  import {
    Key, Palette, Server, Info, Check, Eye, EyeOff, Save, User,
    Loader2, Wifi, WifiOff, RefreshCw, AlertCircle, Zap, Layers,
    DollarSign, Shield, BookOpen, Settings, ChevronRight, Sparkles,
    Cpu, Globe, Lock, Unlock, Trash2, ExternalLink, X,
  } from "@lucide/svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    bots?: any[];
    initialSection?: string;
  }

  let { open, onClose, bots = [], initialSection = "providers" }: Props = $props();

  // Provider catalog (single source of truth)
  let catalog = $state<CatalogProvider[]>(getCachedCatalog() || []);
  interface ProviderUi {
    key: string; show: boolean; saved: boolean;
    testing: boolean; models: number | null; saveError: string | null;
  }

  function defaultUi(): ProviderUi {
    return { key: "", show: false, saved: false, testing: false, models: null, saveError: null };
  }

  function initUi(providers: CatalogProvider[]): Record<string, ProviderUi> {
    const record: Record<string, ProviderUi> = {};
    for (const p of providers) {
      record[p.id] = defaultUi();
    }
    return record;
  }

  let ui = $state<Record<string, ProviderUi>>(initUi(getCachedCatalog() || []));

  function ensureUi(id: string): ProviderUi {
    if (!ui[id]) {
      ui[id] = defaultUi();
    }
    return ui[id];
  }

  // ── Navigation sections ─────────────────────────────────────────────────────
  interface NavSection {
    id: string;
    label: string;
    icon: any;
    badge?: string;
    color?: string;
  }

  // Legacy section ids from older callers ("keys", "mcp") map to real ones.
  function mapSection(id: string): string {
    if (id === "keys") return "providers";
    if (id === "mcp") return "connectors";
    return id;
  }

  let keyedCount = $derived(catalog.filter((p) => !p.keyless && p.id !== "local").length);
  let providersBadge = $derived(`${keyedCount || 12}`);

  const navSections: NavSection[] = [
    { id: "providers", label: "Providers", icon: Key, color: "text-[var(--brand-text)]" },
    { id: "connectors", label: "Connectors", icon: Layers, badge: "135+", color: "text-[var(--brand-text)]" },
    { id: "models", label: "Model Manager", icon: Cpu, badge: "Live", color: "text-warning" },
    { id: "budgets", label: "Budgets", icon: DollarSign, color: "text-success" },
    { id: "profile", label: "Profile", icon: User, color: "text-[var(--brand-text)]" },
    { id: "themes", label: "Themes", icon: Palette, color: "text-pink-400" },
    { id: "local", label: "Local AI", icon: Server, color: "text-green-400" },
    { id: "about", label: "About", icon: Info, color: "text-[var(--text-tertiary)]" },
  ];

  let activeSection = $state("providers");
  let searchQuery = $state("");

  // ── Provider key state (UI-only; catalog owns names/icons/urls) ────────────

  let configuredProviders = $state<string[]>([]);
  let isLoadingConfigured = $state(false);

  // ── Model discovery state ───────────────────────────────────────────────────
  let discoveredModels = $state<Record<string, any[]>>({});
  let isLoadingModels = $state<Record<string, boolean>>({});
  let totalDiscovered = $state(0);

  // ── Budget state ────────────────────────────────────────────────────────────
  let budgetBotId = $state("");
  let budgetState = $state<any>(null);
  let budgetKind = $state<"unlimited" | "tokens" | "cost">("unlimited");
  let budgetMax = $state<number | null>(null);
  let budgetPeriod = $state("total");

  // ── Profile state ───────────────────────────────────────────────────────────
  let currentTheme = $state<ThemeDefinition>(getStoredTheme());
  let userAvatarUrl = $state<string | null>(null);
  let userAvatarStyle = $state("micah");
  let showUserAvatarPicker = $state(false);
  let userName = $state("");

  // ── Local AI (Ollama endpoint) state ──────────────────────────────────────
  let ollamaUrl = $state("http://localhost:11434");
  let ollamaProbe = $state<any>(null);
  let ollamaProbing = $state(false);
  let ollamaSaving = $state(false);
  let ollamaSaved = $state(false);
  let ollamaError = $state<string | null>(null);

  // ── Global default model state ────────────────────────────────────────────
  let defaultProvider = $state("ollama");
  let defaultModel = $state("llama3.1");
  let defaultSource = $state("settings");
  let defaultSaving = $state(false);
  let defaultSaved = $state(false);
  let defaultModels = $state<any[]>([]);
  let defaultLoadingModels = $state(false);

  onMount(() => {
    return subscribeTheme((t) => {
      currentTheme = t;
    });
  });

  // ── Effects ─────────────────────────────────────────────────────────────────
  $effect(() => {
    if (open) {
      if (initialSection) activeSection = mapSection(initialSection);
      // Catalog first (everything else keys off it), then keys + ollama.
      getCatalog().then((list) => {
        catalog = list;
        for (const p of list) {
          if (!ui[p.id]) ui[p.id] = defaultUi();
        }
        loadConfiguredProviders();
        loadOllamaUrl();
        loadDefaultModel();
      }).catch(() => {
        loadConfiguredProviders();
      });
      if (typeof localStorage !== "undefined") {
        userAvatarUrl = localStorage.getItem("ravenbot_user_avatar");
        const s = localStorage.getItem("ravenbot_user_avatar_style");
        if (s) userAvatarStyle = s;
        userName = localStorage.getItem("ravenbot_user_name") || "";
      }
    }
  });

  $effect(() => {
    if (budgetBotId && open) loadBudget();
  });

  // ── Functions ───────────────────────────────────────────────────────────────
  function hasKey(id: string): boolean {
    const target = id.toLowerCase();
    return configuredProviders.some((p) => {
      const pid = p.toLowerCase();
      return pid === target ||
        (target === "anthropic" && pid === "claude") ||
        (target === "claude" && pid === "anthropic") ||
        (target === "xai" && pid === "grok") ||
        (target === "grok" && pid === "xai");
    });
  }

  async function loadConfiguredProviders() {
    isLoadingConfigured = true;
    try {
      configuredProviders = await invoke<string[]>("get_configured_providers");
    } catch (e) {
      console.error("Failed to load configured providers:", e);
    } finally {
      isLoadingConfigured = false;
    }
  }

  async function saveKey(provider: string) {
    const st = ensureUi(provider);
    if (!st.key.trim()) return;
    st.saveError = null;
    try {
      // One persisted setter (DB + runtime + keychain). The old code called
      // both set_api_key and set_provider_api_key — now the same thing.
      await invoke("set_provider_api_key", { provider, apiKey: st.key.trim() });
      st.saved = true;
      st.key = "";
      if (!configuredProviders.includes(provider)) {
        configuredProviders = [...configuredProviders, provider];
      }
      // Refresh the live model count right away so the badge is truthful.
      testProvider(provider);
      setTimeout(() => { st.saved = false; }, 2500);
    } catch (e: any) {
      st.saveError = String(e);
    }
  }

  async function clearKey(provider: string) {
    const st = ensureUi(provider);
    st.saveError = null;
    try {
      await invoke("set_provider_api_key", { provider, apiKey: "" });
      configuredProviders = configuredProviders.filter((id) => id !== provider);
      st.models = null;
      discoveredModels[provider] = [];
      totalDiscovered = Object.values(discoveredModels).reduce((sum, arr) => sum + arr.length, 0);
    } catch (e: any) {
      st.saveError = String(e);
    }
  }

  async function testProvider(provider: string) {
    const st = ensureUi(provider);
    st.testing = true;
    st.models = null;
    try {
      const models = await invoke<any[]>("fetch_provider_models", { provider });
      st.models = models?.length || 0;
      discoveredModels[provider] = models || [];
      totalDiscovered = Object.values(discoveredModels).reduce((sum, arr) => sum + arr.length, 0);
    } catch (e) {
      st.models = -1;
    } finally {
      st.testing = false;
    }
  }

  async function testAllProviders() {
    // Mark every configured/keyless provider as probing, then fire ONE
    // parallel backend call instead of N sequential spawns.
    const targets = catalog.filter((p) => p.keyless || hasKey(p.id)).map((p) => p.id);
    for (const id of targets) {
      const st = ensureUi(id);
      st.testing = true;
      st.models = null;
    }
    try {
      const all: Record<string, any[]> = await invoke("fetch_all_provider_models");
      for (const id of targets) {
        const models = all[id] || [];
        const st = ensureUi(id);
        st.models = models.length;
        discoveredModels[id] = models;
      }
      totalDiscovered = Object.values(discoveredModels).reduce((sum, arr) => sum + arr.length, 0);
    } catch (e) {
      for (const id of targets) ensureUi(id).models = -1;
    } finally {
      for (const id of targets) ensureUi(id).testing = false;
    }
    if (targets.includes("ollama")) probeOllama(true);
  }

  // ── Local AI (Ollama) ─────────────────────────────────────────────────────
  async function loadOllamaUrl() {
    try {
      ollamaUrl = await invoke<string>("get_ollama_url");
    } catch { /* default stands */ }
  }

  async function saveOllamaUrl() {
    ollamaSaving = true;
    ollamaError = null;
    try {
      ollamaUrl = await invoke<string>("set_ollama_url", { url: ollamaUrl });
      ollamaSaved = true;
      setTimeout(() => { ollamaSaved = false; }, 2500);
      probeOllama(true);
    } catch (e: any) {
      ollamaError = String(e);
    } finally {
      ollamaSaving = false;
    }
  }

  async function probeOllama(silent = false) {
    if (!silent) ollamaProbing = true;
    else ollamaProbing = true;
    ollamaError = null;
    try {
      ollamaProbe = await invoke<any>("probe_ollama");
      const st = ensureUi("ollama");
      st.models = ollamaProbe?.count ?? 0;
    } catch (e: any) {
      ollamaError = String(e);
      ollamaProbe = null;
    } finally {
      ollamaProbing = false;
    }
  }

  // ── Global default model ──────────────────────────────────────────────────
  async function loadDefaultModel() {
    try {
      const res: any = await invoke("get_default_model");
      defaultProvider = res?.provider || "ollama";
      defaultModel = res?.model || "llama3.1";
      defaultSource = res?.source || "settings";
      loadDefaultModels(defaultProvider);
    } catch { /* compiled default stands */ }
  }

  async function loadDefaultModels(provider: string) {
    defaultLoadingModels = true;
    try {
      defaultModels = (await invoke<any[]>("fetch_provider_models", { provider })) || [];
      if (defaultModels.length && !defaultModels.find((m: any) => m.id === defaultModel)) {
        const entry = catalog.find((p) => p.id === provider);
        defaultModel = entry?.default_model || defaultModels[0].id;
      }
    } catch {
      defaultModels = [];
    } finally {
      defaultLoadingModels = false;
    }
  }

  async function saveDefaultModel() {
    defaultSaving = true;
    try {
      await invoke("set_default_model", { provider: defaultProvider, model: defaultModel });
      defaultSource = "settings";
      defaultSaved = true;
      setTimeout(() => { defaultSaved = false; }, 2500);
    } catch (e: any) {
      alert("Failed to save default: " + String(e));
    } finally {
      defaultSaving = false;
    }
  }

  async function loadBudget() {
    if (!budgetBotId) return;
    budgetState = null;
    try {
      const res: any = await invoke("get_bot_budget", { botId: budgetBotId });
      budgetState = res;
      const limit = res?.budget?.limit;
      budgetKind = limit?.kind || "unlimited";
      budgetMax = limit?.kind && limit.kind !== "unlimited" ? limit.max : null;
      budgetPeriod = res?.budget?.period || "total";
    } catch (e) { console.error("Failed to load budget:", e); }
  }

  async function saveBudget() {
    if (!budgetBotId) return;
    try {
      await invoke("set_bot_budget", {
        botId: budgetBotId, kind: budgetKind,
        max: budgetKind === "unlimited" ? 0 : Number(budgetMax || 0),
        period: budgetPeriod,
      });
      await loadBudget();
    } catch (e) { alert("Failed to save budget: " + String(e)); }
  }

  async function resetBudget() {
    if (!budgetBotId) return;
    try {
      await invoke("reset_bot_budget", { botId: budgetBotId });
      await loadBudget();
    } catch (e) { alert("Failed to reset: " + String(e)); }
  }

  function saveUserAvatar(url: string, style: string) {
    userAvatarUrl = url;
    userAvatarStyle = style;
    localStorage.setItem("ravenbot_user_avatar", url);
    localStorage.setItem("ravenbot_user_avatar_style", style);
    window.dispatchEvent(new Event("user-avatar-changed"));
    showUserAvatarPicker = false;
  }

  function pickTheme(t: ThemeDefinition) {
    currentTheme = t;
    applyTheme(t.id);
  }

  let filteredProviders = $derived(
    searchQuery
      ? catalog.filter(p => {
          const q = searchQuery.toLowerCase().trim();
          return p.name.toLowerCase().includes(q) ||
            p.id.toLowerCase().includes(q) ||
            p.description.toLowerCase().includes(q) ||
            (q === "claude" && (p.id === "anthropic" || p.name.toLowerCase().includes("claude"))) ||
            (q === "grok" && (p.id === "xai" || p.name.toLowerCase().includes("grok"))) ||
            (q.includes("router") && (p.id === "openrouter" || p.name.toLowerCase().includes("openrouter"))) ||
            (q === "9 router" && p.id === "openrouter");
        })
      : catalog
  );

  let keyedProviders = $derived(catalog.filter((p) => !p.keyless && p.id !== "local"));
  let connectedCount = $derived(keyedProviders.filter((p) => hasKey(p.id)).length);
  let totalModels = $derived(
    catalog.reduce((sum, p) => {
      const n = ui[p.id]?.models;
      return sum + (n && n > 0 ? n : 0);
    }, 0)
  );
</script>

{#if open}
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 " onclick={(e) => { if (e.target === e.currentTarget) onClose(); }} onkeydown={(e) => { if (e.key === "Escape") onClose(); }} role="dialog" aria-modal="true" tabindex="-1">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div role="document" class="w-[95vw] max-w-[1400px] h-[92vh] rounded-xl border border-[var(--hairline-strong)] shadow-[var(--shadow-xl)] bg-[var(--surface-1)] flex overflow-hidden" onclick={(e) => e.stopPropagation()}>

    <!-- ── Left Sidebar Navigation ─────────────────────────────────────────── -->
    <div class="w-64 shrink-0 border-r flex flex-col" style="border-color: {currentTheme.borderHex}; background-color: {currentTheme.cardHex}cc;">
      <!-- Logo / Header -->
      <div class="p-5 border-b" style="border-color: {currentTheme.borderHex};">
        <div class="flex items-center gap-3">
          <div class="size-10 rounded-xl flex items-center justify-center border" style="background-color: {currentTheme.primaryColor}25; border-color: {currentTheme.primaryColor}50;">
            <Settings class="size-5" style="color: {currentTheme.accentColor};" />
          </div>
          <div>
            <h2 class="font-bold text-sm text-white">Settings</h2>
            <p class="text-[10px] text-[var(--text-muted)] font-mono">RAVENBOT v0.2.0</p>
          </div>
        </div>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 overflow-y-auto p-3 space-y-1">
        {#each navSections as section}
          {@const SectionIcon = section.icon}
          <button
            type="button"
            class="w-full flex items-center gap-3 px-3 py-2.5 rounded-xl text-left transition-all cursor-pointer {activeSection === section.id ? 'bg-[var(--surface-3)] text-white shadow-lg' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
            onclick={() => activeSection = section.id}
          >
            <SectionIcon class="size-4 {section.color || ''}" />
            <span class="text-sm font-medium flex-1">{section.label}</span>
            {#if section.id === "providers"}
              <span class="text-[9px] font-mono px-1.5 py-0.5 rounded-full bg-[var(--surface-3)] text-[var(--text-tertiary)]">{providersBadge}</span>
            {:else if section.badge}
              <span class="text-[9px] font-mono px-1.5 py-0.5 rounded-full bg-[var(--surface-3)] text-[var(--text-tertiary)]">{section.badge}</span>
            {/if}
            {#if activeSection === section.id}
              <ChevronRight class="size-3 text-[var(--text-muted)]" />
            {/if}
          </button>
        {/each}
      </nav>

      <!-- Bottom Stats -->
      <div class="p-4 border-t space-y-2" style="border-color: {currentTheme.borderHex};">
        <div class="flex items-center justify-between text-[10px] font-mono text-[var(--text-muted)]">
          <span>Connected</span>
          <span class="text-success">{connectedCount} / {keyedProviders.length}</span>
        </div>
        <div class="flex items-center justify-between text-[10px] font-mono text-[var(--text-muted)]">
          <span>Models found</span>
          <span class="text-[var(--brand-text)]">{totalModels}</span>
        </div>
        <button type="button" onclick={loadConfiguredProviders} class="w-full text-[10px] text-[var(--brand-text)] hover:text-[var(--brand-text)] flex items-center justify-center gap-1 cursor-pointer">
          <RefreshCw class="size-3" /> Refresh status
        </button>
      </div>
    </div>

    <!-- ── Main Content Area ───────────────────────────────────────────────── -->
    <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
      <!-- Top Header Bar with Close Button -->
      <div class="h-12 px-6 border-b flex items-center justify-between shrink-0" style="border-color: {currentTheme.borderHex}; background-color: {currentTheme.cardHex}80;">
        <div class="flex items-center gap-2">
          <span class="text-xs font-mono uppercase tracking-wider text-[var(--text-muted)]">Settings</span>
          <span class="text-[var(--text-muted)]">/</span>
          <span class="text-xs font-bold text-white capitalize">{navSections.find(s => s.id === activeSection)?.label || activeSection}</span>
        </div>
        <button
          type="button"
          class="size-8 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          onclick={onClose}
          title="Close Settings (Esc)"
        >
          <X class="size-4" />
        </button>
      </div>

      <!-- Section: Providers -->
      {#if activeSection === "providers"}
        <div class="flex-1 overflow-y-auto p-6">
          <div class="max-w-4xl mx-auto space-y-6">
            <!-- Header -->
            <div>
              <h3 class="text-lg font-bold text-white flex items-center gap-2">
                <Key class="size-5 text-[var(--brand-text)]" /> Provider API Keys
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">Connect your AI providers. Keys are stored locally and encrypted at rest.</p>
            </div>

            <!-- Search + Actions -->
            <div class="flex items-center gap-3">
              <div class="relative flex-1">
                <Input bind:value={searchQuery} placeholder="Search providers..." class="h-9 pl-8 text-sm" />
                <svg class="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-[var(--text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>
              </div>
              <Button size="sm" variant="outline" class="h-9 gap-1.5 text-xs border-[var(--hairline-strong)]" onclick={testAllProviders} disabled={connectedCount === 0}>
                <Zap class="size-3.5" /> Test All
              </Button>
              <Button size="sm" variant="outline" class="h-9 gap-1.5 text-xs border-[var(--hairline-strong)]" onclick={loadConfiguredProviders}>
                <RefreshCw class="size-3.5" /> Refresh
              </Button>
            </div>

            <!-- Provider Cards Grid (keyed providers; Ollama lives in Local AI) -->
            <div class="grid grid-cols-2 gap-4">
              {#each filteredProviders.filter((p) => p.id !== "ollama" && p.id !== "local") as ps (ps.id)}
                {@const st = ui[ps.id] || defaultUi()}
                {@const connected = hasKey(ps.id)}
                <div class="rounded-2xl border p-4 transition-all hover:shadow-lg {connected ? 'border-success/30 bg-success/10' : 'border-[var(--hairline)] bg-card/60'}">
                  <div class="flex items-start justify-between mb-3">
                    <div class="flex items-center gap-2">
                      <span class="text-lg">{ps.icon}</span>
                      <div>
                        <h4 class="text-sm font-bold text-white">{ps.name}</h4>
                        <p class="text-[10px] text-[var(--text-muted)]">{ps.description}</p>
                      </div>
                    </div>
                    <div class="flex items-center gap-1">
                      {#if connected}
                        <Wifi class="size-3.5 text-success" />
                      {:else}
                        <WifiOff class="size-3.5 text-[var(--text-muted)]" />
                      {/if}
                    </div>
                  </div>

                  <!-- Status badges -->
                  <div class="flex items-center gap-2 mb-3">
                    {#if st.saved}
                      <span class="text-[10px] text-success font-bold flex items-center gap-0.5 bg-success/10 px-1.5 py-0.5 rounded-full"><Check class="size-2.5" /> Saved</span>
                    {:else if connected}
                      <span class="text-[10px] text-success font-mono bg-success/10 px-1.5 py-0.5 rounded-full">Connected</span>
                    {:else}
                      <span class="text-[10px] text-[var(--text-muted)] font-mono bg-[var(--surface-3)] px-1.5 py-0.5 rounded-full">Not configured</span>
                    {/if}
                    {#if st.models !== null && st.models >= 0}
                      <span class="text-[10px] text-[var(--brand-text)] font-mono bg-[var(--brand-soft)] px-1.5 py-0.5 rounded-full">{st.models} models</span>
                    {:else if st.models === -1}
                      <span class="text-[10px] text-danger font-mono bg-danger/10 px-1.5 py-0.5 rounded-full">Failed</span>
                    {/if}
                  </div>

                  <!-- Key input -->
                  <div class="flex gap-2">
                    <div class="relative flex-1">
                      <Input type={st.show ? "text" : "password"} bind:value={ui[ps.id].key} placeholder={ps.key_placeholder} class="pr-9 h-8 text-xs font-mono" />
                      <button type="button" class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer" onclick={() => ui[ps.id].show = !ui[ps.id].show}>
                        {#if st.show}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
                      </button>
                    </div>
                    <Button size="sm" class="h-8 gap-1 text-[10px] px-3" onclick={() => saveKey(ps.id)} disabled={!st.key.trim()}>
                      <Save class="size-3" />
                    </Button>
                    {#if connected}
                      <Button size="sm" variant="ghost" class="h-8 px-2 text-[var(--text-tertiary)] hover:text-[var(--text-primary)]" onclick={() => testProvider(ps.id)} title="Test connection">
                        {#if st.testing}<Loader2 class="size-3.5 animate-spin" />{:else}<Zap class="size-3.5" />{/if}
                      </Button>
                      <Button size="sm" variant="ghost" class="h-8 px-2 text-[var(--text-muted)] hover:text-danger" onclick={() => clearKey(ps.id)} title="Remove saved key">
                        <Trash2 class="size-3.5" />
                      </Button>
                    {/if}
                  </div>
                  {#if st.saveError}
                    <p class="text-[10px] text-danger mt-2">{st.saveError}</p>
                  {/if}

                  <!-- Get key link -->
                  {#if !connected && ps.key_url}
                    <a href={ps.key_url} target="_blank" rel="noopener" class="inline-flex items-center gap-1 text-[10px] text-[var(--brand-text)] hover:text-[var(--brand-text)] mt-2">
                      Get API key <ExternalLink class="size-2.5" />
                    </a>
                  {/if}
                </div>
              {/each}
              {#if filteredProviders.filter((p) => p.id !== "ollama" && p.id !== "local").length === 0}
                <div class="col-span-2 text-center py-10 rounded-2xl border border-dashed border-[var(--hairline)] bg-[var(--surface-2)]/50">
                  <Key class="size-8 mx-auto text-[var(--text-muted)] mb-2 opacity-60" />
                  <p class="text-sm font-semibold text-[var(--text-secondary)]">No providers match "{searchQuery}"</p>
                  <p class="text-xs text-[var(--text-muted)] mt-1">Available: Command Code, OpenCode, Claude, Cline, OpenRouter, OpenAI, Gemini, DeepSeek, Groq, xAI Grok, Mistral, Together, Perplexity, Cohere, MiMo, TokenRouter</p>
                </div>
              {/if}
            </div>
            <p class="text-[11px] text-[var(--text-muted)]">Ollama is keyless — configure it in <button type="button" class="text-[var(--brand-text)] hover:text-[var(--brand-text)] cursor-pointer" onclick={() => activeSection = "local"}>Local AI</button>.</p>
          </div>
        </div>
      {/if}

      <!-- Section: Connectors -->
      {#if activeSection === "connectors"}
        <div class="flex-1 overflow-y-auto">
          <ConnectorCenter {bots} />
        </div>
      {/if}

      <!-- Section: Model Manager -->
      {#if activeSection === "models"}
        <div class="flex-1 overflow-y-auto p-6">
          <div class="max-w-4xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-white flex items-center gap-2">
                <Cpu class="size-5 text-warning" /> Model Manager
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">Discover and manage models across all connected providers.</p>
            </div>

            <!-- Summary -->
            <div class="grid grid-cols-4 gap-3">
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">{connectedCount}</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">Connected</span>
              </div>
              <div class="p-3 rounded-xl bg-warning/10 border border-warning/20 text-center">
                <span class="text-2xl font-bold text-warning block">{totalModels}</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">Total Models</span>
              </div>
              <div class="p-3 rounded-xl bg-success/10 border border-success/20 text-center">
                <span class="text-2xl font-bold text-success block">{Object.values(discoveredModels).flat().filter(m => m.is_free).length}</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">Free Models</span>
              </div>
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">{Object.values(discoveredModels).flat().filter(m => m.supports_vision).length}</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">Vision</span>
              </div>
            </div>

            <!-- Actions -->
            <div class="flex items-center gap-3">
              <Button size="sm" class="gap-1.5 text-xs" onclick={testAllProviders} disabled={connectedCount === 0}>
                <Zap class="size-3.5" /> Discover All Models
              </Button>
              <Button size="sm" variant="outline" class="gap-1.5 text-xs border-[var(--hairline-strong)]" onclick={() => { discoveredModels = {}; totalDiscovered = 0; }}>
                <Trash2 class="size-3.5" /> Clear
              </Button>
            </div>

            <!-- Default model for new agents -->
            <div class="p-4 rounded-2xl border border-[var(--hairline)] bg-card/60 space-y-3">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-sm font-bold text-white">Default for new agents</h4>
                  <p class="text-[11px] text-[var(--text-muted)]">New bots start here (env override still wins). Source: {defaultSource}.</p>
                </div>
                {#if defaultSaved}
                  <span class="text-[10px] text-success font-bold flex items-center gap-1 bg-success/10 px-2 py-1 rounded-full"><Check class="size-3" /> Saved</span>
                {/if}
              </div>
              <ModelPicker
                providers={catalog.filter((p) => p.id !== "local")}
                provider={defaultProvider}
                model={defaultModel}
                models={defaultModels.length
                  ? defaultModels.map((m: any) => ({ id: m.id, name: m.name || m.id, is_free: m.is_free, supports_vision: m.supports_vision }))
                  : (catalog.find((p) => p.id === defaultProvider)?.fallback_models ?? [])}
                configured={configuredProviders}
                loading={defaultLoadingModels}
                onSelectProvider={(id: string) => { defaultProvider = id; loadDefaultModels(id); }}
                onSelectModel={(id: string) => (defaultModel = id)}
                onRefresh={() => loadDefaultModels(defaultProvider)}
                heightClass="max-h-[18rem]"
              />
              <div class="flex justify-end">
                <Button size="sm" class="h-8 gap-1.5 text-xs" onclick={saveDefaultModel} disabled={defaultSaving}>
                  {#if defaultSaving}<Loader2 class="size-3 animate-spin" />{:else}<Save class="size-3" />{/if} Save default
                </Button>
              </div>
            </div>

            <!-- Discovered models by provider -->
            {#each Object.entries(discoveredModels) as [providerId, models]}
              {#if models.length > 0}
                {@const entry = catalog.find((p) => p.id === providerId)}
                <div class="rounded-2xl border border-[var(--hairline)] bg-card/60 overflow-hidden">
                  <div class="px-4 py-3 border-b border-[var(--hairline)] flex items-center justify-between">
                    <div class="flex items-center gap-2">
                      <span class="text-lg">{entry?.icon || '🔌'}</span>
                      <h4 class="text-sm font-bold text-white">{entry?.name || providerId}</h4>
                      <span class="text-[10px] font-mono text-[var(--text-muted)]">{models.length} models</span>
                    </div>
                  </div>
                  <div class="p-3 max-h-64 overflow-y-auto">
                    <div class="grid grid-cols-2 gap-2">
                      {#each models as model}
                        <div class="flex items-center justify-between p-2 rounded-lg bg-black/20 text-xs">
                          <div class="flex items-center gap-2 min-w-0">
                            <span class="truncate text-[var(--text-secondary)]">{model.name || model.id}</span>
                          </div>
                          <div class="flex items-center gap-1 shrink-0">
                            {#if model.is_free}
                              <span class="text-[9px] text-success bg-success/10 px-1 py-0.5 rounded">Free</span>
                            {/if}
                            {#if model.supports_vision}
                              <span class="text-[9px] text-[var(--brand-text)] bg-[var(--brand-soft)] px-1 py-0.5 rounded">Vision</span>
                            {/if}
                          </div>
                        </div>
                      {/each}
                    </div>
                  </div>
                </div>
              {/if}
            {:else}
              <div class="text-center py-12 text-[var(--text-muted)]">
                <Cpu class="size-12 mx-auto mb-3 text-[var(--text-muted)]" />
                <p class="text-sm">No models discovered yet.</p>
                <p class="text-xs mt-1">Connect a provider and click "Discover All Models".</p>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Section: Budgets -->
      {#if activeSection === "budgets"}
        <div class="flex-1 overflow-y-auto p-6">
          <div class="max-w-3xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-white flex items-center gap-2">
                <DollarSign class="size-5 text-success" /> Agent Budgets
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">Cap what each agent can spend. Runs are refused once the limit is hit.</p>
            </div>

            <div class="p-5 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <div class="mb-4">
                <Select.Root type="single" value={budgetBotId} onValueChange={(v) => (budgetBotId = v ?? "")}>
                  <Select.Trigger class="w-full h-9 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--hairline-strong)] px-3 flex items-center justify-between text-sm cursor-pointer">
                    <span class={budgetBotId ? "text-[var(--text-secondary)]" : "text-[var(--text-muted)]"}>
                      {budgetBotId
                        ? (bots.find((b) => b.id === budgetBotId)?.name ?? "Select an agent…")
                        : "Select an agent…"}
                    </span>
                    <ChevronDown class="size-4 text-[var(--text-muted)] shrink-0" />
                  </Select.Trigger>
                  <Select.Content class="max-h-72">
                    {#each bots as b (b.id)}
                      <Select.Item value={b.id} class="text-xs">{b.name}</Select.Item>
                    {/each}
                  </Select.Content>
                </Select.Root>
              </div>

              {#if budgetBotId && budgetState}
                <div class="space-y-4">
                  <div class="flex items-center justify-between text-sm">
                    <span class="text-[var(--text-tertiary)]">Tokens used</span>
                    <span class="font-mono text-white">{budgetState.tokens_used.toLocaleString()}</span>
                  </div>
                  <div class="flex items-center justify-between text-sm">
                    <span class="text-[var(--text-tertiary)]">Cost</span>
                    <span class="font-mono text-white">${budgetState.cost_used.toFixed(4)}</span>
                  </div>
                  <div class="h-2 rounded-full bg-black/40 overflow-hidden">
                    <div class="h-full rounded-full transition-all {budgetState.should_warn ? 'bg-warning' : budgetState.allowed ? 'bg-success' : 'bg-danger'}" style="width: {Math.min(budgetState.percentage_used, 100)}%"></div>
                  </div>
                  <p class="text-xs text-[var(--text-muted)]">{budgetState.percentage_used.toFixed(0)}% used {budgetState.allowed ? '' : '— BLOCKED'}</p>

                  <div class="grid grid-cols-3 gap-3 pt-3 border-t" style="border-color: {currentTheme.borderHex};">
                    <SimpleSelect
                      value={budgetKind}
                      options={[
                        { value: "unlimited", label: "No limit" },
                        { value: "tokens", label: "Max tokens" },
                        { value: "cost", label: "Max cost ($)" },
                      ]}
                      onValueChange={(v) => (budgetKind = v as "unlimited" | "tokens" | "cost")}
                    />
                    <Input type="number" bind:value={budgetMax} placeholder="Limit" class="h-9 text-xs font-mono" disabled={budgetKind === "unlimited"} />
                    <SimpleSelect
                      value={budgetPeriod}
                      options={[
                        { value: "total", label: "Lifetime" },
                        { value: "daily", label: "Daily" },
                        { value: "weekly", label: "Weekly" },
                        { value: "monthly", label: "Monthly" },
                      ]}
                      onValueChange={(v) => (budgetPeriod = v)}
                    />
                  </div>
                  <div class="flex justify-between">
                    <Button size="sm" variant="outline" class="h-8 gap-1.5 text-xs border-danger/30 text-danger" onclick={resetBudget}>
                      <Trash2 class="size-3" /> Reset usage
                    </Button>
                    <Button size="sm" class="h-8 gap-1.5 text-xs" onclick={saveBudget}>
                      <Save class="size-3" /> Save budget
                    </Button>
                  </div>
                </div>
              {:else if budgetBotId}
                <p class="text-sm text-[var(--text-muted)]">Loading budget…</p>
              {:else}
                <p class="text-sm text-[var(--text-muted)]">Select an agent to manage its budget.</p>
              {/if}
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: Profile -->
      {#if activeSection === "profile"}
        <div class="flex-1 overflow-y-auto p-6">
          <div class="max-w-2xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-white flex items-center gap-2">
                <User class="size-5 text-[var(--brand-text)]" /> Profile
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">Your identity across RAVENBOT.</p>
            </div>
            <div class="p-5 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <div class="flex items-center gap-5">
                <button type="button" onclick={() => (showUserAvatarPicker = !showUserAvatarPicker)} class="group shrink-0">
                  <div class="size-24 rounded-2xl overflow-hidden bg-muted border-2 p-1 shadow-md group-hover:scale-105 transition-transform" style="border-color: {currentTheme.primaryColor}60;">
                    <img src={userAvatarUrl || getDiceBearUrl(userName || "You", userAvatarStyle)} alt="You" class="size-full rounded-xl object-cover" />
                  </div>
                  <p class="text-[11px] text-muted-foreground mt-1 text-center">Tap to change</p>
                </button>
                <div class="flex-1 space-y-3">
                  <div>
                    <Label class="text-xs font-bold">Display name</Label>
                    <Input bind:value={userName} placeholder="Your name" class="mt-1" oninput={() => { localStorage.setItem("ravenbot_user_name", userName); window.dispatchEvent(new Event("user-avatar-changed")); }} />
                  </div>
                </div>
              </div>
              {#if showUserAvatarPicker}
                <div class="mt-4 pt-4 border-t" style="border-color: {currentTheme.borderHex};">
                  <AvatarPicker seed={userName || "You"} style={userAvatarStyle} customUrl={userAvatarUrl} onSelect={saveUserAvatar} />
                </div>
              {/if}
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: Themes -->
      {#if activeSection === "themes"}
        <div class="flex-1 overflow-y-auto p-6">
          <div class="max-w-3xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-white flex items-center gap-2">
                <Palette class="size-5 text-pink-400" /> Themes
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">Choose your visual style.</p>
            </div>
            <div class="grid grid-cols-2 gap-4">
              {#each THEMES as t}
                {@const isSelected = currentTheme.id === t.id}
                <button type="button" class="flex flex-col text-left p-4 rounded-2xl border transition-all {isSelected ? 'shadow-xl ring-2 scale-[1.02]' : 'hover:bg-[var(--surface-3)] hover:scale-[1.01]'}" style={isSelected ? `background-color: ${t.cardHex}; border-color: ${t.primaryColor};` : `background-color: ${t.cardHex}90; border-color: ${t.borderHex};`} onclick={() => pickTheme(t)}>
                  <div class="flex items-center gap-3 mb-2">
                    <ThemeLogo theme={t} size="sm" class="!size-10" />
                    <div>
                      <span class="font-bold text-sm text-white block">{t.name}</span>
                      <span class="text-[10px] font-mono text-[var(--text-tertiary)]">{t.brand.badgeLabel}</span>
                    </div>
                  </div>
                  <p class="text-xs text-[var(--text-tertiary)]">{t.description}</p>
                  <div class="flex items-center gap-1 mt-2.5">
                    {#each [t.bgHex, t.cardHex, t.borderHex, t.primaryColor, t.accentColor, t.secondaryAccent].filter(Boolean) as c}
                      <span class="size-3 rounded-sm border border-black/40" style="background-color: {c};" title={c}></span>
                    {/each}
                  </div>
                  {#if isSelected}
                    <div class="mt-2 text-[10px] text-success font-bold flex items-center gap-1"><Check class="size-3" /> Active</div>
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: Local AI -->
      {#if activeSection === "local"}
        <div class="flex-1 overflow-y-auto p-6">
          <div class="max-w-3xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-white flex items-center gap-2">
                <Server class="size-5 text-green-400" /> Local AI
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">Run AI entirely on your device. No cloud, no data leaves your machine.</p>
            </div>
            <div class="p-5 rounded-2xl border bg-card/60 space-y-3" style="border-color: {currentTheme.borderHex};">
              <div class="flex items-center justify-between">
                <h4 class="text-sm font-bold text-white">Ollama Endpoint</h4>
                {#if ollamaSaved}
                  <span class="text-[10px] text-success font-bold flex items-center gap-1 bg-success/10 px-2 py-1 rounded-full"><Check class="size-3" /> Saved</span>
                {/if}
              </div>
              <div class="flex gap-2">
                <Input bind:value={ollamaUrl} placeholder="http://localhost:11434" class="text-sm font-mono flex-1" />
                <Button size="sm" class="gap-1.5" onclick={saveOllamaUrl} disabled={ollamaSaving}>
                  {#if ollamaSaving}<Loader2 class="size-3.5 animate-spin" />{:else}<Save class="size-3.5" />{/if} Save
                </Button>
                <Button size="sm" variant="outline" class="gap-1.5 border-[var(--hairline-strong)]" onclick={() => probeOllama()} disabled={ollamaProbing}>
                  {#if ollamaProbing}<Loader2 class="size-3.5 animate-spin" />{:else}<Zap class="size-3.5" />{/if} Probe
                </Button>
              </div>
              {#if ollamaError}
                <p class="text-[11px] text-danger bg-danger/10 border border-danger/20 rounded-lg px-2.5 py-2">{ollamaError}</p>
              {/if}
              {#if ollamaProbe}
                <div class="rounded-xl border border-success/20 bg-success/10 p-3">
                  <p class="text-[11px] text-success font-mono">✓ {ollamaProbe.base_url} · {ollamaProbe.count} model{(ollamaProbe.count === 1) ? "" : "s"} installed</p>
                  {#if ollamaProbe.count > 0}
                    <div class="flex flex-wrap gap-1.5 mt-2">
                      {#each ollamaProbe.models.slice(0, 12) as m}
                        <span class="text-[10px] font-mono text-[var(--text-secondary)] bg-black/40 border border-[var(--hairline)] px-1.5 py-0.5 rounded">{m.name}</span>
                      {/each}
                      {#if ollamaProbe.count > 12}
                        <span class="text-[10px] font-mono text-[var(--text-muted)]">+{ollamaProbe.count - 12} more</span>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
              <p class="text-xs text-[var(--text-muted)]">
                Install Ollama from <a href="https://ollama.com" target="_blank" class="text-[var(--brand-text)] hover:underline">ollama.com</a>, then run:
                <code class="block mt-1 p-2 rounded bg-black/40 text-[11px] font-mono text-[var(--text-secondary)]">ollama pull llama3.1</code>
              </p>
            </div>
            <div class="p-5 rounded-2xl border bg-warning/5 border-warning/20">
              <div class="flex items-center gap-2 text-warning text-sm font-bold mb-2"><AlertCircle class="size-4" /> Experimental</div>
              <p class="text-xs text-[var(--text-tertiary)]">
                The "Local (candle/llama.cpp)" provider is an experimental, unlinked engine.
                Ollama is the recommended local path today.
              </p>
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: About -->
      {#if activeSection === "about"}
        <div class="flex-1 overflow-y-auto p-6">
          <div class="max-w-2xl mx-auto space-y-6">
            <div class="text-center py-8">
              <ThemeLogo theme={currentTheme} size="xl" class="mx-auto mb-4" />
              <h3 class="text-xl font-bold text-white">{currentTheme.brand.brandTitle}{currentTheme.brand.brandAccent}</h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{currentTheme.brand.tagline}</p>
              <p class="text-xs text-[var(--text-muted)] font-mono mt-2">v0.2.0 · Sovereign AI OS</p>
            </div>
            <div class="grid grid-cols-4 gap-3">
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">16</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">Providers</span>
              </div>
              <div class="p-3 rounded-xl bg-warning/10 border border-warning/20 text-center">
                <span class="text-2xl font-bold text-warning block">28+</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">Models</span>
              </div>
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">135+</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">MCP Tools</span>
              </div>
              <div class="p-3 rounded-xl bg-success/10 border border-success/20 text-center">
                <span class="text-2xl font-bold text-success block">1.5K+</span>
                <span class="text-[10px] text-[var(--text-muted)] font-mono uppercase">Skills</span>
              </div>
            </div>
            <div class="p-4 rounded-2xl border border-[var(--hairline)] bg-card/60 text-center">
              <p class="text-xs text-[var(--text-muted)]">100% Local · Zero Data Collection · Fully Sovereign</p>
            </div>
          </div>
        </div>
      {/if}

    </div>
  </div>
</div>
{/if}
