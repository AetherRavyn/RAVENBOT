<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
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
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    open: boolean;
    onClose: () => void;
  }

  let { bot, open, onClose }: Props = $props();

  let plugins = $state<[string, string, string, string][]>([]);
  let botPlugins = $state<Set<string>>(new Set());
  let query = $state("");
  let selectedCategory = $state("All");
  let customUrl = $state("");
  let importing = $state(false);
  let syncing = $state(false);

  const categories = [
    { id: "All", label: "All Tools" },
    { id: "productivity", label: "Productivity & Mail", matches: ["gmail", "calendar", "drive", "notion", "outlook"] },
    { id: "dev", label: "Code & DevOps", matches: ["github", "linear", "jira", "supabase", "aws", "context7"] },
    { id: "chat", label: "Chat & Social", matches: ["slack", "discord", "telegram", "twitter", "youtube", "zoom"] },
    { id: "sales", label: "CRM & Finance", matches: ["stripe", "hubspot", "salesforce", "trello", "asana"] },
    { id: "custom", label: "Custom OpenAPI", matches: ["openapi", "custom"] },
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
      if (bot?.id) {
        botPlugins = new Set(await invoke("list_bot_plugins", { botId: bot.id }));
      }
    } catch (e) {
      console.error("Failed to load plugins:", e);
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
      alert("Failed to import OpenAPI spec: " + String(e));
    } finally {
      importing = false;
    }
  }

  $effect(() => {
    if (open && bot) load();
  });
</script>

<Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
  <Dialog.Content class="sm:max-w-4xl max-h-[88vh] flex flex-col bg-[#0c0c14]/98 border border-purple-500/30 shadow-[0_0_60px_rgba(147,51,234,0.25)] backdrop-blur-2xl rounded-3xl p-0 overflow-hidden text-zinc-100">
    <!-- Fixed Dialog Header -->
    <div class="px-6 pt-5 pb-3.5 border-b border-white/10 shrink-0">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="size-10 rounded-2xl bg-purple-950/70 border border-purple-800/50 flex items-center justify-center text-purple-400 shadow-md">
            <Plug class="size-5" />
          </div>
          <div>
            <Dialog.Title class="text-base font-bold flex items-center gap-2 text-white">
              Plugins & Native Tools for {bot.name}
            </Dialog.Title>
            <Dialog.Description class="text-xs text-zinc-400 mt-0.5">
              Equip {bot.name} with 1000+ native API integrations, SaaS actions, and custom OpenAPI tools.
            </Dialog.Description>
          </div>
        </div>

        <Button
          size="sm"
          variant="outline"
          class="h-8 gap-1.5 text-xs bg-[#171726] border-purple-500/30 text-purple-300 hover:bg-purple-950/40 hover:text-white cursor-pointer"
          onclick={sync}
          disabled={syncing}
        >
          <RefreshCw class="size-3.5 {syncing ? 'animate-spin' : ''}" />
          {syncing ? "Syncing Tools…" : "Sync Composio Catalog"}
        </Button>
      </div>

      <div class="mt-3 p-2.5 rounded-xl border bg-emerald-950/20 border-emerald-800/30 flex items-center gap-2 text-xs">
        <span class="size-2 rounded-full bg-emerald-500 animate-pulse"></span>
        <span class="text-zinc-300"><span class="font-medium text-white">100% In-App</span> — 3 meta tools (<code class="px-1 py-0.5 rounded bg-black/30 font-mono text-[11px]">plugin_search</code> etc.) + 1000 plugins run locally via OpenAPI, no external service.</span>
      </div>

      <!-- Search & Import Controls -->
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 mt-4">
        <!-- Search Bar -->
        <div class="relative">
          <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500 pointer-events-none" />
          <Input
            bind:value={query}
            placeholder="Search tools (e.g. Gmail, Slack, GitHub, Notion, Stripe)..."
            class="h-9 text-xs bg-[#141420] border-[#252538] pl-9 text-zinc-200 placeholder:text-zinc-500"
          />
        </div>

        <!-- Custom OpenAPI URL Import -->
        <div class="flex gap-1.5">
          <Input
            bind:value={customUrl}
            placeholder="Paste OpenAPI or ai-plugin.json URL..."
            class="h-9 text-xs bg-[#141420] border-[#252538] text-zinc-200 placeholder:text-zinc-500 flex-1"
          />
          <Button
            size="sm"
            class="h-9 gap-1.5 text-xs bg-purple-600 hover:bg-purple-500 text-white shrink-0 cursor-pointer"
            onclick={importOpenApi}
            disabled={importing || !customUrl.trim()}
          >
            <Plus class="size-3.5" />
            {importing ? "Adding…" : "Add Spec"}
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
              ? 'bg-purple-600 text-white shadow-sm'
              : 'bg-[#141422] border border-[#262638] text-zinc-400 hover:text-zinc-200 hover:bg-[#1a1a2c]'}"
            onclick={() => (selectedCategory = cat.id)}
          >
            {cat.label}
          </button>
        {/each}
      </div>
    </div>

    <!-- Scrollable Plugin Cards Grid (Fixed height prevents modal overflow) -->
    <div class="flex-1 overflow-y-auto px-6 py-4">
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {#each filtered as [id, name, desc, logo] (id)}
          {@const isEnabled = botPlugins.has(id)}
          <div
            class="p-3.5 rounded-2xl border transition-all flex flex-col justify-between group {isEnabled
              ? 'border-purple-500/80 bg-[#141026]/90 shadow-[0_0_20px_rgba(147,51,234,0.2)] ring-1 ring-purple-500/40'
              : 'border-[#1e1e2d] bg-[#0e0e18]/80 hover:border-purple-500/40 hover:bg-[#131322]'}"
          >
            <div>
              <div class="flex items-start justify-between gap-3">
                <!-- Authentic Official Vector Brand Logo -->
                <PluginLogo {id} {name} logoUrl={logo} size="md" />

                <!-- Status Badge -->
                <div class="flex items-center gap-1">
                  {#if isEnabled}
                    <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-purple-950/90 text-purple-300 border border-purple-500/50 flex items-center gap-1">
                      <CheckCircle2 class="size-3 text-purple-400" />
                      Active
                    </span>
                  {:else}
                    <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-white/5 text-zinc-400 border border-white/10">
                      Available
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Tool Title & ID -->
              <div class="mt-2.5">
                <h4 class="font-bold text-xs text-white truncate">{name}</h4>
                <span class="font-mono text-[10px] text-purple-400/80 block truncate">{id}</span>
                <p class="text-xs text-zinc-400 line-clamp-2 mt-1 leading-relaxed">
                  {desc || "Native cloud integration and tool actions"}
                </p>
              </div>
            </div>

            <!-- Card Bottom Row: Scope and Toggle Button -->
            <div class="flex items-center justify-between pt-3 mt-3 border-t border-white/5">
              <span class="text-[10px] text-zinc-500 font-mono flex items-center gap-1">
                <Shield class="size-3 text-zinc-400" />
                In-App Tool
                {#if isEnabled}
                  <span class="ml-1 text-[10px] text-emerald-400">✓ Ready</span>
                {/if}
              </span>

              <Button
                size="sm"
                variant={isEnabled ? "default" : "outline"}
                class="h-7 text-xs font-medium px-3 gap-1 cursor-pointer transition-all {isEnabled
                  ? 'bg-purple-600 text-white shadow-sm hover:bg-purple-500'
                  : 'bg-[#161626] border-[#29293e] text-zinc-300 hover:bg-[#202034] hover:text-white'}"
                onclick={() => toggle(id)}
              >
                {#if isEnabled}
                  <Check class="size-3" />
                  Enabled
                {:else}
                  <Plus class="size-3" />
                  Enable
                {/if}
              </Button>
            </div>
          </div>
        {:else}
          <div class="col-span-full py-16 text-center text-zinc-500">
            <Plug class="size-10 mx-auto mb-2 opacity-30 text-purple-400" />
            <p class="text-sm font-semibold text-zinc-300">No plugins match your filter</p>
            <p class="text-xs text-zinc-500 mt-0.5">Try searching for another service or paste a custom OpenAPI spec URL.</p>
          </div>
        {/each}
      </div>
    </div>

    <!-- Fixed Pinned Footer -->
    <div class="px-6 py-3.5 border-t border-white/10 bg-[#0a0a12] flex items-center justify-between shrink-0">
      <span class="text-xs text-zinc-400 font-mono">
        <strong>{filtered.length}</strong> available • <strong>{botPlugins.size}</strong> equipped for {bot.name}
      </span>

      <div class="flex items-center gap-2">
        <Button size="sm" class="bg-purple-600 hover:bg-purple-500 text-white font-medium text-xs px-5 h-8 rounded-xl shadow-md cursor-pointer" onclick={onClose}>
          Done
        </Button>
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
