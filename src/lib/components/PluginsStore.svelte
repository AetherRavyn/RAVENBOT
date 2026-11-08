<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t, type TranslationKey } from "$lib/i18n";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import PluginLogo from "$lib/components/PluginLogo.svelte";
  import { notify } from "$lib/toast";
  import { untrack } from "svelte";
  import {
    Plug,
    Search,
    RefreshCw,
    Plus,
    Check,
    Globe,
    KeyRound,
    ChevronDown,
    AlertTriangle,
    CheckCircle2,
    Trash2,
    X,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    open: boolean;
    onClose: () => void;
  }

  interface PluginSummary {
    id: string;
    name: string;
    description: string;
    logo: string;
    enabled_global: boolean;
    operations: number;
    has_credential: boolean;
    requires_auth: boolean;
    source_url?: string | null;
    base_url_override?: string | null;
    credential_header_hint?: string | null;
  }

  interface PluginOperation {
    id: string;
    method: string;
    path: string;
    description: string;
    input_schema: any;
  }

  interface PluginDetails {
    summary: PluginSummary;
    operations: PluginOperation[];
    credential_header?: string | null;
  }

  interface CatalogEntry {
    id: string;
    name: string;
    description: string;
    spec_url: string;
    category: string;
    categories: string[];
    provider: string;
    logo: string;
    version: string;
    openapi_version: string;
    requires_auth: boolean | null;
    featured: boolean;
    source: string;
  }

  interface CatalogCategory {
    id: string;
    label: string;
    count: number;
  }

  interface CatalogPage {
    entries: CatalogEntry[];
    total: number;
    catalog_total: number;
    categories: CatalogCategory[];
    source: string;
    fetched_at: string;
    stale: boolean;
    warning?: string | null;
    has_more: boolean;
  }

  interface SyncReport {
    verified: number;
    removed: number;
    failed: { plugin_id: string; error: string }[];
  }

  let { bot, open, onClose }: Props = $props();

  let plugins = $state<PluginSummary[]>([]);
  let catalog = $state<CatalogEntry[]>([]);
  let catalogQuery = $state("");
  let catalogCategory = $state("all");
  let catalogCategories = $state<CatalogCategory[]>([]);
  let catalogTotal = $state(0);
  let catalogGrandTotal = $state(0);
  let catalogHasMore = $state(false);
  let catalogLoading = $state(false);
  let catalogRefreshing = $state(false);
  let catalogError = $state<string | null>(null);
  let catalogSource = $state("cache");
  let catalogFetchedAt = $state("");
  let catalogStale = $state(false);
  let catalogWarning = $state<string | null>(null);
  let catalogRequestId = 0;
  let catalogTimer: ReturnType<typeof setTimeout> | null = null;
  let catalogTouched = false;
  let lastCatalogKey = "";
  let installingCatalog = $state<string | null>(null);
  let botPlugins = $state<Set<string>>(new Set());
  let globalPlugins = $state<Set<string>>(new Set());
  let query = $state("");
  let selectedCategory = $state("All");
  let customUrl = $state("");
  let importing = $state(false);
  let syncing = $state(false);
  let expanded = $state<string | null>(null);
  let details = $state<Record<string, PluginDetails>>({});
  let loadingDetails = $state<string | null>(null);
  let headerDraft = $state<Record<string, string>>({});
  let tokenDraft = $state<Record<string, string>>({});
  let baseUrlDraft = $state<Record<string, string>>({});
  let savingKey = $state<string | null>(null);
  let notice = $state<{ tone: "success" | "warning"; text: string } | null>(null);

  const categories = [
    { id: "All", labelKey: "store.catAll" },
    { id: "productivity", labelKey: "store.catProductivity", matches: ["gmail", "calendar", "drive", "notion", "outlook", "mail"] },
    { id: "dev", labelKey: "store.catDev", matches: ["github", "linear", "jira", "supabase", "aws", "context7", "gitlab", "bitbucket"] },
    { id: "chat", labelKey: "store.catChat", matches: ["slack", "discord", "telegram", "twitter", "youtube", "zoom"] },
    { id: "sales", labelKey: "store.catSales", matches: ["stripe", "hubspot", "salesforce", "trello", "asana"] },
    { id: "custom", labelKey: "store.catCustom", matches: [] },
  ];

  function categoryMatches(plugin: PluginSummary, categoryId: string): boolean {
    if (categoryId === "All") return true;
    const category = categories.find((entry) => entry.id === categoryId);
    if (!category) return true;
    const haystack = `${plugin.id} ${plugin.name}`.toLowerCase();
    if (category.id === "custom") {
      const known = categories
        .filter((entry) => entry.matches && entry.matches.length > 0)
        .flatMap((entry) => entry.matches ?? []);
      return !known.some((keyword) => haystack.includes(keyword));
    }
    return (category.matches ?? []).some((keyword) => haystack.includes(keyword));
  }

  const filtered = $derived(
    plugins.filter((plugin) => {
      const needle = query.trim().toLowerCase();
      const matchesQuery =
        !needle ||
        plugin.id.toLowerCase().includes(needle) ||
        plugin.name.toLowerCase().includes(needle) ||
        plugin.description.toLowerCase().includes(needle);
      return matchesQuery && categoryMatches(plugin, selectedCategory);
    }),
  );

  const totalOperations = $derived(
    plugins.reduce((total, plugin) => total + plugin.operations, 0),
  );
  const installedSources = $derived(
    new Set(
      plugins
        .map((plugin) => (plugin.source_url ?? "").replace(/\/$/, ""))
        .filter(Boolean),
    ),
  );
  const readyCount = $derived(
    plugins.filter((plugin) => plugin.operations > 0 && (!plugin.requires_auth || plugin.has_credential)).length,
  );

  function statusFor(plugin: PluginSummary): {
    label: string;
    tone: "success" | "warning" | "muted";
  } {
    if (plugin.operations === 0) {
      return { label: t("store.noOperations"), tone: "muted" };
    }
    if (plugin.requires_auth && !plugin.has_credential) {
      return { label: t("store.needsKey"), tone: "warning" };
    }
    return { label: t("store.ready"), tone: "success" };
  }

  function sourceLabel(plugin: PluginSummary): string {
    if (!plugin.source_url) return t("store.addedManually");
    try {
      return new URL(plugin.source_url).hostname.replace(/^www\./, "");
    } catch {
      return plugin.source_url;
    }
  }

  function parameterCount(operation: PluginOperation): number {
    const properties = operation.input_schema?.properties;
    return properties && typeof properties === "object" ? Object.keys(properties).length : 0;
  }

  function catalogCategoryLabel(entry: CatalogEntry): string {
    return (
      catalogCategories.find((category) => category.id === entry.category)?.label ??
      entry.category.replaceAll("_", " ")
    );
  }

  function catalogAuthLabel(entry: CatalogEntry): string {
    if (entry.requires_auth === true) return t("store.keyRequired");
    if (entry.requires_auth === false) return t("store.publicTool");
    return t("store.catalogAuthOnInstall");
  }

  function credentialHeaderFor(loaded: PluginDetails): string {
    return (
      loaded.credential_header ||
      loaded.summary.credential_header_hint ||
      "Authorization"
    );
  }

  function catalogUpdatedLabel(): string {
    if (!catalogFetchedAt) return "";
    const date = new Date(catalogFetchedAt);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }

  function catalogKey(): string {
    return `${catalogQuery.trim().toLowerCase()}|${catalogCategory}`;
  }

  async function loadCatalog(refresh = false, append = false) {
    const key = catalogKey();
    if (!append && !refresh && key === lastCatalogKey && catalog.length > 0) {
      return;
    }
    const requestId = ++catalogRequestId;
    catalogLoading = true;
    catalogRefreshing = refresh;
    if (!append) {
      catalogError = null;
      catalogWarning = null;
    }
    try {
      const page = await invoke<CatalogPage>("search_plugin_catalog", {
        query: catalogQuery.trim() || null,
        category: catalogCategory === "all" ? null : catalogCategory,
        limit: 48,
        offset: append ? catalog.length : 0,
        refresh,
      });
      if (requestId !== catalogRequestId) return;
      catalog = append ? [...catalog, ...page.entries] : page.entries;
      catalogCategories = page.categories;
      catalogTotal = page.total;
      catalogGrandTotal = page.catalog_total;
      catalogHasMore = page.has_more;
      catalogSource = page.source;
      catalogFetchedAt = page.fetched_at;
      catalogStale = page.stale;
      catalogWarning = page.warning ?? null;
      lastCatalogKey = key;
    } catch (error) {
      if (requestId !== catalogRequestId) return;
      catalogError = String(error);
      if (!append) {
        catalog = [];
        catalogTotal = 0;
        catalogHasMore = false;
      }
    } finally {
      if (requestId === catalogRequestId) {
        catalogLoading = false;
        catalogRefreshing = false;
      }
    }
  }

  async function load() {
    try {
      plugins = await invoke<PluginSummary[]>("list_plugins", { query: null });
      globalPlugins = new Set(
        (await invoke<[string, string, string, string][]>("list_global_plugins")).map((row) => row[0]),
      );
      if (bot?.id) {
        botPlugins = new Set(await invoke<string[]>("list_bot_plugins", { botId: bot.id }));
      }
    } catch (e) {
      notify(`Could not load plugins: ${String(e)}`, "error");
    }
  }

  async function sync() {
    syncing = true;
    notice = null;
    try {
      const report = await invoke<SyncReport>("sync_plugins");
      await load();
      const parts = [
        t("store.syncVerified", { n: report.verified }),
        report.removed > 0 ? t("store.syncRemoved", { n: report.removed }) : "",
        report.failed.length > 0 ? t("store.syncFailed", { n: report.failed.length }) : "",
      ].filter(Boolean);
      notice = {
        tone: report.failed.length > 0 ? "warning" : "success",
        text: parts.join(" · "),
      };
    } catch (e) {
      notify(`Verification failed: ${String(e)}`, "error");
    } finally {
      syncing = false;
    }
  }

  async function importPlugin() {
    const url = customUrl.trim();
    if (!url) return;
    importing = true;
    notice = null;
    try {
      const imported = await invoke<PluginDetails>("import_openapi_plugin", {
        manifestUrl: url,
      });
      customUrl = "";
      await load();
      expanded = imported.summary.id;
      details = { ...details, [imported.summary.id]: imported };
      headerDraft[imported.summary.id] = credentialHeaderFor(imported);
      tokenDraft[imported.summary.id] = "";
      baseUrlDraft[imported.summary.id] =
        imported.summary.base_url_override || "";
      notice = {
        tone: "success",
        text: t("store.imported", {
          name: imported.summary.name,
          n: imported.operations.length,
        }),
      };
    } catch (e) {
      notify(`${t("store.importFailed")}${String(e)}`, "error");
    } finally {
      importing = false;
    }
  }

  async function installCatalog(entry: CatalogEntry) {
    if (installingCatalog) return;
    installingCatalog = entry.id;
    notice = null;
    try {
      const imported = await invoke<PluginDetails>("install_catalog_plugin", {
        catalogId: entry.id,
        specUrl: entry.spec_url,
      });
      await load();
      expanded = imported.summary.id;
      details = { ...details, [imported.summary.id]: imported };
      headerDraft[imported.summary.id] = credentialHeaderFor(imported);
      tokenDraft[imported.summary.id] = "";
      baseUrlDraft[imported.summary.id] =
        imported.summary.base_url_override || "";
      notice = {
        tone: "success",
        text: t("store.imported", {
          name: imported.summary.name,
          n: imported.operations.length,
        }),
      };
    } catch (e) {
      notify(`${t("store.importFailed")}${String(e)}`, "error");
    } finally {
      installingCatalog = null;
    }
  }

  async function toggleGlobal(plugin: PluginSummary) {
    const enabled = !globalPlugins.has(plugin.id);
    try {
      await invoke("toggle_plugin_global", { pluginId: plugin.id, enabled });
      const next = new Set(globalPlugins);
      if (enabled) next.add(plugin.id);
      else next.delete(plugin.id);
      globalPlugins = next;
      plugins = plugins.map((entry) =>
        entry.id === plugin.id ? { ...entry, enabled_global: enabled } : entry,
      );
    } catch (e) {
      notify(`Could not update global scope: ${String(e)}`, "error");
    }
  }

  async function toggle(plugin: PluginSummary) {
    if (!bot?.id) return;
    if (plugin.operations === 0) {
      notify(t("store.importFirst"), "error");
      return;
    }
    const enabled = !botPlugins.has(plugin.id);
    try {
      await invoke("toggle_bot_plugin", {
        botId: bot.id,
        pluginId: plugin.id,
        enabled,
      });
      const next = new Set(botPlugins);
      if (enabled) next.add(plugin.id);
      else next.delete(plugin.id);
      botPlugins = next;
    } catch (e) {
      notify(`Could not update this agent: ${String(e)}`, "error");
    }
  }

  async function toggleDetails(plugin: PluginSummary) {
    if (expanded === plugin.id) {
      expanded = null;
      return;
    }
    expanded = plugin.id;
    if (!details[plugin.id]) {
      loadingDetails = plugin.id;
      try {
        const loaded = await invoke<PluginDetails>("get_plugin_details", {
          pluginId: plugin.id,
        });
        details = { ...details, [plugin.id]: loaded };
        headerDraft[plugin.id] = credentialHeaderFor(loaded);
        tokenDraft[plugin.id] = "";
        baseUrlDraft[plugin.id] = loaded.summary.base_url_override || "";
      } catch (e) {
        notify(`Could not load details: ${String(e)}`, "error");
      } finally {
        loadingDetails = null;
      }
    } else {
      headerDraft[plugin.id] = credentialHeaderFor(details[plugin.id]);
      tokenDraft[plugin.id] = "";
      baseUrlDraft[plugin.id] =
        details[plugin.id].summary.base_url_override || "";
    }
  }

  async function saveBaseUrl(plugin: PluginSummary) {
    try {
      const updated = await invoke<PluginDetails>("set_plugin_base_url", {
        pluginId: plugin.id,
        baseUrl: (baseUrlDraft[plugin.id] ?? "").trim() || null,
      });
      details = { ...details, [plugin.id]: updated };
      baseUrlDraft[plugin.id] = updated.summary.base_url_override || "";
      await load();
      notice = { tone: "success", text: t("store.serverSaved") };
    } catch (e) {
      notify(`Could not save the server URL: ${String(e)}`, "error");
    }
  }

  async function saveKey(plugin: PluginSummary) {
    const value = (tokenDraft[plugin.id] ?? "").trim();
    if (!value || savingKey) return;
    savingKey = plugin.id;
    try {
      const updated = await invoke<PluginDetails>("set_plugin_credential", {
        pluginId: plugin.id,
        headerName: headerDraft[plugin.id] || "Authorization",
        headerValue: value,
      });
      details = { ...details, [plugin.id]: updated };
      tokenDraft[plugin.id] = "";
      await load();
      notice = { tone: "success", text: t("store.credentialSaved") };
    } catch (e) {
      notify(`Could not save the key: ${String(e)}`, "error");
    } finally {
      savingKey = null;
    }
  }

  async function clearKey(plugin: PluginSummary) {
    try {
      const updated = await invoke<PluginDetails>("clear_plugin_credential", {
        pluginId: plugin.id,
      });
      details = { ...details, [plugin.id]: updated };
      await load();
    } catch (e) {
      notify(`Could not remove the key: ${String(e)}`, "error");
    }
  }

  function onKeySubmit(event: SubmitEvent, plugin: PluginSummary) {
    event.preventDefault();
    void saveKey(plugin);
  }

  $effect(() => {
    if (open && bot) void load();
  });
  $effect(() => {
    if (!open) return;
    // Read both filters so changing either schedules a new search.
    const queryKey = catalogQuery;
    const categoryKey = catalogCategory;
    const key = `${queryKey}|${categoryKey}`;
    void key;
    if (!catalogTouched) {
      catalogTouched = true;
      // The loader reads catalog state synchronously; keep those reads out
      // of this effect's dependency set so a result cannot re-trigger it.
      untrack(() => {
        void loadCatalog();
      });
      return;
    }
    if (catalogTimer) clearTimeout(catalogTimer);
    catalogTimer = setTimeout(() => {
      catalogTimer = null;
      untrack(() => {
        void loadCatalog();
      });
    }, 240);
    return () => {
      if (catalogTimer) {
        clearTimeout(catalogTimer);
        catalogTimer = null;
      }
    };
  });
  $effect(() => {
    if (!open) {
      expanded = null;
      notice = null;
      query = "";
      selectedCategory = "All";
      customUrl = "";
      catalogQuery = "";
      catalogCategory = "all";
      catalogError = null;
      catalogStale = false;
      catalogRequestId += 1;
      catalogLoading = false;
      catalogRefreshing = false;
      catalogTouched = false;
      lastCatalogKey = "";
    }
  });
