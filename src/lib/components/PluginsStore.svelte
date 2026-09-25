<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t, type TranslationKey } from "$lib/i18n";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Card from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import PluginLogo from "$lib/components/PluginLogo.svelte";
  import { onMount } from "svelte";
  import {
    Plug,
    Search,
    RefreshCw,
    Plus,
    Check,
    Globe,
    ExternalLink,
    Sliders,
    Layers,
    Sparkles,
    Shield,
    CheckCircle2,
    X,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    open: boolean;
    onClose: () => void;
  }

  let { bot, open, onClose }: Props = $props();

  let plugins = $state<[string, string, string, string][]>([]);
  let botPlugins = $state<Set<string>>(new Set());
  // Plugins enabled for every bot (P8 global scope, list_global_plugins).
  let globalPlugins = $state<Set<string>>(new Set());
  let query = $state("");
  let selectedCategory = $state("All");
  let customUrl = $state("");
  let importing = $state(false);
  let syncing = $state(false);

  const categories = [
    { id: "All", labelKey: "store.catAll" },
    { id: "productivity", labelKey: "store.catProductivity", matches: ["gmail", "calendar", "drive", "notion", "outlook"] },
    { id: "dev", labelKey: "store.catDev", matches: ["github", "linear", "jira", "supabase", "aws", "context7"] },
    { id: "chat", labelKey: "store.catChat", matches: ["slack", "discord", "telegram", "twitter", "youtube", "zoom"] },
    { id: "sales", labelKey: "store.catSales", matches: ["stripe", "hubspot", "salesforce", "trello", "asana"] },
    { id: "custom", labelKey: "store.catCustom", matches: ["openapi", "custom"] },
  ];

  let filtered = $derived(
    plugins.filter(([id, name, desc]) => {
      const matchesQuery =
        !query ||
        id.toLowerCase().includes(query.toLowerCase()) ||
        name.toLowerCase().includes(query.toLowerCase()) ||
        desc.toLowerCase().includes(query.toLowerCase());

      if (!matchesQuery) return false;

      if (selectedCategory === "All") return true;

      const catDef = categories.find((c) => c.id === selectedCategory);
      if (!catDef || !catDef.matches) return true;

      const lowerId = id.toLowerCase();
      const lowerName = name.toLowerCase();
      return catDef.matches.some((m) => lowerId.includes(m) || lowerName.includes(m));
    })
  );

  async function load() {
    try {
      plugins = await invoke("list_plugins", { query: query || null });
      globalPlugins = new Set(
        (await invoke<[string, string, string, string][]>("list_global_plugins")).map((r) => r[0]),
      );
      if (bot?.id) {
        botPlugins = new Set(await invoke("list_bot_plugins", { botId: bot.id }));
      }
    } catch (e) {
      console.error("Failed to load plugins:", e);
    }
  }

  async function toggleGlobal(id: string) {
    const enabled = !globalPlugins.has(id);
    try {
      await invoke("toggle_plugin_global", { pluginId: id, enabled });
      if (enabled) globalPlugins.add(id);
      else globalPlugins.delete(id);
      globalPlugins = new Set(globalPlugins);
    } catch (e) {
      console.error("Global toggle error:", e);
    }
  }

  async function sync() {
    syncing = true;
    try {
      await invoke("sync_plugins");
      await load();
    } catch (e) {
      console.error("Sync error:", e);
    } finally {
      syncing = false;
    }
  }

  async function toggle(id: string) {
    if (!bot?.id) return;
    const enabled = !botPlugins.has(id);
    try {
      await invoke("toggle_bot_plugin", { botId: bot.id, pluginId: id, enabled });
      if (enabled) botPlugins.add(id);
      else botPlugins.delete(id);
      botPlugins = new Set(botPlugins);
    } catch (e) {
      console.error("Toggle error:", e);
    }
  }

  async function importOpenApi() {
    if (!customUrl.trim()) return;
    importing = true;
    try {
      await invoke("import_openapi_plugin", { manifestUrl: customUrl.trim() });
      customUrl = "";
      await load();
    } catch (e) {
      alert(t("store.importFailed") + String(e));
    } finally {
      importing = false;
    }
  }

  $effect(() => {
    if (open && bot) load();
  });
</script>

<Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
  <Dialog.Content showCloseButton={false} class="sm:max-w-4xl max-h-[88vh] flex flex-col bg-[var(--surface-1)] border border-[var(--brand)]/30  rounded-xl p-0 overflow-hidden text-[var(--text-primary)]">
    <!-- Fixed Dialog Header -->
    <div class="px-6 pt-5 pb-3.5 border-b border-[var(--hairline)] shrink-0">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="size-10 rounded-2xl bg-[var(--brand-soft)] border border-[var(--brand)]/50 flex items-center justify-center text-[var(--brand-text)] shadow-md">
            <Plug class="size-5" />
          </div>
          <div>
            <Dialog.Title class="text-base font-bold flex items-center gap-2 text-[var(--text-primary)]">
              {t("store.title", { name: bot?.name ?? "" })}
            </Dialog.Title>
            <Dialog.Description class="text-xs text-[var(--text-tertiary)] mt-0.5">
              {t("store.desc", { name: bot?.name ?? "" })}
            </Dialog.Description>
          </div>
        </div>

        <Button
          size="sm"
          variant="outline"
          class="h-8 gap-1.5 text-xs bg-[var(--surface-3)] border-[var(--brand)]/30 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] hover:text-[var(--text-primary)] cursor-pointer"
          onclick={sync}
          disabled={syncing}
        >
          <RefreshCw class="size-3.5 {syncing ? 'animate-spin' : ''}" />
          {syncing ? t("store.syncing") : t("store.sync")}
        </Button>

        <button
          type="button"
          aria-label="Close"
          onclick={onClose}
          class="absolute top-2 right-2 size-8 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer z-10"
        >
          <X class="size-4" />
        </button>
      </div>

      <div class="mt-3 p-2.5 rounded-xl border bg-success/20 border-success/30 flex items-center gap-2 text-xs">
        <span class="size-2 rounded-full bg-success animate-pulse"></span>
        <span class="text-[var(--text-secondary)]"><span class="font-medium text-[var(--text-primary)]">{t("store.inapp1")}</span>{t("store.inapp2")}<code class="px-1 py-0.5 rounded bg-black/30 font-mono text-[11px]">plugin_search</code>{t("store.inapp3")}</span>
      </div>

      <!-- Search & Import Controls -->
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 mt-4">
        <!-- Search Bar -->
        <div class="relative">
          <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)] pointer-events-none" />
          <Input
            bind:value={query}
            aria-label={t("store.searchPh")}
            placeholder={t("store.searchPh")}
            class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] pl-9 text-[var(--text-secondary)] placeholder:text-[var(--text-muted)]"
          />
        </div>

        <!-- Custom OpenAPI URL Import -->
        <div class="flex gap-1.5">
          <Input
            bind:value={customUrl}
            aria-label={t("store.urlPh")}
            placeholder={t("store.urlPh")}
            class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] placeholder:text-[var(--text-muted)] flex-1"
          />
          <Button
            size="sm"
            class="h-9 gap-1.5 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] shrink-0 cursor-pointer"
            onclick={importOpenApi}
            disabled={importing || !customUrl.trim()}
          >
            <Plus class="size-3.5" />
            {importing ? t("store.adding") : t("store.addSpec")}
          </Button>
        </div>
      </div>

      <!-- Category Filter Chips -->
      <div class="flex items-center gap-1.5 overflow-x-auto pt-3 pb-1 no-scrollbar">
        {#each categories as cat}
          {@const isSelected = selectedCategory === cat.id}
          <button
            type="button"
            class="px-3 py-1 rounded-xl text-xs font-medium transition-all shrink-0 cursor-pointer {isSelected
              ? 'bg-[var(--brand)] text-[var(--text-on-light)] shadow-sm'
              : 'bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-2)]'}"
            onclick={() => (selectedCategory = cat.id)}
            aria-pressed={isSelected}
          >
            {t(cat.labelKey as TranslationKey)}
          </button>
        {/each}
      </div>
    </div>

    <!-- Scrollable Plugin Cards Grid (Fixed height prevents modal overflow) -->
    <div class="flex-1 overflow-y-auto px-6 py-4">
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {#each filtered as [id, name, desc, logo] (id)}
          {@const isEnabled = botPlugins.has(id)}
          {@const isGlobal = globalPlugins.has(id)}
          <div
            class="p-3.5 rounded-2xl border transition-all flex flex-col justify-between group {isEnabled
              ? 'border-[var(--brand)]/80 bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/40'
              : 'border-[var(--hairline)] bg-[var(--surface-1)]/80 hover:border-[var(--brand)]/40 hover:bg-[var(--surface-2)]'}"
          >
            <div>
              <div class="flex items-start justify-between gap-3">
                <!-- Authentic Official Vector Brand Logo -->
                <PluginLogo {id} {name} logoUrl={logo} size="md" />

                <!-- Status Badge -->
                <div class="flex items-center gap-1">
                  {#if isEnabled}
                    <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/50 flex items-center gap-1">
                      <CheckCircle2 class="size-3 text-[var(--brand-text)]" />
                      {t("store.active")}
                    </span>
                  {:else}
                    <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-[var(--surface-2)] text-[var(--text-tertiary)] border border-[var(--hairline)]">
                      {t("store.available")}
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Tool Title & ID -->
              <div class="mt-2.5">
                <h4 class="font-bold text-xs text-[var(--text-primary)] truncate">{name}</h4>
                <span class="font-mono text-[10px] text-[var(--brand-text)]/80 block truncate">{id}</span>
                <p class="text-xs text-[var(--text-tertiary)] line-clamp-2 mt-1 leading-relaxed">
                  {desc || t("store.noDesc")}
                </p>
              </div>
            </div>

            <!-- Card Bottom Row: Scope, Global toggle, Enable Button -->
            <div class="flex items-center justify-between pt-3 mt-3 border-t border-[var(--hairline)] gap-2">
              <span class="text-[10px] text-[var(--text-muted)] font-mono flex items-center gap-1 min-w-0">
                <Shield class="size-3 text-[var(--text-tertiary)] shrink-0" />
                {t("store.inappTool")}
                {#if isEnabled}
                  <span class="text-[10px] text-success">{t("store.ready")}</span>
                {/if}
              </span>

              <div class="flex items-center gap-1.5 shrink-0">
                <button
                  type="button"
                  onclick={() => toggleGlobal(id)}
                  aria-pressed={isGlobal}
                  title={t("store.globalHint")}
                  class="h-7 px-2 rounded-lg text-[10px] font-mono flex items-center gap-1 transition-all cursor-pointer {isGlobal
                    ? 'bg-success/15 text-success border border-success/40'
                    : 'bg-[var(--surface-2)] text-[var(--text-muted)] border border-[var(--hairline)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                >
                  <Globe class="size-3" />
                  {isGlobal ? t("store.globalOn") : t("store.global")}
                </button>

                <Button
                  size="sm"
                  variant={isEnabled ? "default" : "outline"}
                  aria-pressed={isEnabled}
                  class="h-7 text-xs font-medium px-3 gap-1 cursor-pointer transition-all {isEnabled
                    ? 'bg-[var(--brand)] text-[var(--text-on-light)] shadow-sm hover:bg-[var(--brand-hover)]'
                    : 'bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--hairline)] hover:text-[var(--text-primary)]'}"
                  onclick={() => toggle(id)}
                >
                  {#if isEnabled}
                    <Check class="size-3" />
                    {t("store.enabled")}
                  {:else}
                    <Plus class="size-3" />
                    {t("store.enable")}
                  {/if}
                </Button>
              </div>
            </div>
          </div>
        {:else}
          <div class="col-span-full py-16 text-center text-[var(--text-muted)]">
            <Plug class="size-10 mx-auto mb-2 opacity-30 text-[var(--brand-text)]" />
            <p class="text-sm font-semibold text-[var(--text-secondary)]">{t("store.empty")}</p>
            <p class="text-xs text-[var(--text-muted)] mt-0.5">{t("store.emptyHint")}</p>
          </div>
        {/each}
      </div>
    </div>

    <!-- Fixed Pinned Footer -->
    <div class="px-6 py-3.5 border-t border-[var(--hairline)] bg-[var(--surface-1)] flex items-center justify-between shrink-0">
      <span class="text-xs text-[var(--text-tertiary)] font-mono">
        <strong>{filtered.length}</strong> {t("store.countAvailable")} • <strong>{botPlugins.size}</strong> {t("store.countEquipped", { name: bot?.name ?? "" })}
      </span>

      <div class="flex items-center gap-2">
        <Button size="sm" class="bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium text-xs px-5 h-8 rounded-xl shadow-md cursor-pointer" onclick={onClose}>
          {t("ui.done")}
        </Button>
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
