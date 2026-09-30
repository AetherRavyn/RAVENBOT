<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Badge } from "$lib/components/ui/badge";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { Textarea } from "$lib/components/ui/textarea";
  import { getDiceBearUrl, cn } from "$lib/utils";
  import {
    Wrench,
    Server,
    Search,
    Plus,
    Key,
    Zap,
    Trash2,
    Check,
    Copy,
    RefreshCw,
    Globe,
    Shield,
    Terminal,
    AlertCircle,
    Eye,
    EyeOff,
    ExternalLink,
    CheckCircle2,
    Sliders,
    Layers,
    Sparkles,
    Bot as BotIcon,
    X,
    Code,
    Database,
    TrendingUp,
    MessageSquare,
    Home,
  } from "@lucide/svelte";
  import ConnectorIcon from "$lib/components/ConnectorIcon.svelte";

  interface Props {
    bot?: any;
    open: boolean;
    onClose: () => void;
  }

  let { bot = null, open, onClose }: Props = $props();

  interface McpServerSummary {
    id: string;
    name: string;
    description: string;
    category: string;
    icon: string;
    command: string;
    args: string[];
    env_keys: string[];
    enabled: boolean;
    is_custom: boolean;
    env_configured: boolean;
    assigned_bot_ids: string[];
    tools_count: number;
  }

  let servers = $state<McpServerSummary[]>([]);
  let botServers = $state<Set<string>>(new Set());
  let allBots = $state<any[]>([]);
  let query = $state("");
  let selectedCategory = $state("All");
  let activeTab = $state<"all" | "bot" | "global" | "configured" | "custom">("all");
  let syncing = $state(false);

  // Custom MCP Server Creation State
  let showAddCustom = $state(false);
  let customId = $state("");
  let customName = $state("");
  let customDesc = $state("");
  let customCategory = $state("Development & Coding");
  let customIcon = $state("⚡");
  let customCommand = $state("npx");
  let customArgs = $state("-y @my-org/mcp-server");
  let customEnvKeys = $state("");
  let isSavingCustom = $state(false);
  let customError = $state("");

  // Environment / Key Configuration State
  let showConfigEnv = $state(false);
  let selectedServerForEnv = $state<McpServerSummary | null>(null);
  let envValues = $state<Record<string, string>>({});
  let showSecrets = $state<Record<string, boolean>>({});
  let isSavingEnv = $state(false);
  let envSaveSuccess = $state(false);

  // Test Connection & Tool Inspection State
  let showTestModal = $state(false);
  let selectedServerForTest = $state<McpServerSummary | null>(null);
  let isTesting = $state(false);
  let testResult = $state<{
    success: boolean;
    server_id: string;
    message: string;
    latency_ms: number;
    tools: Array<{ name: string; description: string; input_schema: any }>;
  } | null>(null);

  // Quick feedback copy tooltip
  let copiedId = $state<string | null>(null);

  // Custom server delete confirmation modal state
  let serverToDelete = $state<McpServerSummary | null>(null);
  let isDeletingServer = $state(false);

  const categoryDefs = [
    { id: "All", label: "All", icon: Globe },
    { id: "Development & Coding", label: "Dev & Code", icon: Code },
    { id: "Databases", label: "Databases", icon: Database },
    { id: "Web & Research", label: "Web & Search", icon: Search },
    { id: "Cloud & Infrastructure", label: "Cloud & Infra", icon: Server },
    { id: "Productivity", label: "Productivity", icon: Zap },
    { id: "Design & Creative", label: "Design", icon: Layers },
    { id: "AI / ML", label: "AI & ML", icon: BotIcon },
    { id: "Business / Commerce", label: "Business", icon: Key },
    { id: "Finance & Web3", label: "Finance & Web3", icon: TrendingUp },
    { id: "Social & Messaging", label: "Social & Comms", icon: MessageSquare },
    { id: "Security / Observability", label: "Security", icon: Shield },
    { id: "Smart Home & IoT", label: "Smart Home", icon: Home },
    { id: "Local Computer", label: "Local System", icon: Terminal },
    { id: "Custom", label: "Custom Tools", icon: Sparkles },
  ];

  function getCategoryCount(catId: string): number {
    if (catId === "All") return servers.length;
    if (catId === "Custom") return servers.filter((s) => s.is_custom).length;
    return servers.filter((s) => s.category === catId).length;
  }

  let filtered = $derived(
    servers.filter((s) => {
      const q = query.trim().toLowerCase();
      const matchesQ =
        !q ||
        s.name.toLowerCase().includes(q) ||
        s.id.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        s.command.toLowerCase().includes(q) ||
        s.category.toLowerCase().includes(q);

      if (!matchesQ) return false;

      // Category filter
      if (selectedCategory !== "All") {
        if (selectedCategory === "Custom") {
          if (!s.is_custom) return false;
        } else if (s.category !== selectedCategory) {
          return false;
        }
      }

      // Tab filter
      if (activeTab === "bot" && bot?.id) {
        return botServers.has(s.id);
      } else if (activeTab === "global") {
        return s.enabled;
      } else if (activeTab === "configured") {
        return s.env_keys.length > 0 && !s.env_configured;
      } else if (activeTab === "custom") {
        return s.is_custom;
      }

      return true;
    })
  );

  let activeBotCount = $derived(botServers.size);
  let globalCount = $derived(servers.filter((s) => s.enabled).length);
  let missingKeysCount = $derived(servers.filter((s) => s.env_keys.length > 0 && !s.env_configured).length);
  let customCount = $derived(servers.filter((s) => s.is_custom).length);

  async function load() {
    syncing = true;
    try {
      servers = await invoke("list_mcp_servers", { category: null });
      try {
        allBots = await invoke("list_bots");
      } catch {}

      if (bot?.id) {
        const assigned: string[] = await invoke("list_bot_mcp_servers", { botId: bot.id });
        botServers = new Set(assigned);
      }
    } catch (e) {
      console.error("Failed to load MCP servers:", e);
    } finally {
      syncing = false;
    }
  }

  async function toggleBot(id: string) {
    if (!bot?.id) return;
    const enabled = !botServers.has(id);
    try {
      await invoke("toggle_bot_mcp_server", { botId: bot.id, serverId: id, enabled });
      if (enabled) botServers.add(id);
      else botServers.delete(id);
      botServers = new Set(botServers);
      await load();
    } catch (e) {
      console.error("Failed to toggle bot server:", e);
    }
  }

  async function toggleGlobal(id: string, currentEnabled: boolean) {
    try {
      await invoke("toggle_mcp_server", { serverId: id, enabled: !currentEnabled });
      await load();
    } catch (e) {
      console.error("Failed to toggle global server:", e);
    }
  }

  async function openEnvConfig(server: McpServerSummary) {
    selectedServerForEnv = server;
    envValues = {};
    showSecrets = {};
    envSaveSuccess = false;
    try {
      const stored: Record<string, string> = await invoke("get_mcp_server_env", { serverId: server.id });
      server.env_keys.forEach((k) => {
        envValues[k] = stored[k] || "";
      });
      showConfigEnv = true;
    } catch (e) {
      console.error("Failed to load server env:", e);
      server.env_keys.forEach((k) => {
        envValues[k] = "";
      });
      showConfigEnv = true;
    }
  }

  async function saveEnvConfig() {
    if (!selectedServerForEnv) return;
    isSavingEnv = true;
    try {
      await invoke("save_mcp_server_env", {
        serverId: selectedServerForEnv.id,
        env: envValues,
      });
      envSaveSuccess = true;
      setTimeout(() => {
        envSaveSuccess = false;
        showConfigEnv = false;
        load();
      }, 1000);
    } catch (e) {
      alert("Failed to save credentials: " + String(e));
    } finally {
      isSavingEnv = false;
    }
  }

  async function openTestModal(server: McpServerSummary) {
    selectedServerForTest = server;
    testResult = null;
    showTestModal = true;
    isTesting = true;
    try {
      testResult = await invoke("test_mcp_server", { serverId: server.id });
    } catch (e) {
      testResult = {
        success: false,
        server_id: server.id,
        message: "Failed to connect to MCP server: " + String(e),
        latency_ms: 0,
        tools: [],
      };
    } finally {
      isTesting = false;
    }
  }

  async function saveCustomServer() {
    if (!customId.trim() || !customName.trim()) {
      customError = "Server ID and Name are required";
      return;
    }
    customError = "";
    isSavingCustom = true;

    const parsedArgs = customArgs
      .split(/[\s,]+/)
      .map((s) => s.trim())
      .filter(Boolean);

    const parsedEnvKeys = customEnvKeys
      .split(/[\s,]+/)
      .map((s) => s.trim())
      .filter(Boolean);

    const config = {
      id: customId.trim().toLowerCase().replace(/\s+/g, "-"),
      name: customName.trim(),
      description: customDesc.trim() || "Custom MCP server integration",
      category: customCategory,
      icon: customIcon.trim() || "⚡",
      command: customCommand.trim() || "npx",
      args: parsedArgs,
      env_keys: parsedEnvKeys,
      enabled_by_default: false,
      is_custom: true,
    };

    try {
      await invoke("save_custom_mcp_server", { server: config });
      showAddCustom = false;
      customId = "";
      customName = "";
      customDesc = "";
      customArgs = "-y @my-org/mcp-server";
      customEnvKeys = "";
      await load();
    } catch (e) {
      customError = String(e);
    } finally {
      isSavingCustom = false;
    }
  }

  function deleteServer(server: McpServerSummary) {
    serverToDelete = server;
  }

  async function confirmDeleteServer() {
    if (!serverToDelete) return;
    isDeletingServer = true;
    try {
      await invoke("delete_mcp_server", { serverId: serverToDelete.id });
      serverToDelete = null;
      await load();
    } catch (e) {
      alert("Failed to delete server: " + String(e));
    } finally {
      isDeletingServer = false;
    }
  }

  function copyCommand(server: McpServerSummary) {
    const cmdStr = `${server.command} ${server.args.join(" ")}`;
    navigator.clipboard?.writeText(cmdStr);
    copiedId = server.id;
    setTimeout(() => {
      if (copiedId === server.id) copiedId = null;
    }, 2000);
  }

  $effect(() => {
    if (open) {
      load();
    }
  });