</script>

<Dialog.Root {open} onOpenChange={(value) => !value && onClose()}>
  <Dialog.Content showCloseButton={false} class="plugin-modal">
    <header class="plugin-modal__header">
      <div class="plugin-modal__identity">
        <span class="plugin-modal__mark" aria-hidden="true">
          <Plug class="size-5" strokeWidth={1.8} />
        </span>
        <div class="plugin-modal__heading">
          <Dialog.Title class="plugin-modal__title">
            {t("store.title", { name: bot?.name ?? "" })}
          </Dialog.Title>
          <Dialog.Description class="plugin-modal__subtitle">
            {t("store.desc", { name: bot?.name ?? "" })}
          </Dialog.Description>
        </div>
      </div>

      <div class="plugin-modal__header-actions">
        <Button
          size="sm"
          variant="outline"
          class="plugin-button plugin-button--quiet"
          onclick={() => void sync()}
          disabled={syncing}
          title={t("store.verifyHint")}
        >
          <RefreshCw class="size-3.5 {syncing ? 'animate-spin' : ''}" />
          {syncing ? t("store.syncing") : t("store.verify")}
        </Button>
        <button
          type="button"
          class="plugin-modal__close"
          aria-label={t("ui.close")}
          onclick={onClose}
        >
          <X class="size-4" />
        </button>
      </div>
    </header>

    <div class="plugin-statusbar" role="status">
      <span class="plugin-statusbar__dot" data-tone={readyCount > 0 ? "success" : "muted"} aria-hidden="true"></span>
      <span class="plugin-statusbar__item"><strong>{plugins.length}</strong> {t("store.apps")}</span>
      <span class="plugin-statusbar__divider">·</span>
      <span class="plugin-statusbar__item"><strong>{totalOperations}</strong> {t("store.operations")}</span>
      <span class="plugin-statusbar__divider">·</span>
      <span class="plugin-statusbar__item"><strong>{readyCount}</strong> {t("store.ready")}</span>
      <span class="plugin-statusbar__divider">·</span>
      <span class="plugin-statusbar__item"><strong>{botPlugins.size}</strong> {t("store.equipped")}</span>
    </div>

    <div class="plugin-toolbar">
      <label class="plugin-search">
        <Search class="size-3.5" strokeWidth={1.8} aria-hidden="true" />
        <input
          bind:value={query}
          type="search"
          aria-label={t("store.searchPh")}
          placeholder={t("store.searchPh")}
        />
      </label>

      <form
        class="plugin-import"
        onsubmit={(event) => {
          event.preventDefault();
          void importPlugin();
        }}
      >
        <input
          bind:value={customUrl}
          type="url"
          aria-label={t("store.urlPh")}
          placeholder={t("store.urlPh")}
        />
        <Button
          type="submit"
          size="sm"
          class="plugin-button plugin-button--primary"
          disabled={importing || !customUrl.trim()}
        >
          {#if importing}
            <RefreshCw class="size-3.5 animate-spin" />
            {t("store.adding")}
          {:else}
            <Plus class="size-3.5" />
            {t("store.addSpec")}
          {/if}
        </Button>
      </form>
    </div>

    <div class="plugin-categories" role="group" aria-label={t("store.categoryLabel")}>
      {#each categories as category (category.id)}
        <button
          type="button"
          class="plugin-category"
          data-active={selectedCategory === category.id ? "true" : undefined}
          aria-pressed={selectedCategory === category.id}
          onclick={() => (selectedCategory = category.id)}
        >
          {t(category.labelKey as TranslationKey)}
        </button>
      {/each}
    </div>

    {#if notice}
      <div class="plugin-notice" data-tone={notice.tone} role="status">
        {#if notice.tone === "success"}
          <CheckCircle2 class="size-3.5" strokeWidth={2} />
        {:else}
          <AlertTriangle class="size-3.5" strokeWidth={2} />
        {/if}
        <span>{notice.text}</span>
        <button type="button" class="plugin-notice__close" onclick={() => (notice = null)} aria-label={t("ui.dismiss")}>
          <X class="size-3" />
        </button>
      </div>
    {/if}

    <div class="plugin-scroll">
      <div class="plugin-installed-heading">
        <h3>{t("store.installedTitle")}</h3>
        <span class="plugin-installed-heading__count">{filtered.length}</span>
      </div>

      {#if filtered.length === 0}
        <div class="plugin-empty">
          <span class="plugin-empty__mark" aria-hidden="true"><Plug class="size-5" strokeWidth={1.7} /></span>
          <h3>{plugins.length === 0 ? t("store.emptyTitle") : t("store.noMatch")}</h3>
          <p>{plugins.length === 0 ? t("store.emptyDesc") : t("store.noMatchHint")}</p>
        </div>
      {:else}
        <div class="plugin-grid">
          {#each filtered as plugin (plugin.id)}
            {@const status = statusFor(plugin)}
            {@const isEnabled = botPlugins.has(plugin.id)}
            {@const isGlobal = globalPlugins.has(plugin.id)}
            {@const isExpanded = expanded === plugin.id}
            <article class="plugin-card" data-enabled={isEnabled ? "true" : undefined} data-expanded={isExpanded ? "true" : undefined}>
              <div class="plugin-card__head">
                <PluginLogo id={plugin.id} name={plugin.name} logoUrl={plugin.logo} size="md" />
                <div class="plugin-card__identity">
                  <h4 class="plugin-card__name">{plugin.name}</h4>
                  <span class="plugin-card__id">{plugin.id}</span>
                </div>
                <span class="plugin-chip" data-tone={status.tone}>{status.label}</span>
              </div>

              <p class="plugin-card__description">
                {plugin.description || t("store.noDesc")}
              </p>

              <div class="plugin-card__meta">
                <span class="plugin-card__meta-item">
                  <Plug class="size-3" strokeWidth={1.8} />
                  {t("store.operationCount", { n: plugin.operations })}
                </span>
                <span class="plugin-card__meta-item" title={plugin.source_url ?? ""}>
                  {sourceLabel(plugin)}
                </span>
                {#if plugin.requires_auth}
                  <span class="plugin-card__meta-item" data-tone={plugin.has_credential ? "success" : "warning"}>
                    <KeyRound class="size-3" strokeWidth={1.8} />
                    {plugin.has_credential ? t("store.keyConfigured") : t("store.keyRequired")}
                  </span>
                {/if}
              </div>

              <div class="plugin-card__actions">
                <button
                  type="button"
                  class="plugin-details-toggle"
                  aria-expanded={isExpanded}
                  onclick={() => void toggleDetails(plugin)}
                >
                  <ChevronDown class="size-3.5 {isExpanded ? 'rotate-180' : ''}" strokeWidth={1.9} />
                  {isExpanded ? t("store.hideDetails") : t("store.details")}
                </button>

                <div class="plugin-card__controls">
                  <button
                    type="button"
                    class="plugin-scope"
                    aria-pressed={isGlobal}
                    title={t("store.globalHint")}
                    onclick={() => void toggleGlobal(plugin)}
                  >
                    <Globe class="size-3" strokeWidth={1.8} />
                    {isGlobal ? t("store.globalOn") : t("store.global")}
                  </button>

                  <Button
                    size="sm"
                    variant={isEnabled ? "default" : "outline"}
                    class="plugin-button plugin-button--enable"
                    data-enabled={isEnabled ? "true" : undefined}
                    aria-pressed={isEnabled}
                    disabled={plugin.operations === 0}
                    onclick={() => void toggle(plugin)}
                  >
                    {#if isEnabled}
                      <Check class="size-3" strokeWidth={2.2} />
                      {t("store.enabled")}
                    {:else}
                      <Plus class="size-3" strokeWidth={2.2} />
                      {t("store.enable")}
                    {/if}
                  </Button>
                </div>
              </div>

              {#if isExpanded}
                <div class="plugin-details">
                  {#if loadingDetails === plugin.id}
                    <div class="plugin-details__loading">
                      <RefreshCw class="size-3.5 animate-spin" />
                      {t("store.loadingDetails")}
                    </div>
                  {:else if details[plugin.id]}
                    {@const loaded = details[plugin.id]}
                    <section class="plugin-section">
                      <div class="plugin-section__heading">
                        <span>{t("store.operationList")}</span>
                        <span class="plugin-section__count">{loaded.operations.length}</span>
                      </div>
                      <div class="plugin-operations">
                        {#each loaded.operations.slice(0, 24) as operation (operation.id)}
                          <div class="plugin-operation">
                            <span class="plugin-method" data-method={operation.method}>{operation.method}</span>
                            <span class="plugin-operation__name" title={operation.id}>{operation.id}</span>
                            <span class="plugin-operation__path" title={operation.path}>{operation.path}</span>
                            <span class="plugin-operation__params">
                              {t("store.paramCount", { n: parameterCount(operation) })}
                            </span>
                          </div>
                        {/each}
                      </div>
                      {#if loaded.operations.length > 24}
                        <p class="plugin-section__more">
                          {t("store.moreOperations", { n: loaded.operations.length - 24 })}
                        </p>
                      {/if}
                    </section>

                    <section class="plugin-section">
                      <div class="plugin-section__heading">
                        <span>{t("store.serverUrl")}</span>
                        {#if loaded.summary.base_url_override}
                          <span class="plugin-chip" data-tone="success">{t("store.serverOverridden")}</span>
                        {/if}
                      </div>
                      <form
                        class="plugin-credential"
                        onsubmit={(event) => {
                          event.preventDefault();
                          void saveBaseUrl(plugin);
                        }}
                      >
                        <label class="plugin-field plugin-field--grow">
                          <span>{t("store.serverUrl")}</span>
                          <Input
                            bind:value={baseUrlDraft[plugin.id]}
                            placeholder={t("store.serverUrlPlaceholder")}
                            autocomplete="off"
                          />
                        </label>
                        <Button
                          type="submit"
                          size="sm"
                          class="plugin-button plugin-button--quiet plugin-credential__save"
                          disabled={(baseUrlDraft[plugin.id] ?? "") === (loaded.summary.base_url_override ?? "")}
                        >
                          {t("store.saveServer")}
                        </Button>
                      </form>
                      <p class="plugin-hint">{t("store.serverHint")}</p>
                    </section>

                    <section class="plugin-section">
                      <div class="plugin-section__heading">
                        <span>{t("store.authentication")}</span>
                        {#if loaded.summary.has_credential}
                          <span class="plugin-chip" data-tone="success">
                            <CheckCircle2 class="size-3" strokeWidth={2} />
                            {t("store.keyConfigured")}
                          </span>
                        {:else if loaded.summary.requires_auth}
                          <span class="plugin-chip" data-tone="warning">{t("store.keyRequired")}</span>
                        {:else}
                          <span class="plugin-chip" data-tone="muted">{t("store.authNotDeclared")}</span>
                        {/if}
                      </div>

                      {#if loaded.summary.credential_header_hint}
                        <p class="plugin-hint plugin-hint--auth">
                          {t("store.authHeaderHint", {
                            header: loaded.summary.credential_header_hint,
                          })}
                        </p>
                      {/if}
                      {#if !loaded.summary.requires_auth && !loaded.summary.has_credential}
                        <p class="plugin-hint">{t("store.authNotDeclaredHint")}</p>
                      {/if}

                      <form class="plugin-credential" onsubmit={(event) => onKeySubmit(event, plugin)}>
                        <label class="plugin-field">
                          <span>{t("store.headerName")}</span>
                          <Input
                            bind:value={headerDraft[plugin.id]}
                            placeholder="Authorization"
                            autocomplete="off"
                          />
                        </label>
                        <label class="plugin-field plugin-field--grow">
                          <span>{t("store.token")}</span>
                          <Input
                            bind:value={tokenDraft[plugin.id]}
                            type="password"
                            placeholder={t("store.tokenPlaceholder")}
                            autocomplete="new-password"
                          />
                        </label>
                        <Button
                          type="submit"
                          size="sm"
                          class="plugin-button plugin-button--primary plugin-credential__save"
                          disabled={savingKey === plugin.id || !(tokenDraft[plugin.id] ?? "").trim()}
                        >
                          {savingKey === plugin.id ? t("store.savingKey") : t("store.saveKey")}
                        </Button>
                        {#if loaded.summary.has_credential}
                          <button
                            type="button"
                            class="plugin-credential__remove"
                            onclick={() => void clearKey(plugin)}
                          >
                            <Trash2 class="size-3" strokeWidth={1.8} />
                            {t("store.removeKey")}
                          </button>
                        {/if}
                      </form>
                      <p class="plugin-hint">
                        {t("store.credentialHint", { host: sourceLabel(plugin) })}
                      </p>
                    </section>
                  {/if}
                </div>
              {/if}
            </article>
          {/each}
        </div>
      {/if}

      <section class="plugin-catalog" aria-label={t("store.catalogTitle")}>
        <div class="plugin-catalog__heading">
          <div class="plugin-catalog__intro">
            <div class="plugin-catalog__title-row">
              <h3>{t("store.catalogTitle")}</h3>
              <span class="plugin-catalog__count" title={t("store.catalogTotalHint")}>
                {catalogGrandTotal > 0 ? catalogGrandTotal.toLocaleString() : "…"}
              </span>
            </div>
            <p>{t("store.catalogDesc")}</p>
          </div>
          <div class="plugin-catalog__source">
            <span
              class="plugin-catalog__source-dot"
              data-live={catalogSource === "live" ? "true" : undefined}
              aria-hidden="true"
            ></span>
            <span>{catalogSource === "live" ? t("store.catalogLive") : t("store.catalogCached")}</span>
            {#if catalogUpdatedLabel()}
              <span class="plugin-catalog__source-date">· {catalogUpdatedLabel()}</span>
            {/if}
            <button
              type="button"
              class="plugin-catalog__refresh"
              aria-label={t("store.catalogRefresh")}
              title={t("store.catalogRefresh")}
              disabled={catalogRefreshing}
              onclick={() => void loadCatalog(true)}
            >
              <RefreshCw class="size-3 {catalogRefreshing ? 'animate-spin' : ''}" strokeWidth={1.9} />
            </button>
          </div>
        </div>

        <div class="plugin-catalog__toolbar">
          <label class="plugin-search plugin-search--catalog">
            <Search class="size-3.5" strokeWidth={1.8} aria-hidden="true" />
            <input
              bind:value={catalogQuery}
              type="search"
              aria-label={t("store.catalogSearchPh")}
              placeholder={t("store.catalogSearchPh")}
            />
            {#if catalogQuery}
              <button
                type="button"
                class="plugin-search__clear"
                aria-label={t("store.catalogClear")}
                onclick={() => (catalogQuery = "")}
              >
                <X class="size-3" />
              </button>
            {/if}
          </label>

          <label class="plugin-catalog__select">
            <span class="sr-only">{t("store.catalogCategory")}</span>
            <select bind:value={catalogCategory} aria-label={t("store.catalogCategory")}>
              <option value="all">{t("store.catalogAllCategories")}</option>
              {#each catalogCategories as category (category.id)}
                <option value={category.id}>{category.label} ({category.count.toLocaleString()})</option>
              {/each}
            </select>
            <ChevronDown class="size-3" strokeWidth={1.9} aria-hidden="true" />
          </label>
        </div>

        {#if catalogWarning || catalogStale}
          <div class="plugin-notice plugin-notice--compact" data-tone="warning" title={catalogWarning ?? ""}>
            <AlertTriangle class="size-3.5" strokeWidth={2} />
            <span>{t("store.catalogStaleWarning")}</span>
          </div>
        {/if}

        {#if catalogError}
          <div class="plugin-notice plugin-notice--compact" data-tone="error">
            <AlertTriangle class="size-3.5" strokeWidth={2} />
            <span>{t("store.catalogError")} <span class="plugin-notice__detail">{catalogError}</span></span>
            <button
              type="button"
              class="plugin-notice__action"
              onclick={() => void loadCatalog()}
            >
              {t("store.catalogRetry")}
            </button>
          </div>
        {/if}

        <div class="plugin-catalog__summary" role="status">
          {#if catalogLoading}
            <RefreshCw class="size-3 animate-spin" strokeWidth={2} />
            {t("store.catalogLoading")}
          {:else if catalogError}
            {t("store.catalogError")}
          {:else if catalogTotal > 0}
            {t("store.catalogShowing", {
              shown: catalog.length.toLocaleString(),
              total: catalogTotal.toLocaleString(),
            })}
          {:else}
            {t("store.catalogNoMatch")}
          {/if}
        </div>

        {#if catalog.length === 0 && !catalogError && (catalogLoading || !catalogFetchedAt)}
          <div class="plugin-catalog__grid" aria-hidden="true">
            {#each [0, 1, 2, 3, 4, 5] as index (index)}
              <div class="plugin-catalog__skeleton">
                <span class="plugin-catalog__skeleton-mark"></span>
                <span class="plugin-catalog__skeleton-copy">
                  <span></span>
                  <span></span>
                  <span></span>
                </span>
              </div>
            {/each}
          </div>
        {:else if catalog.length === 0 && !catalogError}
          <div class="plugin-catalog__empty">
            <span class="plugin-catalog__empty-mark" aria-hidden="true">
              <Search class="size-4" strokeWidth={1.8} />
            </span>
            <h4>{t("store.catalogNoMatch")}</h4>
            <p>{t("store.catalogNoMatchHint")}</p>
            <Button
              size="sm"
              variant="outline"
              class="plugin-button plugin-button--quiet"
              onclick={() => {
                catalogQuery = "";
                catalogCategory = "all";
              }}
            >
              {t("store.catalogBrowseAll")}
            </Button>
          </div>
        {:else}
          <div class="plugin-catalog__grid" aria-busy={catalogLoading}>
            {#each catalog as entry (entry.id)}
              {@const installed = installedSources.has(entry.spec_url.replace(/\/$/, ""))}
              <article class="plugin-catalog__card" data-installed={installed ? "true" : undefined}>
                <PluginLogo id={entry.id} name={entry.name} logoUrl={entry.logo} size="md" />
                <div class="plugin-catalog__copy">
                  <div class="plugin-catalog__title">
                    <h4 title={entry.name}>{entry.name}</h4>
                    <span class="plugin-category-tag">{catalogCategoryLabel(entry)}</span>
                  </div>
                  <p title={entry.description}>{entry.description}</p>
                  <span class="plugin-catalog__meta">
                    <span class="plugin-catalog__provider" title={entry.provider}>{entry.provider}</span>
                    <span class="plugin-catalog__dot">·</span>
                    <span>{entry.openapi_version === "2.0" ? "Swagger 2.0" : `OpenAPI ${entry.openapi_version}`}</span>
                    <span class="plugin-catalog__dot">·</span>
                    <span>{catalogAuthLabel(entry)}</span>
                  </span>
                </div>
                <Button
                  size="sm"
                  class="plugin-button plugin-button--enable plugin-catalog__install"
                  disabled={installingCatalog !== null || installed}
                  onclick={() => void installCatalog(entry)}
                >
                  {#if installed}
                    <Check class="size-3" strokeWidth={2.2} />
                    {t("store.catalogInstalled")}
                  {:else if installingCatalog === entry.id}
                    <RefreshCw class="size-3.5 animate-spin" />
                    {t("store.installing")}
                  {:else}
                    <Plus class="size-3.5" />
                    {t("store.install")}
                  {/if}
                </Button>
              </article>
            {/each}
          </div>

          {#if catalogHasMore}
            <div class="plugin-catalog__more">
              <Button
                size="sm"
                variant="outline"
                class="plugin-button plugin-button--quiet"
                disabled={catalogLoading}
                onclick={() => void loadCatalog(false, true)}
              >
                {#if catalogLoading}
                  <RefreshCw class="size-3.5 animate-spin" />
                {:else}
                  <ChevronDown class="size-3.5" strokeWidth={1.9} />
                {/if}
                {t("store.catalogLoadMore")}
              </Button>
            </div>
          {/if}
        {/if}
      </section>
    </div>

    <footer class="plugin-modal__footer">
      <span class="plugin-footer__summary">
        <strong>{filtered.length}</strong> {t("store.countAvailable")} · <strong>{botPlugins.size}</strong> {t("store.countEquipped", { name: bot?.name ?? "" })}
      </span>
      <Button size="sm" class="plugin-button plugin-button--primary" onclick={onClose}>
        {t("ui.done")}
      </Button>
    </footer>
  </Dialog.Content>
</Dialog.Root>
