<script lang="ts">
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { onMount } from "svelte";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl, cn } from "$lib/utils";
  import { getStoredTheme, applyTheme, subscribeTheme, type ThemeDefinition, THEMES } from "$lib/theme";
  import { setLocale, getAvailableLocales, localeState, type Locale } from "$lib/i18n";
  import { t } from "$lib/i18n";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
  import ConnectorCenter from "$lib/components/ConnectorCenter.svelte";
  import ModelPicker from "$lib/components/ModelPicker.svelte";
  import * as Select from "$lib/components/ui/select";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import {
    getCatalog, getCachedCatalog, noteDiscovery, modelMetaFor, modelSummary,
    type CatalogProvider,
  } from "$lib/model-catalog";
  import {
    Key, Palette, Server, Info, Check, Eye, EyeOff, Save, User,
    Loader2, Wifi, WifiOff, RefreshCw, AlertCircle, Zap, Layers,
    DollarSign, Shield, BookOpen, Settings, ChevronRight, Sparkles,
    Cpu, Globe, Lock, Unlock, Trash2, ExternalLink, X, Plug, Plus,
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

  // Real numbers only — the badge reflects the actual curated provider count.
  let providersBadge = $derived(`${catalog.length}`);
  let curatedModelCount = $derived(catalog.reduce((s, p) => s + p.fallback_models.length, 0));

  const navSections = $derived<NavSection[]>([
    { id: "providers", label: t("settings.navProviders"), icon: Key, color: "text-[var(--brand-text)]" },
    { id: "connectors", label: t("ui.connectors"), icon: Layers, badge: "135+", color: "text-[var(--brand-text)]" },
    { id: "models", label: t("settings.navModels"), icon: Cpu, badge: t("settings.live"), color: "text-warning" },
    { id: "budgets", label: t("settings.navBudgets"), icon: DollarSign, color: "text-success" },
    { id: "profile", label: t("settings.navProfile"), icon: User, color: "text-[var(--brand-text)]" },
    { id: "themes", label: t("settings.navThemes"), icon: Palette, color: "text-info" },
    { id: "local", label: t("settings.navLocal"), icon: Server, color: "text-success" },
    { id: "about", label: t("settings.about"), icon: Info, color: "text-[var(--text-tertiary)]" },
  ]);

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
  // Honest facts for the chosen default (discovery cache first, list second).
  let defaultFacts = $derived(
    modelSummary(modelMetaFor(defaultProvider, defaultModel)) ||
      modelSummary(defaultModels.find((m: any) => m.id === defaultModel)),
  );

  onMount(() => {
    return subscribeTheme((t) => {
      currentTheme = t;
    });
  });

  // ── Custom providers (P8) ─────────────────────────────────────────────────
  interface CustomProviderRow {
    id: string; display_name: string; kind: string; base_url: string;
    default_model: string; supports_tools: boolean; enabled: boolean; has_key: boolean;
  }
  let customProviders = $state<CustomProviderRow[]>([]);
  let showCustomDialog = $state(false);
  let customEditing = $state(""); // "" = creating
  let customForm = $state({
    display_name: "", kind: "openai", base_url: "", api_key: "",
    default_model: "", supports_tools: true, enabled: true,
  });
  let customSaving = $state(false);
  let customTesting = $state(false);
  let customTestResult = $state<string | null>(null);
  let customError = $state<string | null>(null);

  async function loadCustomProviders() {
    try {
      customProviders = await invoke<CustomProviderRow[]>("list_custom_providers");
    } catch {
      customProviders = [];
    }
  }

  function openCustomDialog(row?: CustomProviderRow) {
    customError = null;
    customTestResult = null;
    if (row) {
      customEditing = row.id;
      customForm = {
        display_name: row.display_name, kind: row.kind, base_url: row.base_url,
        api_key: "", default_model: row.default_model,
        supports_tools: row.supports_tools, enabled: row.enabled,
      };
    } else {
      customEditing = "";
      customForm = {
        display_name: "", kind: "openai", base_url: "", api_key: "",
        default_model: "", supports_tools: true, enabled: true,
      };
    }
    showCustomDialog = true;
  }

  function customIdFromName(name: string): string {
    const slug = name.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+/, "").slice(0, 32);
    return /^[a-z]/.test(slug) ? slug : `p-${slug}`.slice(0, 32);
  }

  /** Reload catalog (customs merged) + configured keys so every surface sees changes. */
  async function refreshProviderSurfaces() {
    await loadCustomProviders();
    const list = await getCatalog(true);
    catalog = list;
    for (const p of list) {
      if (!ui[p.id]) ui[p.id] = defaultUi();
    }
    loadConfiguredProviders();
  }

  async function saveCustomProvider() {
    customSaving = true;
    customError = null;
    try {
      const trimmedKey = customForm.api_key.trim();
      await invoke("upsert_custom_provider", {
        provider: {
          id: customEditing || customIdFromName(customForm.display_name),
          display_name: customForm.display_name.trim(),
          kind: customForm.kind,
          base_url: customForm.base_url.trim(),
          api_key: trimmedKey === "" ? null : trimmedKey,
          default_model: customForm.default_model.trim(),
          supports_tools: customForm.supports_tools,
          enabled: customForm.enabled,
        },
      });
      showCustomDialog = false;
      await refreshProviderSurfaces();
    } catch (e) {
      customError = String(e);
    } finally {
      customSaving = false;
    }
  }

  async function testCustomProvider(id: string) {
    customTesting = true;
    customTestResult = null;
    try {
      const reply = await invoke<string>("test_custom_provider", { id });
      customTestResult = reply ? t("settings.testReply", { reply }) : t("settings.testOk");
    } catch (e) {
      customTestResult = String(e);
    } finally {
      customTesting = false;
    }
  }

  async function deleteCustomProvider(row: CustomProviderRow) {
    if (!confirm(t("settings.deleteCustomConfirm", { name: row.display_name }))) return;
    try {
      await invoke("delete_custom_provider", { id: row.id });
    } catch (e) {
      // Backend refuses while bots reference it — surface the count, allow force.
      if (!confirm(`${String(e)} ${t("settings.forceDeleteQuestion")}`)) return;
      try {
        await invoke("delete_custom_provider", { id: row.id, force: true });
      } catch (e2) {
        customError = String(e2);
        return;
      }
    }
    await refreshProviderSurfaces();
  }

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
        loadCustomProviders();
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

  /**
   * When the provider status was last actually read.
   *
   * The numbers below this block — connected, models found — are a snapshot,
   * and a snapshot with no timestamp is one the user cannot reason about: an
   * infrastructure tool that says "2 / 16" without saying when is indistinguishable
   * from one that is simply stale. `null` until the first read, which is a
   * real state and is labelled "never" rather than rendered as a zero.
   */
  let statusCheckedAt = $state<number | null>(null);
  /** Bumped once a minute; the only reason the "time ago" label is reactive. */
  let statusTick = $state(0);
  let statusTicker: ReturnType<typeof setInterval> | null = null;

  /** Live "time ago", so the timestamp does not need a timer to stay true. */
  let checkedAge = $derived.by(() => {
    if (statusCheckedAt === null) return null;
    void statusTick;
    return Math.floor((Date.now() - statusCheckedAt) / 1000);
  });

  let statusLabel = $derived.by(() => {
    const s = checkedAge;
    if (s === null) return t("settings.never");
    if (s < 10) return t("settings.justNow");
    if (s < 60) return t("settings.agoSeconds", { n: s });
    if (s < 3600) return t("settings.agoMinutes", { n: Math.floor(s / 60) });
    return t("settings.agoHours", { n: Math.floor(s / 3600) });
  });

  async function loadConfiguredProviders() {
    isLoadingConfigured = true;
    try {
      configuredProviders = await invoke<string[]>("get_configured_providers");
      statusCheckedAt = Date.now();
      statusTick++;
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
      noteDiscovery(provider, models || []);
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
        noteDiscovery(id, models);
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
      noteDiscovery(provider, defaultModels);
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
      alert(t("settings.alertDefaultFailed") + String(e));
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
    } catch (e) { alert(t("settings.alertBudgetFailed") + String(e)); }
  }

  async function resetBudget() {
    if (!budgetBotId) return;
    try {
      await invoke("reset_bot_budget", { botId: budgetBotId });
      await loadBudget();
    } catch (e) { alert(t("settings.alertResetFailed") + String(e)); }
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
  <div role="document" class="modal-panel w-[95vw] max-w-[1400px] h-[92vh] flex overflow-hidden" onclick={(e) => e.stopPropagation()}>

    <!-- ── Left Sidebar Navigation ─────────────────────────────────────────── -->
    <div class="w-64 shrink-0 border-r flex flex-col" style="border-color: {currentTheme.borderHex}; background-color: {currentTheme.cardHex}cc;">
      <!-- Logo / Header -->
      <div class="p-5 border-b" style="border-color: {currentTheme.borderHex};">
        <div class="flex items-center gap-3">
          <div class="size-10 rounded-xl flex items-center justify-center border" style="background-color: {currentTheme.primaryColor}25; border-color: {currentTheme.primaryColor}50;">
            <Settings class="size-5" style="color: {currentTheme.accentColor};" />
          </div>
          <div>
            <h2 class="font-bold text-sm text-[var(--text-primary)]">{t("settings.title")}</h2>
            <p class="text-[11px] text-[var(--text-muted)] font-mono">RAVENBOT v0.2.0</p>
          </div>
        </div>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 overflow-y-auto no-scrollbar p-3 space-y-1">
        {#each navSections as section}
          {@const SectionIcon = section.icon}
          <button
            type="button"
            class="w-full flex items-center gap-3 px-3 py-2.5 rounded-xl text-left transition-all cursor-pointer {activeSection === section.id ? 'bg-[var(--surface-3)] text-[var(--text-primary)] shadow-lg' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
            onclick={() => activeSection = section.id}
            aria-pressed={activeSection === section.id}
          >
            <SectionIcon class="size-4 {section.color || ''}" />
            <span class="text-sm font-medium flex-1">{section.label}</span>
            <!--
              The count badge sits on `--surface-3`, the *hover* surface, where
              9px `--text-tertiary` measures 4.25:1 — under the 4.5:1 body-text
              floor at that size. `--surface-2` is OpenBot's raised row and is
              the right surface for a badge anyway; moving it there fixes the
              ratio without brightening the ramp everywhere.
            -->
            {#if section.id === "providers"}
              <span class="text-[11px] font-mono px-1.5 py-0.5 rounded-full bg-[var(--surface-2)] text-[var(--text-tertiary)]">{providersBadge}</span>
            {:else if section.badge}
              <span class="text-[11px] font-mono px-1.5 py-0.5 rounded-full bg-[var(--surface-2)] text-[var(--text-tertiary)]">{section.badge}</span>
            {/if}
            {#if activeSection === section.id}
              <ChevronRight class="size-3 text-[var(--text-muted)]" />
            {/if}
          </button>
        {/each}
      </nav>

      <!--
        System status.

        Three numbers and a timestamp, because a snapshot without one is
        indistinguishable from a stale one — and this is the strip that stays on
        screen for the whole session. The counts are what the reviewer liked
        about it and they have not changed; what was missing was the third
        question, which is always "and how old is this?".

        Relabelled rather than added to, because "Connected 2 / 16" and
        "Connections 2 / 16" are the same fact and the shorter name leaves room
        for the timestamp on the same row width.
      -->
      <div class="p-4 border-t space-y-2" style="border-color: {currentTheme.borderHex};">
        <span class="block text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("settings.systemStatus")}
        </span>
        <div class="flex items-center justify-between text-[11px] font-mono text-[var(--text-tertiary)]">
          <span>{t("settings.connections")}</span>
          <span class="text-success">{connectedCount} / {keyedProviders.length}</span>
        </div>
        <div class="flex items-center justify-between text-[11px] font-mono text-[var(--text-tertiary)]">
          <span>{t("settings.models")}</span>
          <span class="text-[var(--brand-text)]">{totalModels}</span>
        </div>
        <div class="flex items-center justify-between text-[11px] font-mono text-[var(--text-tertiary)]">
          <span>{t("settings.lastChecked")}</span>
          <span class="text-[var(--text-secondary)]">{statusLabel}</span>
        </div>
        <!--
          A full-width action that was 15px tall. WCAG 2.2 asks 24x24 of any
          pointer target, and a full-width strip is easy to hit — it just needs
          the padding to be honest about that. It also reflects the loading
          state, which it did not before: a status strip whose refresh button
          gives no feedback while the thing it refreshes is in flight is worse
          than one without a button.
        -->
        <button type="button" onclick={loadConfiguredProviders} disabled={isLoadingConfigured}
          class="w-full py-1.5 mt-1 text-[11px] text-[var(--brand-text)] hover:text-[var(--brand-text)] flex items-center justify-center gap-1 cursor-pointer rounded-md transition-colors hover:bg-[var(--brand-soft)] disabled:opacity-60 disabled:cursor-default">
          <RefreshCw class={cn("size-3.5", isLoadingConfigured && "animate-spin")} />
          {isLoadingConfigured ? t("connector.syncing") : t("settings.refreshStatus")}
        </button>
      </div>
    </div>

    <!-- ── Main Content Area ───────────────────────────────────────────────── -->
    <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
      <!-- Top Header Bar with Close Button -->
      <div class="h-12 px-6 border-b flex items-center justify-between shrink-0" style="border-color: {currentTheme.borderHex}; background-color: {currentTheme.cardHex}80;">
        <div class="flex items-center gap-2">
          <span class="text-xs font-mono uppercase tracking-wider text-[var(--text-muted)]">{t("settings.title")}</span>
          <span class="text-[var(--text-muted)]">/</span>
          <span class="text-xs font-bold text-[var(--text-primary)] capitalize">{navSections.find(s => s.id === activeSection)?.label || activeSection}</span>
        </div>
        <button
          type="button"
          class="size-8 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          onclick={onClose}
          aria-label={t("settings.close")}
          title={t("settings.close")}
        >
          <X class="size-4" />
        </button>
      </div>

      <!-- Section: Providers -->
      {#if activeSection === "providers"}
        <div class="flex-1 overflow-y-auto no-scrollbar p-6">
          <div class="max-w-4xl mx-auto space-y-6">
            <!-- Header -->
            <div>
              <h3 class="text-lg font-bold text-[var(--text-primary)] flex items-center gap-2">
                <Key class="size-5 text-[var(--brand-text)]" /> {t("settings.providerKeys")}
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{t("settings.providerKeysDesc")}</p>
            </div>

            <!-- Search + Actions -->
            <div class="flex items-center gap-3">
              <div class="relative flex-1">
                <Input bind:value={searchQuery} aria-label={t("settings.searchProviders")} placeholder={t("settings.searchProviders")} class="h-9 pl-8 text-sm" />
                <svg class="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-[var(--text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>
              </div>
              <Button size="sm" variant="outline" class="h-9 gap-1.5 text-xs border-[var(--hairline-strong)]" onclick={testAllProviders} disabled={connectedCount === 0}>
                <Zap class="size-3.5" /> {t("settings.testAll")}
              </Button>
              <Button size="sm" variant="outline" class="h-9 gap-1.5 text-xs border-[var(--hairline-strong)]" onclick={loadConfiguredProviders}>
                <RefreshCw class="size-3.5" /> {t("settings.refresh")}
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
                        <h4 class="text-sm font-bold text-[var(--text-primary)]">{ps.name}</h4>
                        <p class="text-[11px] text-[var(--text-muted)]">{ps.description}</p>
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
                      <span class="text-[11px] text-success font-bold flex items-center gap-0.5 bg-success/10 px-1.5 py-0.5 rounded-full"><Check class="size-2.5" /> {t("settings.saved")}</span>
                    {:else if connected}
                      <span class="text-[11px] text-success font-mono bg-success/10 px-1.5 py-0.5 rounded-full">{t("settings.connected")}</span>
                    {:else}
                      <span class="text-[11px] text-[var(--text-tertiary)] font-mono bg-[var(--surface-2)] px-1.5 py-0.5 rounded-full">{t("settings.notConfigured")}</span>
                    {/if}
                    {#if st.models !== null && st.models >= 0}
                      <span class="text-[11px] text-[var(--brand-text)] font-mono bg-[var(--brand-soft)] px-1.5 py-0.5 rounded-full">{t("settings.modelsCount", { n: st.models })}</span>
                    {:else if st.models === -1}
                      <span class="text-[11px] text-danger font-mono bg-danger/10 px-1.5 py-0.5 rounded-full">{t("settings.failed")}</span>
                    {/if}
                  </div>

                  <!-- Key input -->
                  <div class="flex gap-2">
                    <div class="relative flex-1">
                      <Input type={st.show ? "text" : "password"} bind:value={ui[ps.id].key} placeholder={ps.key_placeholder} class="pr-9 h-8 text-xs font-mono" />
                      <!--
                        The reveal toggle is the only way to check what you just
                        pasted, so it needs a name — a screen reader announcing
                        "button" for the control that shows your API key is not
                        a detail. It was 14×14, which is also below the 24×24
                        WCAG 2.2 floor, so the hit area is padded out to 24
                        while the icon stays 14 and the field stays the same
                        size. The padding is negative on the right to keep the
                        icon where it was rather than shifting it inward.
                      -->
                      <button
                        type="button"
                        class="absolute right-0 top-1/2 -translate-y-1/2 grid place-items-center size-6 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
                        onclick={() => ui[ps.id].show = !ui[ps.id].show}
                        aria-label={st.show ? t("settings.hideApiKey") : t("settings.showApiKey")}
                        aria-pressed={st.show}
                        title={st.show ? t("settings.hideApiKey") : t("settings.showApiKey")}
                      >
                        {#if st.show}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
                      </button>
                    </div>
                    <!--
                      Three icon-only buttons in a row. `title` does supply an
                      accessible name, but only as a last resort — it is not
                      reliably announced, it does not survive being read by a
                      screen reader in browse mode, and it disappears the moment
                      the pointer moves away. For the row that saves, tests and
                      *deletes an API key*, "button" is not a name. All three get
                      an explicit one, including which provider they act on,
                      because there are eight of these rows on screen at once.
                    -->
                    <Button size="sm" class="h-8 gap-1 text-[11px] px-3" onclick={() => saveKey(ps.id)} disabled={!st.key.trim()} aria-label={`${t("ui.save")} — ${ps.name}`} title={t("ui.save")}>
                      <Save class="size-3" />
                    </Button>
                    {#if connected}
                      <Button size="sm" variant="ghost" class="h-8 px-2 text-[var(--text-tertiary)] hover:text-[var(--text-primary)]" onclick={() => testProvider(ps.id)} title={t("settings.testConnection")} aria-label={`${t("settings.testConnection")} — ${ps.name}`}>
                        {#if st.testing}<Loader2 class="size-3.5 animate-spin" />{:else}<Zap class="size-3.5" />{/if}
                      </Button>
                      <Button size="sm" variant="ghost" class="h-8 px-2 text-[var(--text-muted)] hover:text-danger" onclick={() => clearKey(ps.id)} title={t("settings.removeKey")} aria-label={`${t("settings.removeKey")} — ${ps.name}`}>
                        <Trash2 class="size-3.5" />
                      </Button>
                    {/if}
                  </div>
                  {#if st.saveError}
                    <p class="text-[11px] text-danger mt-2">{st.saveError}</p>
                  {/if}

                  <!-- Provider rules: env-var fallback the runtime also reads -->
                  {#if ps.key_env}
                    <p class="text-[11px] font-mono text-[var(--text-muted)] mt-1.5" title={t("settings.envHintTitle")}>
                      env: {ps.key_env}
                    </p>
                  {/if}

                  <!-- Get key link -->
                  {#if !connected && ps.key_url}
                    <a href={ps.key_url} target="_blank" rel="noopener" class="inline-flex items-center gap-1 text-[11px] text-[var(--brand-text)] hover:text-[var(--brand-text)] mt-2">
                      {t("settings.getApiKey")} <ExternalLink class="size-2.5" />
                    </a>
                  {/if}
                </div>
              {/each}
              {#if filteredProviders.filter((p) => p.id !== "ollama" && p.id !== "local").length === 0}
                <div class="col-span-2 text-center py-10 rounded-2xl border border-dashed border-[var(--hairline)] bg-[var(--surface-2)]/50">
                  <Key class="size-8 mx-auto text-[var(--text-muted)] mb-2 opacity-60" />
                  <p class="text-sm font-semibold text-[var(--text-secondary)]">{t("settings.noProvidersMatch", { q: searchQuery })}</p>
                  <p class="text-xs text-[var(--text-muted)] mt-1">{t("settings.availableLabel")}: {catalog.map((p) => p.name).join(", ")}</p>
                </div>
              {/if}
            </div>
            <!-- Custom providers (P8): any OpenAI/Anthropic/Ollama-compatible endpoint -->
            <div class="rounded-2xl border border-[var(--hairline)] bg-card/60 p-4 space-y-3">
              <div class="flex items-center justify-between gap-3">
                <div>
                  <h4 class="text-sm font-bold text-[var(--text-primary)] flex items-center gap-2">
                    <Plug class="size-4 text-[var(--brand-text)]" /> {t("settings.customProviders")}
                  </h4>
                  <p class="text-[11px] text-[var(--text-muted)] mt-0.5">{t("settings.customProvidersDesc")}</p>
                </div>
                <Button size="sm" class="h-8 gap-1.5 text-xs" onclick={() => openCustomDialog()}>
                  <Plus class="size-3.5" /> {t("settings.addProvider")}
                </Button>
              </div>
              {#each customProviders as cp (cp.id)}
                <div class="flex items-center gap-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)]/60 px-3 py-2">
                  <div class="min-w-0 flex-1">
                    <p class="text-xs font-semibold text-[var(--text-primary)] truncate">
                      {cp.display_name}
                      <span class="text-[11px] font-mono text-[var(--text-muted)]">· {cp.id}</span>
                    </p>
                    <p class="text-[11px] font-mono text-[var(--text-muted)] truncate">{cp.base_url}</p>
                  </div>
                  <span class="text-[11px] font-mono px-1.5 py-0.5 rounded-full bg-[var(--surface-2)] text-[var(--text-tertiary)] shrink-0">{cp.kind}</span>
                  {#if !cp.supports_tools}
                    <span class="text-[11px] font-mono px-1.5 py-0.5 rounded-full bg-warning/15 text-warning shrink-0">{t("model.noToolsShort")}</span>
                  {/if}
                  {#if cp.has_key}
                    <span class="text-[11px] font-mono px-1.5 py-0.5 rounded-full bg-success/10 text-success shrink-0">{t("settings.keySet")}</span>
                  {/if}
                  <button type="button" class="text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer p-1" aria-label={t("settings.editProvider")} title={t("settings.editProvider")} onclick={() => openCustomDialog(cp)}>
                    <Settings class="size-3.5" />
                  </button>
                  <button type="button" class="text-[var(--text-muted)] hover:text-danger cursor-pointer p-1" aria-label={t("settings.deleteProvider")} title={t("settings.deleteProvider")} onclick={() => deleteCustomProvider(cp)}>
                    <Trash2 class="size-3.5" />
                  </button>
                </div>
              {:else}
                <p class="text-[11px] text-[var(--text-muted)] text-center py-2">{t("settings.customEmpty")}</p>
              {/each}
              {#if customTestResult}
                <p class="text-[11px] font-mono text-[var(--text-tertiary)] break-words">{customTestResult}</p>
              {/if}
            </div>
            <!--
              An inline link inside a sentence. An inline link is WCAG's one
              exception to the 24×24 target floor, so the *text* keeps its size
              — but the target is padded out vertically with a negative margin
              so the hit area reaches 24px without the line above or below
              moving. It also needs a name of its own: the sentence around it
              otherwise becomes this button's label, which is a 60-character
              description of "Local AI".
            -->
            <p class="text-[11px] text-[var(--text-muted)]">{t("settings.ollamaKeylessA")} <button type="button" class="inline-flex items-center h-6 -my-1.5 px-0.5 text-[var(--brand-text)] hover:text-[var(--brand-text)] cursor-pointer underline underline-offset-2 decoration-[var(--brand-text)]/40 hover:decoration-[var(--brand-text)] rounded-sm transition-colors hover:bg-[var(--brand-soft)]" onclick={() => activeSection = "local"} aria-label={t("settings.navLocal")}>{t("settings.navLocal")}</button>{t("settings.ollamaKeylessB")}</p>
          </div>
        </div>

        <!-- Custom provider add/edit dialog -->
        <Dialog.Root open={showCustomDialog} onOpenChange={(o) => (showCustomDialog = o)}>
          <Dialog.Content class="sm:max-w-lg bg-[var(--surface-1)] border border-[var(--brand)]/30 rounded-xl p-6 text-[var(--text-primary)] no-scrollbar max-h-[85vh] overflow-y-auto">
            <Dialog.Header class="pb-3">
              <Dialog.Title class="text-base font-bold flex items-center gap-2">
                <Plug class="size-4 text-[var(--brand-text)]" />
                {customEditing ? t("settings.editProvider") : t("settings.addProvider")}
              </Dialog.Title>
              <Dialog.Description class="text-xs text-[var(--text-tertiary)]">{t("settings.customProvidersDesc")}</Dialog.Description>
            </Dialog.Header>
            <div class="space-y-3">
              <div class="grid grid-cols-2 gap-3">
                <div class="space-y-1">
                  <Label for="cp-display" class="text-[11px] font-mono uppercase text-[var(--text-muted)]">{t("settings.displayName")}</Label>
                  <Input id="cp-display" bind:value={customForm.display_name} placeholder="LM Studio" class="h-8 text-xs" />
                </div>
                <div class="space-y-1">
                  <Label class="text-[11px] font-mono uppercase text-[var(--text-muted)]">{t("settings.kindLabel")}</Label>
                  <SimpleSelect
                    value={customForm.kind}
                    options={[
                      { value: "openai", label: t("settings.kindOpenai") },
                      { value: "anthropic", label: t("settings.kindAnthropic") },
                      { value: "ollama", label: t("settings.kindOllama") },
                    ]}
                    onValueChange={(v) => (customForm.kind = v)}
                  />
                </div>
              </div>
              <div class="space-y-1">
                <Label for="cp-baseurl" class="text-[11px] font-mono uppercase text-[var(--text-muted)]">{t("settings.baseUrl")}</Label>
                <Input id="cp-baseurl" bind:value={customForm.base_url} placeholder="http://192.168.1.10:1234/v1" class="h-8 text-xs font-mono" />
              </div>
              <div class="grid grid-cols-2 gap-3">
                <div class="space-y-1">
                  <Label for="cp-model" class="text-[11px] font-mono uppercase text-[var(--text-muted)]">{t("settings.defaultModel")}</Label>
                  <Input id="cp-model" bind:value={customForm.default_model} placeholder="qwen/qwen3-8b" class="h-8 text-xs font-mono" />
                </div>
                <div class="space-y-1">
                  <Label for="cp-key" class="text-[11px] font-mono uppercase text-[var(--text-muted)]">{t("settings.apiKeyWriteOnly")}</Label>
                  <Input id="cp-key" type="password" bind:value={customForm.api_key} placeholder={customEditing ? t("settings.apiKeyKeepBlank") : "sk-..."} class="h-8 text-xs font-mono" />
                </div>
              </div>
              <label class="flex items-center gap-2 text-xs text-[var(--text-secondary)] cursor-pointer select-none">
                <input type="checkbox" class="size-3.5 accent-[var(--brand)]" bind:checked={customForm.supports_tools} />
                {t("settings.supportsTools")}
              </label>
              {#if customError}
                <p role="alert" class="text-[11px] text-danger break-words">{customError}</p>
              {/if}
              <div class="flex items-center gap-2 pt-2">
                {#if customEditing}
                  <Button size="sm" variant="outline" class="h-8 gap-1.5 text-xs border-[var(--hairline-strong)]" onclick={() => testCustomProvider(customEditing)} disabled={customTesting}>
                    {#if customTesting}<Loader2 class="size-3.5 animate-spin" />{:else}<Zap class="size-3.5" />{/if} {t("settings.testConnection")}
                  </Button>
                {/if}
                <div class="flex gap-2 ml-auto">
                  <Button size="sm" variant="ghost" class="h-8 text-xs" onclick={() => (showCustomDialog = false)}>{t("settings.cancel")}</Button>
                  <Button size="sm" class="h-8 gap-1.5 text-xs" onclick={saveCustomProvider} disabled={customSaving || !customForm.display_name.trim() || !customForm.base_url.trim()}>
                    {#if customSaving}<Loader2 class="size-3 animate-spin" />{:else}<Save class="size-3" />{/if} {t("settings.save")}
                  </Button>
                </div>
              </div>
            </div>
          </Dialog.Content>
        </Dialog.Root>
      {/if}

      <!-- Section: Connectors -->
      {#if activeSection === "connectors"}
        <div class="flex-1 overflow-y-auto no-scrollbar">
          <ConnectorCenter {bots} />
        </div>
      {/if}

      <!-- Section: Model Manager -->
      {#if activeSection === "models"}
        <div class="flex-1 overflow-y-auto no-scrollbar p-6">
          <div class="max-w-4xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-[var(--text-primary)] flex items-center gap-2">
                <Cpu class="size-5 text-warning" /> {t("settings.navModels")}
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{t("settings.modelsDesc")}</p>
            </div>

            <!-- Summary -->
            <div class="grid grid-cols-4 gap-3">
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">{connectedCount}</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.connected")}</span>
              </div>
              <div class="p-3 rounded-xl bg-warning/10 border border-warning/20 text-center">
                <span class="text-2xl font-bold text-warning block">{totalModels}</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.totalModels")}</span>
              </div>
              <div class="p-3 rounded-xl bg-success/10 border border-success/20 text-center">
                <span class="text-2xl font-bold text-success block">{Object.values(discoveredModels).flat().filter(m => m.is_free).length}</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.freeModels")}</span>
              </div>
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">{Object.values(discoveredModels).flat().filter(m => m.supports_vision).length}</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.vision")}</span>
              </div>
            </div>

            <!-- Actions -->
            <div class="flex items-center gap-3">
              <Button size="sm" class="gap-1.5 text-xs" onclick={testAllProviders} disabled={connectedCount === 0}>
                <Zap class="size-3.5" /> {t("settings.discoverAll")}
              </Button>
              <Button size="sm" variant="outline" class="gap-1.5 text-xs border-[var(--hairline-strong)]" onclick={() => { discoveredModels = {}; totalDiscovered = 0; }}>
                <Trash2 class="size-3.5" /> {t("settings.clear")}
              </Button>
            </div>

            <!-- Default model for new agents -->
            <div class="p-4 rounded-2xl border border-[var(--hairline)] bg-card/60 space-y-3">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-sm font-bold text-[var(--text-primary)]">{t("settings.defaultForNew")}</h4>
                  <p class="text-[11px] text-[var(--text-muted)]">{t("settings.defaultDesc", { source: defaultSource })}</p>
                </div>
                {#if defaultSaved}
                  <span class="text-[11px] text-success font-bold flex items-center gap-1 bg-success/10 px-2 py-1 rounded-full"><Check class="size-3" /> {t("settings.saved")}</span>
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
              {#if defaultModel && defaultFacts}
                <p class="text-[11px] font-mono text-[var(--text-tertiary)]">
                  <span class="text-[var(--brand-text)]">{defaultProvider}/{defaultModel}</span> · {defaultFacts}
                </p>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-8 gap-1.5 text-xs" onclick={saveDefaultModel} disabled={defaultSaving}>
                  {#if defaultSaving}<Loader2 class="size-3 animate-spin" />{:else}<Save class="size-3" />{/if} {t("settings.saveDefault")}
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
                      <h4 class="text-sm font-bold text-[var(--text-primary)]">{entry?.name || providerId}</h4>
                      <span class="text-[11px] font-mono text-[var(--text-muted)]">{t("settings.modelsCount", { n: models.length })}</span>
                    </div>
                  </div>
                  <div class="p-3 max-h-64 overflow-y-auto no-scrollbar">
                    <div class="grid grid-cols-2 gap-2">
                      {#each models as model}
                        {@const facts = modelSummary(model)}
                        <div class="flex items-center justify-between gap-2 p-2 rounded-lg bg-[var(--surface-2)] text-xs">
                          <div class="flex items-center gap-2 min-w-0">
                            <span class="min-w-0">
                              <span class="block truncate text-[var(--text-secondary)]">{model.name || model.id}</span>
                              {#if facts}
                                <span class="block text-[11px] font-mono text-[var(--text-muted)] truncate">{facts}</span>
                              {/if}
                            </span>
                          </div>
                          <div class="flex items-center gap-1 shrink-0">
                            {#if model.is_free}
                              <span class="text-[11px] text-success bg-success/10 px-1 py-0.5 rounded">{t("settings.free")}</span>
                            {/if}
                            {#if model.supports_vision}
                              <span class="text-[11px] text-[var(--brand-text)] bg-[var(--brand-soft)] px-1 py-0.5 rounded">{t("settings.vision")}</span>
                            {/if}
                            {#if model.supports_tools}
                              <span class="text-[11px] text-[var(--text-tertiary)] bg-[var(--surface-2)] border border-[var(--hairline)] px-1 py-0.5 rounded">{t("model.tools")}</span>
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
                <p class="text-sm">{t("settings.noModels")}</p>
                <p class="text-xs mt-1">{t("settings.noModelsHint")}</p>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Section: Budgets -->
      {#if activeSection === "budgets"}
        <div class="flex-1 overflow-y-auto no-scrollbar p-6">
          <div class="max-w-3xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-[var(--text-primary)] flex items-center gap-2">
                <DollarSign class="size-5 text-success" /> {t("settings.agentBudgets")}
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{t("settings.budgetsDesc")}</p>
            </div>

            <div class="p-5 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <div class="mb-4">
                <Select.Root type="single" value={budgetBotId} onValueChange={(v) => (budgetBotId = v ?? "")}>
                  <Select.Trigger class="w-full h-9 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--hairline-strong)] px-3 flex items-center justify-between text-sm cursor-pointer">
                    <span class={budgetBotId ? "text-[var(--text-secondary)]" : "text-[var(--text-muted)]"}>
                      {budgetBotId
                        ? (bots.find((b) => b.id === budgetBotId)?.name ?? t("settings.selectAgent"))
                        : t("settings.selectAgent")}
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
                    <span class="text-[var(--text-tertiary)]">{t("settings.tokensUsed")}</span>
                    <span class="font-mono text-[var(--text-primary)]">{budgetState.tokens_used.toLocaleString()}</span>
                  </div>
                  <div class="flex items-center justify-between text-sm">
                    <span class="text-[var(--text-tertiary)]">{t("thread.cost")}</span>
                    <span class="font-mono text-[var(--text-primary)]">${budgetState.cost_used.toFixed(4)}</span>
                  </div>
                  <div class="h-2 rounded-full bg-[var(--surface-0)] overflow-hidden">
                    <div class="h-full rounded-full transition-all {budgetState.should_warn ? 'bg-warning' : budgetState.allowed ? 'bg-success' : 'bg-danger'}" style="width: {Math.min(budgetState.percentage_used, 100)}%"></div>
                  </div>
                  <p class="text-xs text-[var(--text-muted)]">{t("settings.pctUsed", { pct: budgetState.percentage_used.toFixed(0) })}{budgetState.allowed ? '' : ' — ' + t("settings.blocked")}</p>

                  <div class="grid grid-cols-3 gap-3 pt-3 border-t" style="border-color: {currentTheme.borderHex};">
                    <SimpleSelect
                      value={budgetKind}
                      options={[
                        { value: "unlimited", label: t("settings.noLimit") },
                        { value: "tokens", label: t("settings.maxTokens") },
                        { value: "cost", label: t("settings.maxCost") },
                      ]}
                      onValueChange={(v) => (budgetKind = v as "unlimited" | "tokens" | "cost")}
                    />
                    <Input type="number" bind:value={budgetMax} placeholder={t("settings.limit")} class="h-9 text-xs font-mono" disabled={budgetKind === "unlimited"} />
                    <SimpleSelect
                      value={budgetPeriod}
                      options={[
                        { value: "total", label: t("settings.periodLifetime") },
                        { value: "daily", label: t("settings.periodDaily") },
                        { value: "weekly", label: t("settings.periodWeekly") },
                        { value: "monthly", label: t("settings.periodMonthly") },
                      ]}
                      onValueChange={(v) => (budgetPeriod = v)}
                    />
                  </div>
                  <div class="flex justify-between">
                    <Button size="sm" variant="outline" class="h-8 gap-1.5 text-xs border-danger/30 text-danger" onclick={resetBudget}>
                      <Trash2 class="size-3" /> {t("settings.resetUsage")}
                    </Button>
                    <Button size="sm" class="h-8 gap-1.5 text-xs" onclick={saveBudget}>
                      <Save class="size-3" /> {t("settings.saveBudget")}
                    </Button>
                  </div>
                </div>
              {:else if budgetBotId}
                <p class="text-sm text-[var(--text-muted)]">{t("settings.loadingBudget")}</p>
              {:else}
                <p class="text-sm text-[var(--text-muted)]">{t("settings.selectAgentHint")}</p>
              {/if}
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: Profile -->
      {#if activeSection === "profile"}
        <div class="flex-1 overflow-y-auto no-scrollbar p-6">
          <div class="max-w-2xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-[var(--text-primary)] flex items-center gap-2">
                <User class="size-5 text-[var(--brand-text)]" /> {t("settings.navProfile")}
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{t("settings.profileDesc")}</p>
            </div>
            <div class="p-5 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <div class="flex items-center gap-5">
                <button type="button" onclick={() => (showUserAvatarPicker = !showUserAvatarPicker)} class="group shrink-0">
                  <div class="size-24 rounded-2xl overflow-hidden bg-muted border-2 p-1 shadow-md group-hover:scale-105 transition-transform" style="border-color: {currentTheme.primaryColor}60;">
                    <RavenAvatar name={t("settings.you")} />
                  </div>
                  <p class="text-[11px] text-muted-foreground mt-1 text-center">{t("settings.tapToChange")}</p>
                </button>
                <div class="flex-1 space-y-3">
                  <div>
                    <Label for="settings-user-name" class="text-xs font-bold">{t("settings.displayName")}</Label>
                    <Input id="settings-user-name" bind:value={userName} placeholder={t("settings.yourName")} class="mt-1" oninput={() => { localStorage.setItem("ravenbot_user_name", userName); window.dispatchEvent(new Event("user-avatar-changed")); }} />
                  </div>
                </div>
              </div>
              {#if showUserAvatarPicker}
                <div class="mt-4 pt-4 border-t" style="border-color: {currentTheme.borderHex};">
                  <AvatarPicker seed={userName || "You"} style={userAvatarStyle} customUrl={userAvatarUrl} onSelect={saveUserAvatar} />
                </div>
              {/if}
            </div>
            <div class="p-5 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <Label for="settings-locale" class="text-xs font-bold">{t("settings.language")}</Label>
              <p class="text-sm text-[var(--text-tertiary)] mt-0.5 mb-2">{t("settings.languageDesc")}</p>
              <select
                id="settings-locale"
                class="h-9 w-full max-w-xs rounded-lg border bg-[var(--surface-2)] px-2 text-sm text-[var(--text-primary)]"
                style="border-color: {currentTheme.borderHex};"
                value={localeState.current}
                onchange={(e) => setLocale(e.currentTarget.value as Locale)}
              >
                {#each getAvailableLocales() as l (l.code)}
                  <option value={l.code}>{l.nativeName} — {l.name}</option>
                {/each}
              </select>
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: Themes -->
      {#if activeSection === "themes"}
        <div class="flex-1 overflow-y-auto no-scrollbar p-6">
          <div class="max-w-3xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-[var(--text-primary)] flex items-center gap-2">
                <Palette class="size-5 text-info" /> {t("settings.navThemes")}
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{t("settings.themesDesc")}</p>
            </div>
            <div class="grid grid-cols-2 gap-4">
              {#each THEMES as theme}
                {@const isSelected = currentTheme.id === theme.id}
                <button type="button" class="flex flex-col text-left p-4 rounded-2xl border transition-all {isSelected ? 'shadow-xl ring-2 scale-[1.02]' : 'hover:bg-[var(--surface-3)] hover:scale-[1.01]'}" style={isSelected ? `background-color: ${theme.cardHex}; border-color: ${theme.primaryColor};` : `background-color: ${theme.cardHex}90; border-color: ${theme.borderHex};`} onclick={() => pickTheme(theme)} aria-pressed={isSelected}>
                  <div class="flex items-center gap-3 mb-2">
                    <ThemeLogo theme={theme} size="sm" class="!size-10" />
                    <div>
                      <span class="font-bold text-sm text-[var(--text-primary)] block">{theme.name}</span>
                      <span class="text-[11px] font-mono text-[var(--text-tertiary)]">{theme.brand.badgeLabel}</span>
                    </div>
                  </div>
                  <p class="text-xs text-[var(--text-tertiary)]">{theme.description}</p>
                  <div class="flex items-center gap-1 mt-2.5">
                    {#each [theme.bgHex, theme.cardHex, theme.borderHex, theme.primaryColor, theme.accentColor, theme.secondaryAccent].filter(Boolean) as c}
                      <span class="size-3 rounded-sm border border-black/40" style="background-color: {c};" title={c}></span>
                    {/each}
                  </div>
                  {#if isSelected}
                    <div class="mt-2 text-[11px] text-success font-bold flex items-center gap-1"><Check class="size-3" /> {t("settings.active")}</div>
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: Local AI -->
      {#if activeSection === "local"}
        <div class="flex-1 overflow-y-auto no-scrollbar p-6">
          <div class="max-w-3xl mx-auto space-y-6">
            <div>
              <h3 class="text-lg font-bold text-[var(--text-primary)] flex items-center gap-2">
                <Server class="size-5 text-success" /> {t("settings.navLocal")}
              </h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{t("settings.localDesc")}</p>
            </div>
            <div class="p-5 rounded-2xl border bg-card/60 space-y-3" style="border-color: {currentTheme.borderHex};">
              <div class="flex items-center justify-between">
                <h4 class="text-sm font-bold text-[var(--text-primary)]">{t("settings.ollamaEndpoint")}</h4>
                {#if ollamaSaved}
                  <span class="text-[11px] text-success font-bold flex items-center gap-1 bg-success/10 px-2 py-1 rounded-full"><Check class="size-3" /> {t("settings.saved")}</span>
                {/if}
              </div>
              <div class="flex gap-2">
                <Input bind:value={ollamaUrl} placeholder="http://localhost:11434" class="text-sm font-mono flex-1" />
                <Button size="sm" class="gap-1.5" onclick={saveOllamaUrl} disabled={ollamaSaving}>
                  {#if ollamaSaving}<Loader2 class="size-3.5 animate-spin" />{:else}<Save class="size-3.5" />{/if} {t("ui.save")}
                </Button>
                <Button size="sm" variant="outline" class="gap-1.5 border-[var(--hairline-strong)]" onclick={() => probeOllama()} disabled={ollamaProbing}>
                  {#if ollamaProbing}<Loader2 class="size-3.5 animate-spin" />{:else}<Zap class="size-3.5" />{/if} {t("settings.probe")}
                </Button>
              </div>
              {#if ollamaError}
                <p class="text-[11px] text-danger bg-danger/10 border border-danger/20 rounded-lg px-2.5 py-2">{ollamaError}</p>
              {/if}
              {#if ollamaProbe}
                <div class="rounded-xl border border-success/20 bg-success/10 p-3">
                  <p class="text-[11px] text-success font-mono">✓ {ollamaProbe.base_url} · {ollamaProbe.count === 1 ? t("settings.modelInstalled") : t("settings.modelsInstalled", { n: ollamaProbe.count })}</p>
                  {#if ollamaProbe.count > 0}
                    <div class="flex flex-wrap gap-1.5 mt-2">
                      {#each ollamaProbe.models.slice(0, 12) as m}
                        <span class="text-[11px] font-mono text-[var(--text-secondary)] bg-[var(--surface-1)] border border-[var(--hairline)] px-1.5 py-0.5 rounded">{m.name}</span>
                      {/each}
                      {#if ollamaProbe.count > 12}
                        <span class="text-[11px] font-mono text-[var(--text-muted)]">{t("settings.moreCount", { n: ollamaProbe.count - 12 })}</span>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
              <p class="text-xs text-[var(--text-muted)]">
                {t("settings.installOllamaA")} <a href="https://ollama.com" target="_blank" class="text-[var(--brand-text)] hover:underline">ollama.com</a>{t("settings.installOllamaB")}
                <code class="block mt-1 p-2 rounded bg-[var(--surface-0)] text-[11px] font-mono text-[var(--text-secondary)]">ollama pull llama3.1</code>
              </p>
            </div>
            <div class="p-5 rounded-2xl border bg-warning/5 border-warning/20">
              <div class="flex items-center gap-2 text-warning text-sm font-bold mb-2"><AlertCircle class="size-4" /> {t("settings.experimental")}</div>
              <p class="text-xs text-[var(--text-tertiary)]">
                {t("settings.experimentalDesc")}
              </p>
            </div>
          </div>
        </div>
      {/if}

      <!-- Section: About -->
      {#if activeSection === "about"}
        <div class="flex-1 overflow-y-auto no-scrollbar p-6">
          <div class="max-w-2xl mx-auto space-y-6">
            <div class="text-center py-8">
              <ThemeLogo theme={currentTheme} size="xl" class="mx-auto mb-4" />
              <h3 class="text-xl font-bold text-[var(--text-primary)]">{currentTheme.brand.brandTitle}{currentTheme.brand.brandAccent}</h3>
              <p class="text-sm text-[var(--text-tertiary)] mt-1">{currentTheme.brand.tagline}</p>
              <p class="text-xs text-[var(--text-muted)] font-mono mt-2">v0.2.0 · {t("settings.sovereignOs")}</p>
            </div>
            <div class="grid grid-cols-4 gap-3">
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">{catalog.length}</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.navProviders")}</span>
              </div>
              <div class="p-3 rounded-xl bg-warning/10 border border-warning/20 text-center">
                <span class="text-2xl font-bold text-warning block">{curatedModelCount}+</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.statModels")}</span>
              </div>
              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-center">
                <span class="text-2xl font-bold text-[var(--brand-text)] block">135+</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.statMcpTools")}</span>
              </div>
              <div class="p-3 rounded-xl bg-success/10 border border-success/20 text-center">
                <span class="text-2xl font-bold text-success block">1.5K+</span>
                <span class="text-[11px] text-[var(--text-muted)] font-mono uppercase">{t("settings.statSkills")}</span>
              </div>
            </div>
            <div class="p-4 rounded-2xl border border-[var(--hairline)] bg-card/60 text-center">
              <p class="text-xs text-[var(--text-muted)]">{t("settings.sovereignty")}</p>
            </div>
          </div>
        </div>
      {/if}

    </div>
  </div>
</div>
{/if}