</script>

{#if open}
  <Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
    <Dialog.Content
      class="sm:max-w-5xl md:max-w-5xl lg:max-w-6xl max-w-6xl w-[96vw] max-h-[90vh] flex flex-col bg-[#0c0c14]/98 border border-purple-500/30 shadow-[0_0_60px_rgba(147,51,234,0.25)] backdrop-blur-2xl rounded-3xl p-0 overflow-hidden text-zinc-100"
    >
      <!-- Fixed Header -->
      <div class="px-6 pt-5 pb-3.5 border-b border-white/10 shrink-0 bg-[#0e0e18]/80">
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-center gap-3.5">
            <div
              class="size-11 rounded-2xl bg-gradient-to-br from-purple-900/80 to-indigo-950/80 border border-purple-500/40 flex items-center justify-center text-purple-300 shadow-[0_0_20px_rgba(147,51,234,0.3)] shrink-0"
            >
              <Wrench class="size-5.5" />
            </div>
            <div>
              <div class="flex items-center gap-2.5">
                <Dialog.Title class="text-base font-bold text-white flex items-center gap-2">
                  {#if bot}
                    <span>MCP Native Tools for</span>
                    <span class="text-purple-400 font-extrabold flex items-center gap-1.5">
                      <img
                        src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "bottts")}
                        alt={bot.name}
                        class="size-4.5 rounded-full inline-block border border-purple-400"
                      />
                      {bot.name}
                    </span>
                  {:else}
                    <span>MCP Server Hub — 100+ Native Tools</span>
                  {/if}
                </Dialog.Title>
                <span class="text-[10px] bg-purple-950/80 text-purple-300 border border-purple-500/30 px-2 py-0.5 rounded-full font-mono font-bold">
                  {servers.length} SERVERS
                </span>
              </div>
              <Dialog.Description class="text-xs text-zinc-400 mt-0.5">
                Every Model Context Protocol server connects directly as native tools to all LLMs (Claude, GPT-4o, Ollama, Local).
              </Dialog.Description>
            </div>
          </div>

          <div class="flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              class="h-8.5 gap-1.5 text-xs bg-[#171726] border-purple-500/30 text-purple-300 hover:bg-purple-950/40 hover:text-white cursor-pointer shadow-sm"
              onclick={() => load()}
              disabled={syncing}
            >
              <RefreshCw class={cn("size-3.5", syncing && "animate-spin text-purple-400")} />
              <span>{syncing ? "Syncing..." : "Refresh"}</span>
            </Button>

            <Button
              size="sm"
              class="h-8.5 gap-1.5 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium shadow-[0_0_15px_rgba(168,85,247,0.35)] cursor-pointer"
              onclick={() => (showAddCustom = true)}
            >
              <Plus class="size-3.5" />
              <span>Add Custom MCP</span>
            </Button>
          </div>
        </div>

        <!-- Filter Bar & Search -->
        <div class="flex flex-col lg:flex-row gap-2.5 mt-3.5 items-stretch lg:items-center justify-between">
          <!-- Search input -->
          <div class="relative flex-1 min-w-[240px]">
            <Search class="absolute left-3 top-1/2 -translate-y-1/2 size-3.5 text-zinc-400 pointer-events-none" />
            <Input
              bind:value={query}
              placeholder="Search 100+ MCP servers (github, postgres, fetch, duckdb, telegram, crypto, docker, shell)..."
              class="pl-9 pr-8 h-9 text-xs bg-[#12121e] border-[#222234] focus-visible:border-purple-500/60 focus-visible:ring-purple-500/20 text-zinc-100 placeholder:text-zinc-500 rounded-xl"
            />
            {#if query}
              <button
                type="button"
                class="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white p-1"
                onclick={() => (query = "")}
              >
                <X class="size-3.5" />
              </button>
            {/if}
          </div>

          <!-- Scope Filter Pills -->
          <div class="flex flex-wrap items-center gap-1.5 shrink-0">
            <button
              type="button"
              class={cn(
                "px-2.5 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
                activeTab === "all"
                  ? "bg-purple-600 text-white border-purple-400 shadow-[0_0_12px_rgba(147,51,234,0.35)]"
                  : "bg-[#141422] text-zinc-400 border-[#222234] hover:bg-[#1a1a2e] hover:text-zinc-200"
              )}
              onclick={() => (activeTab = "all")}
            >
              <span>All</span>
              <span class="text-[10px] font-mono opacity-80">({servers.length})</span>
            </button>

            {#if bot}
              <button
                type="button"
                class={cn(
                  "px-2.5 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
                  activeTab === "bot"
                    ? "bg-purple-600 text-white border-purple-400 shadow-[0_0_12px_rgba(147,51,234,0.35)]"
                    : "bg-[#141422] text-zinc-400 border-[#222234] hover:bg-[#1a1a2e] hover:text-zinc-200"
                )}
                onclick={() => (activeTab = "bot")}
              >
                <BotIcon class="size-3 text-purple-300" />
                <span>For {bot.name}</span>
                <span class="text-[10px] font-mono opacity-80">({activeBotCount})</span>
              </button>
            {/if}

            <button
              type="button"
              class={cn(
                "px-2.5 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
                activeTab === "global"
                  ? "bg-cyan-600 text-white border-cyan-400 shadow-[0_0_12px_rgba(6,182,212,0.35)]"
                  : "bg-[#141422] text-zinc-400 border-[#222234] hover:bg-[#1a1a2e] hover:text-zinc-200"
              )}
              onclick={() => (activeTab = "global")}
            >
              <Globe class="size-3 text-cyan-300" />
              <span>Global Active</span>
              <span class="text-[10px] font-mono opacity-80">({globalCount})</span>
            </button>

            {#if missingKeysCount > 0}
              <button
                type="button"
                class={cn(
                  "px-2.5 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
                  activeTab === "configured"
                    ? "bg-amber-600 text-white border-amber-400 shadow-[0_0_12px_rgba(245,158,11,0.35)]"
                    : "bg-[#141422] text-amber-300/90 border-amber-500/30 hover:bg-[#1a1a2e] hover:text-amber-200"
                )}
                onclick={() => (activeTab = "configured")}
              >
                <Key class="size-3 text-amber-400" />
                <span>Needs Keys</span>
                <span class="text-[10px] font-mono opacity-80">({missingKeysCount})</span>
              </button>
            {/if}

            {#if customCount > 0}
              <button
                type="button"
                class={cn(
                  "px-2.5 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
                  activeTab === "custom"
                    ? "bg-indigo-600 text-white border-indigo-400 shadow-[0_0_12px_rgba(99,102,241,0.35)]"
                    : "bg-[#141422] text-zinc-400 border-[#222234] hover:bg-[#1a1a2e] hover:text-zinc-200"
                )}
                onclick={() => (activeTab = "custom")}
              >
                <Sparkles class="size-3 text-indigo-300" />
                <span>Custom</span>
                <span class="text-[10px] font-mono opacity-80">({customCount})</span>
              </button>
            {/if}
          </div>
        </div>

        <!-- Clean Wrapping Categories Strip (No Horizontal Scrolling / No Bar Problem) -->
        <div class="flex flex-wrap items-center gap-1.5 mt-2.5 pt-2 border-t border-white/5">
          {#each categoryDefs as cat}
            {@const count = getCategoryCount(cat.id)}
            {@const IconComponent = cat.icon}
            {#if count > 0 || cat.id === "All"}
              <button
                type="button"
                class={cn(
                  "px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
                  selectedCategory === cat.id
                    ? "bg-purple-900/60 text-purple-200 border-purple-400/80 shadow-[0_0_10px_rgba(168,85,247,0.25)] font-semibold"
                    : "bg-[#11111b] text-zinc-400 border-[#1c1c2b] hover:bg-[#181828] hover:text-zinc-200 hover:border-purple-500/20"
                )}
                onclick={() => (selectedCategory = cat.id)}
              >
                <IconComponent class="size-3.5 text-purple-400 shrink-0" />
                <span>{cat.label}</span>
                <span class={cn(
                  "text-[10px] font-mono px-1 rounded",
                  selectedCategory === cat.id ? "bg-purple-800/80 text-purple-200" : "text-zinc-500"
                )}>
                  {count}
                </span>
              </button>
            {/if}
          {/each}
        </div>
      </div>

      <!-- Main Scroll Area with Server Cards Grid (Native Smooth Scroll) -->
      <div class="flex-1 min-h-0 overflow-y-auto px-6 py-4 overscroll-contain scroll-smooth focus:outline-none">
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-2 xl:grid-cols-3 gap-3.5 pb-8">
          {#each filtered as s (s.id)}
            {@const isBotEnabled = bot?.id ? botServers.has(s.id) : false}
            {@const isGlobalEnabled = s.enabled}
            {@const hasRequiredKeys = s.env_keys.length > 0}
            {@const isKeyConfigured = s.env_configured}

            <div
              class={cn(
                "rounded-2xl border p-4 transition-all flex flex-col justify-between gap-3 relative group backdrop-blur-md",
                isBotEnabled
                  ? "border-purple-500/70 bg-purple-950/20 shadow-[0_0_20px_rgba(168,85,247,0.15)] ring-1 ring-purple-500/30"
                  : isGlobalEnabled
                    ? "border-cyan-500/40 bg-[#0e121e]/80 hover:border-cyan-500/60"
                    : "border-[#1e1e2d] bg-[#0e0e18]/85 hover:border-purple-500/40 hover:bg-[#121220]"
              )}
            >
              <!-- Top Row: Icon, Title, ID, Category & Badges -->
              <div class="space-y-2">
                <div class="flex items-start justify-between gap-2.5">
                  <div class="flex items-start gap-3 min-w-0">
                    <ConnectorIcon id={s.id} name={s.name} size="md" />
                    <div class="min-w-0">
                      <div class="flex items-center gap-1.5 flex-wrap">
                        <h4 class="font-bold text-xs text-white truncate max-w-[140px] sm:max-w-[170px]" title={s.name}>
                          {s.name}
                        </h4>
                        <span class="font-mono text-[9px] text-zinc-400 bg-black/40 border border-white/5 px-1.5 py-0.2 rounded shrink-0">
                          {s.id}
                        </span>
                        {#if s.is_custom}
                          <span class="text-[9px] bg-indigo-950/80 text-indigo-300 border border-indigo-500/40 px-1.5 py-0.2 rounded font-bold">
                            CUSTOM
                          </span>
                        {/if}
                      </div>
                      <div class="flex items-center gap-1 mt-1">
                        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-purple-500/25 text-purple-300 bg-purple-950/30">
                          {s.category}
                        </Badge>
                        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-zinc-700 text-zinc-400 bg-black/30">
                          {s.tools_count} tools
                        </Badge>
                      </div>
                    </div>
                  </div>

                  <!-- Quick Action Menu -->
                  <div class="flex items-center gap-1 shrink-0">
                    <button
                      type="button"
                      class="size-7 rounded-lg bg-[#181826] border border-[#252538] hover:border-purple-500/40 hover:bg-purple-950/30 text-zinc-400 hover:text-purple-300 flex items-center justify-center transition-all cursor-pointer"
                      title="Test Connection & Inspect Tools"
                      onclick={() => openTestModal(s)}
                    >
                      <Zap class="size-3.5" />
                    </button>
                    {#if hasRequiredKeys}
                      <button
                        type="button"
                        class={cn(
                          "size-7 rounded-lg border flex items-center justify-center transition-all cursor-pointer",
                          isKeyConfigured
                            ? "bg-emerald-950/40 border-emerald-500/40 text-emerald-400 hover:bg-emerald-900/50"
                            : "bg-amber-950/40 border-amber-500/40 text-amber-400 hover:bg-amber-900/50 animate-pulse"
                        )}
                        title={isKeyConfigured ? "View / Edit Configured Credentials" : "Missing Required Credentials — Click to Configure"}
                        onclick={() => openEnvConfig(s)}
                      >
                        <Key class="size-3.5" />
                      </button>
                    {/if}
                    {#if s.is_custom}
                      <button
                        type="button"
                        class="size-7 rounded-lg bg-[#181826] border border-[#252538] hover:border-red-500/40 hover:bg-red-950/30 text-zinc-400 hover:text-red-400 flex items-center justify-center transition-all cursor-pointer"
                        title="Delete Custom Server"
                        onclick={() => deleteServer(s)}
                      >
                        <Trash2 class="size-3.5" />
                      </button>
                    {/if}
                  </div>
                </div>

                <!-- Description -->
                <p class="text-xs text-zinc-400 line-clamp-2 leading-relaxed h-8">
                  {s.description}
                </p>

                <!-- Credentials status badge -->
                {#if hasRequiredKeys}
                  <div class="flex items-center gap-1.5 pt-0.5">
                    {#if isKeyConfigured}
                      <button
                        type="button"
                        class="text-[10px] text-emerald-400 bg-emerald-950/50 border border-emerald-500/30 px-2 py-0.5 rounded-md flex items-center gap-1 hover:bg-emerald-900/40 cursor-pointer font-medium"
                        onclick={() => openEnvConfig(s)}
                      >
                        <Check class="size-3" />
                        <span>Credentials Configured</span>
                      </button>
                    {:else}
                      <button
                        type="button"
                        class="text-[10px] text-amber-300 bg-amber-950/60 border border-amber-500/40 px-2 py-0.5 rounded-md flex items-center gap-1 hover:bg-amber-900/50 cursor-pointer font-semibold animate-pulse"
                        onclick={() => openEnvConfig(s)}
                      >
                        <Key class="size-3 text-amber-400" />
                        <span>Needs {s.env_keys.slice(0, 2).join(", ")}</span>
                      </button>
                    {/if}
                  </div>
                {/if}

                <!-- Command Snippet with Copy -->
                <div class="flex items-center justify-between gap-1 bg-[#090910] border border-[#1c1c2b] rounded-lg px-2 py-1 text-[10px] font-mono text-zinc-400">
                  <span class="truncate" title={`${s.command} ${s.args.join(" ")}`}>
                    <span class="text-purple-400">{s.command}</span> {s.args.slice(0, 3).join(" ")}
                  </span>
                  <button
                    type="button"
                    class="shrink-0 text-zinc-500 hover:text-white transition-colors"
                    title="Copy command"
                    onclick={() => copyCommand(s)}
                  >
                    {#if copiedId === s.id}
                      <Check class="size-3 text-emerald-400" />
                    {:else}
                      <Copy class="size-3" />
                    {/if}
                  </button>
                </div>
              </div>

              <!-- Footer Actions Row -->
              <div class="pt-2.5 border-t border-[#1e1e2d] flex items-center justify-between gap-2 mt-1">
                {#if bot}
                  <Button
                    size="sm"
                    variant={isBotEnabled ? "default" : "outline"}
                    class={cn(
                      "h-7.5 text-xs px-3 font-medium flex-1 cursor-pointer transition-all",
                      isBotEnabled
                        ? "bg-purple-600 hover:bg-purple-500 text-white shadow-[0_0_12px_rgba(168,85,247,0.3)]"
                        : "bg-[#151522] border-[#29293e] text-zinc-300 hover:bg-purple-950/30 hover:border-purple-500/40 hover:text-white"
                    )}
                    onclick={() => toggleBot(s.id)}
                  >
                    {#if isBotEnabled}
                      <Check class="size-3 mr-1" />
                      <span>Active for {bot.name}</span>
                    {:else}
                      <Plus class="size-3 mr-1" />
                      <span>Enable for {bot.name}</span>
                    {/if}
                  </Button>
                {/if}

                <!-- Global Toggle Button -->
                <Button
                  size="sm"
                  variant="ghost"
                  class={cn(
                    "h-7.5 text-xs px-2.5 cursor-pointer font-normal",
                    isGlobalEnabled
                      ? "text-cyan-400 hover:bg-cyan-950/30 hover:text-cyan-300"
                      : "text-zinc-500 hover:text-zinc-300 hover:bg-[#171724]"
                  )}
                  onclick={() => toggleGlobal(s.id, isGlobalEnabled)}
                  title={isGlobalEnabled ? "Globally enabled for all agents" : "Click to enable globally for all agents"}
                >
                  <Globe class="size-3 mr-1" />
                  <span>{isGlobalEnabled ? "Global" : "Off"}</span>
                </Button>
              </div>
            </div>
          {:else}
            <div class="col-span-full py-16 text-center border-2 border-dashed border-[#222234] rounded-3xl bg-[#0e0e18]/40 space-y-3">
              <div class="size-12 rounded-2xl bg-purple-950/40 border border-purple-800/30 flex items-center justify-center text-purple-400 mx-auto">
                <Wrench class="size-6" />
              </div>
              <div class="space-y-1">
                <p class="text-sm font-bold text-white">No MCP servers match your filter</p>
                <p class="text-xs text-zinc-400 max-w-sm mx-auto">
                  Try clearing your search query or add a custom MCP server to connect any custom tool.
                </p>
              </div>
              <div class="flex justify-center gap-2 pt-2">
                {#if query || selectedCategory !== "All" || activeTab !== "all"}
                  <Button
                    size="sm"
                    variant="outline"
                    class="h-8 text-xs bg-[#161626] border-[#252538] text-zinc-300"
                    onclick={() => {
                      query = "";
                      selectedCategory = "All";
                      activeTab = "all";
                    }}
                  >
                    Clear Filters
                  </Button>
                {/if}
                <Button
                  size="sm"
                  class="h-8 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium gap-1.5"
                  onclick={() => (showAddCustom = true)}
                >
                  <Plus class="size-3.5" />
                  <span>Add Custom Server</span>
                </Button>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <!-- Fixed Footer Summary -->
      <div class="px-6 py-3 border-t border-white/10 flex flex-col sm:flex-row justify-between items-center bg-[#0e0e18]/90 shrink-0 gap-2">
        <div class="flex items-center gap-3 text-xs text-zinc-400">
          <span class="flex items-center gap-1.5 font-medium text-white">
            <Sparkles class="size-3.5 text-purple-400" />
            <span>{filtered.length} of {servers.length} servers available</span>
          </span>
          {#if bot}
            <span class="text-zinc-600">•</span>
            <span class="text-purple-300">
              <span class="font-bold text-white">{botServers.size}</span> active for {bot.name}
            </span>
          {/if}
          <span class="text-zinc-600">•</span>
          <span class="text-cyan-300">
            <span class="font-bold text-white">{globalCount}</span> globally enabled
          </span>
        </div>

        <Button size="sm" class="bg-purple-600 hover:bg-purple-500 text-white px-5 h-8.5 font-medium cursor-pointer" onclick={onClose}>
          Done
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>

  <!-- SUB-MODAL 1: Add Custom MCP Server -->
  {#if showAddCustom}
    <Dialog.Root open={showAddCustom} onOpenChange={(o) => !o && (showAddCustom = false)}>
      <Dialog.Content class="sm:max-w-xl max-h-[88vh] overflow-y-auto bg-[#0c0c14]/98 border border-purple-500/30 shadow-[0_0_50px_rgba(147,51,234,0.3)] backdrop-blur-2xl rounded-3xl p-6 text-zinc-100">
        <Dialog.Header class="pb-3 border-b border-white/10">
          <div class="flex items-center gap-3">
            <div class="size-10 rounded-2xl bg-purple-950/70 border border-purple-800/50 flex items-center justify-center text-purple-400">
              <Plus class="size-5" />
            </div>
            <div>
              <Dialog.Title class="text-base font-bold text-white">
                Add Custom MCP Server
              </Dialog.Title>
              <Dialog.Description class="text-xs text-zinc-400">
                Integrate any stdio CLI or SSE remote endpoint into RAVENBOT as native model skills.
              </Dialog.Description>
            </div>
          </div>
        </Dialog.Header>

        {#if customError}
          <div class="p-3 rounded-xl bg-red-950/40 border border-red-500/40 text-xs text-red-300 flex items-center gap-2 mt-3">
            <AlertCircle class="size-4 shrink-0 text-red-400" />
            <span>{customError}</span>
          </div>
        {/if}

        <div class="space-y-3.5 py-3">
          <div class="grid grid-cols-3 gap-2.5">
            <div class="col-span-2 space-y-1.5">
              <Label class="text-xs font-bold text-white">Server Name *</Label>
              <Input
                bind:value={customName}
                placeholder="e.g. SQLite DB Explorer"
                class="h-9 text-xs bg-[#141420] border-[#252538]"
              />
            </div>
            <div class="space-y-1.5">
              <Label class="text-xs font-bold text-white">Icon / Emoji</Label>
              <Input
                bind:value={customIcon}
                placeholder="e.g. ⚡"
                class="h-9 text-xs bg-[#141420] border-[#252538] text-center"
              />
            </div>
          </div>

          <div class="grid grid-cols-2 gap-2.5">
            <div class="space-y-1.5">
              <Label class="text-xs font-bold text-white">Unique Identifier (ID) *</Label>
              <Input
                bind:value={customId}
                placeholder="e.g. sqlite-explorer"
                class="h-9 text-xs font-mono bg-[#141420] border-[#252538]"
              />
            </div>
            <div class="space-y-1.5">
              <Label class="text-xs font-bold text-white">Category</Label>
              <select
                bind:value={customCategory}
                class="w-full h-9 rounded-xl text-xs bg-[#141424] border border-[#28283e] text-zinc-200 px-3 pr-8 focus:outline-none focus:border-purple-500 cursor-pointer"
              >
                {#each categoryDefs.filter((c) => c.id !== "All") as c}
                  <option value={c.id} class="bg-[#0e0e1a] text-zinc-100">{c.icon} {c.id}</option>
                {/each}
              </select>
            </div>
          </div>

          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-white">Description</Label>
            <Textarea
              bind:value={customDesc}
              placeholder="Describe what capabilities and tools this MCP server provides to your agents..."
              class="text-xs bg-[#141420] border-[#252538] min-h-[60px]"
            />
          </div>

          <div class="grid grid-cols-3 gap-2.5">
            <div class="space-y-1.5">
              <Label class="text-xs font-bold text-white">Command (Executable)</Label>
              <Input
                bind:value={customCommand}
                placeholder="npx, uvx, python, node"
                class="h-9 text-xs font-mono bg-[#141420] border-[#252538]"
              />
            </div>
            <div class="col-span-2 space-y-1.5">
              <Label class="text-xs font-bold text-white">Arguments (Space-separated)</Label>
              <Input
                bind:value={customArgs}
                placeholder="-y @modelcontextprotocol/server-sqlite /path/db.sqlite"
                class="h-9 text-xs font-mono bg-[#141420] border-[#252538]"
              />
            </div>
          </div>

          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-white">
              Required Environment Keys (Optional, Comma-separated)
            </Label>
            <Input
              bind:value={customEnvKeys}
              placeholder="e.g. API_KEY, DATABASE_URL, AUTH_TOKEN"
              class="h-9 text-xs font-mono bg-[#141420] border-[#252538]"
            />
            <p class="text-[10px] text-zinc-500">
              Keys will be securely requested and stored in hardware keychain for authentication.
            </p>
          </div>
        </div>

        <div class="flex justify-end gap-2 pt-3 border-t border-white/10">
          <Button
            variant="outline"
            size="sm"
            class="bg-[#141420] border-[#252538] text-zinc-300"
            onclick={() => (showAddCustom = false)}
            disabled={isSavingCustom}
          >
            Cancel
          </Button>
          <Button
            size="sm"
            class="bg-purple-600 hover:bg-purple-500 text-white font-medium"
            onclick={saveCustomServer}
            disabled={isSavingCustom}
          >
            {isSavingCustom ? "Registering..." : "Register MCP Server"}
          </Button>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}

  <!-- SUB-MODAL 2: Configure Environment Keys -->
  {#if showConfigEnv && selectedServerForEnv}
    <Dialog.Root open={showConfigEnv} onOpenChange={(o) => !o && (showConfigEnv = false)}>
      <Dialog.Content class="sm:max-w-md bg-[#0c0c14]/98 border border-purple-500/30 shadow-[0_0_50px_rgba(147,51,234,0.3)] backdrop-blur-2xl rounded-3xl p-5 text-zinc-100">
        <Dialog.Header class="pb-3 border-b border-white/10">
          <div class="flex items-center gap-3">
            <div class="size-9 rounded-xl bg-purple-950/70 border border-purple-800/50 flex items-center justify-center text-purple-400">
              <Key class="size-4.5" />
            </div>
            <div>
              <Dialog.Title class="text-sm font-bold text-white">
                Credentials: {selectedServerForEnv.name}
              </Dialog.Title>
              <Dialog.Description class="text-xs text-zinc-400">
                Stored in sovereign local database. Injected securely into MCP runtime.
              </Dialog.Description>
            </div>
          </div>
        </Dialog.Header>

        {#if envSaveSuccess}
          <div class="p-3 rounded-xl bg-emerald-950/50 border border-emerald-500/40 text-xs text-emerald-300 flex items-center gap-2 my-2">
            <CheckCircle2 class="size-4 shrink-0 text-emerald-400" />
            <span>Credentials saved securely!</span>
          </div>
        {/if}

        <div class="space-y-3 py-3">
          {#each selectedServerForEnv.env_keys as key}
            <div class="space-y-1.5">
              <Label class="text-xs font-mono font-bold text-zinc-300 flex items-center justify-between">
                <span>{key}</span>
                <span class="text-[10px] text-zinc-500 font-sans">Required</span>
              </Label>
              <div class="relative">
                <Input
                  type={showSecrets[key] ? "text" : "password"}
                  bind:value={envValues[key]}
                  placeholder={`Enter value for ${key}...`}
                  class="h-9 text-xs font-mono pr-8 bg-[#141420] border-[#252538] text-zinc-200"
                />
                <button
                  type="button"
                  class="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
                  onclick={() => (showSecrets[key] = !showSecrets[key])}
                >
                  {#if showSecrets[key]}
                    <EyeOff class="size-3.5" />
                  {:else}
                    <Eye class="size-3.5" />
                  {/if}
                </button>
              </div>
            </div>
          {/each}
        </div>

        <div class="flex justify-between items-center pt-3 border-t border-white/10">
          <Button
            size="sm"
            variant="ghost"
            class="text-xs text-purple-300 hover:text-white"
            onclick={() => {
              showConfigEnv = false;
              openTestModal(selectedServerForEnv!);
            }}
          >
            <Zap class="size-3 mr-1 text-purple-400" />
            <span>Test Connection</span>
          </Button>

          <div class="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              class="bg-[#141420] border-[#252538] text-zinc-300"
              onclick={() => (showConfigEnv = false)}
            >
              Cancel
            </Button>
            <Button
              size="sm"
              class="bg-purple-600 hover:bg-purple-500 text-white font-medium"
              onclick={saveEnvConfig}
              disabled={isSavingEnv}
            >
              {isSavingEnv ? "Saving..." : "Save Credentials"}
            </Button>
          </div>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}

  <!-- SUB-MODAL 3: Test Connection & Tool Inspector -->
  {#if showTestModal && selectedServerForTest}
    <Dialog.Root open={showTestModal} onOpenChange={(o) => !o && (showTestModal = false)}>
      <Dialog.Content class="sm:max-w-lg max-h-[85vh] flex flex-col bg-[#0c0c14]/98 border border-purple-500/30 shadow-[0_0_50px_rgba(147,51,234,0.3)] backdrop-blur-2xl rounded-3xl p-5 text-zinc-100">
        <Dialog.Header class="pb-3 border-b border-white/10 shrink-0">
          <div class="flex items-center gap-3">
            <div class="size-9 rounded-xl bg-purple-950/70 border border-purple-800/50 flex items-center justify-center text-purple-400">
              <Zap class="size-4.5" />
            </div>
            <div>
              <Dialog.Title class="text-sm font-bold text-white flex items-center gap-2">
                <span>MCP Diagnostic: {selectedServerForTest.name}</span>
              </Dialog.Title>
              <Dialog.Description class="text-xs text-zinc-400">
                Testing tool discovery and protocol handshake.
              </Dialog.Description>
            </div>
          </div>
        </Dialog.Header>

        <div class="flex-1 overflow-y-auto py-3 space-y-3">
          {#if isTesting}
            <div class="py-8 text-center space-y-2">
              <RefreshCw class="size-6 text-purple-400 animate-spin mx-auto" />
              <p class="text-xs text-zinc-400">Connecting to MCP stdio/remote server...</p>
            </div>
          {:else if testResult}
            <div
              class={cn(
                "p-3 rounded-xl border flex items-start gap-2.5 text-xs",
                testResult.success
                  ? "bg-emerald-950/40 border-emerald-500/40 text-emerald-300"
                  : "bg-red-950/40 border-red-500/40 text-red-300"
              )}
            >
              {#if testResult.success}
                <CheckCircle2 class="size-4.5 text-emerald-400 shrink-0 mt-0.5" />
              {:else}
                <AlertCircle class="size-4.5 text-red-400 shrink-0 mt-0.5" />
              {/if}
              <div class="space-y-0.5 flex-1">
                <div class="font-bold flex items-center justify-between">
                  <span>{testResult.success ? "Connection Verified" : "Connection Failed"}</span>
                  <span class="font-mono text-[10px] text-zinc-400">{testResult.latency_ms}ms latency</span>
                </div>
                <p class="leading-relaxed opacity-90">{testResult.message}</p>
              </div>
            </div>

            <!-- Discovered Tools List -->
            <div class="space-y-2 pt-1">
              <div class="flex items-center justify-between text-xs font-bold text-zinc-300">
                <span class="flex items-center gap-1.5">
                  <Wrench class="size-3.5 text-purple-400" />
                  <span>Discovered Native Tools ({testResult.tools.length})</span>
                </span>
                <span class="text-[10px] font-mono text-zinc-500">Autonomous Execution Ready</span>
              </div>

              <div class="space-y-2 max-h-56 overflow-y-auto pr-1">
                {#each testResult.tools as tool}
                  <div class="p-2.5 rounded-xl border border-[#1e1e2d] bg-[#12121e] space-y-1">
                    <div class="flex items-center justify-between">
                      <span class="font-mono text-xs font-bold text-purple-300">{tool.name}</span>
                      <span class="text-[9px] bg-purple-950/60 border border-purple-500/20 text-purple-300 px-1.5 py-0.2 rounded font-mono">
                        NATIVE
                      </span>
                    </div>
                    <p class="text-[11px] text-zinc-400 leading-snug">{tool.description}</p>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <div class="flex justify-end gap-2 pt-3 border-t border-white/10 shrink-0">
          <Button
            size="sm"
            class="bg-purple-600 hover:bg-purple-500 text-white font-medium"
            onclick={() => (showTestModal = false)}
          >
            Close Inspector
          </Button>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}

  <!-- Custom Server Delete Confirmation Modal -->
  {#if serverToDelete}
    <Dialog.Root open={Boolean(serverToDelete)} onOpenChange={(o) => !o && (serverToDelete = null)}>
      <Dialog.Content class="sm:max-w-md max-w-md bg-[#0e0e1a] border border-red-500/40 rounded-3xl p-6 text-zinc-100 shadow-[0_0_60px_rgba(239,68,68,0.3)] flex flex-col gap-4 font-sans select-none z-50">
        <div class="flex items-start gap-3.5">
          <div class="size-11 rounded-2xl bg-red-950/70 border border-red-500/50 flex items-center justify-center text-red-400 shrink-0 shadow-[0_0_20px_rgba(239,68,68,0.35)]">
            <AlertCircle class="size-6" />
          </div>
          <div class="space-y-1">
            <Dialog.Title class="text-base font-extrabold text-white">
              Delete Custom Server?
            </Dialog.Title>
            <Dialog.Description class="text-xs text-zinc-400 leading-relaxed">
              Are you sure you want to delete custom server <span class="text-white font-bold font-mono">"{serverToDelete.name}"</span>?
            </Dialog.Description>
          </div>
        </div>

        <div class="flex items-center justify-end gap-2.5 pt-2 border-t border-white/5">
          <Button
            variant="outline"
            class="h-8.5 text-xs bg-[#141424] border-[#29293e] text-zinc-300 hover:bg-[#1a1a2e] hover:text-white cursor-pointer"
            onclick={() => (serverToDelete = null)}
            disabled={isDeletingServer}
          >
            Cancel
          </Button>

          <Button
            class="h-8.5 text-xs bg-red-600 hover:bg-red-500 text-white font-medium gap-1.5 shadow-[0_0_25px_rgba(239,68,68,0.4)] cursor-pointer"
            disabled={isDeletingServer}
            onclick={confirmDeleteServer}
          >
            <Trash2 class="size-3.5" />
            <span>{isDeletingServer ? "Deleting..." : "Delete Server"}</span>
          </Button>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}
{/if}
