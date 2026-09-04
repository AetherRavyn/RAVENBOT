<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { cn } from "$lib/utils.js";
  import { getDiceBearUrl } from "$lib/utils";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Badge } from "$lib/components/ui/badge";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Label } from "$lib/components/ui/label";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import * as Dialog from "$lib/components/ui/dialog";
  import ConnectorIcon from "$lib/components/ConnectorIcon.svelte";
  import {
    Wrench,
    Search,
    RefreshCw,
    Plus,
    Check,
    Key,
    Globe,
    Bot as BotIcon,
    Trash2,
    AlertCircle,
    Copy,
    Zap,
    Eye,
    EyeOff,
    Sparkles,
    Shield,
    Terminal,
    Layers,
    Server,
    ExternalLink,
    Filter,
    CheckSquare,
    Square,
    SlidersHorizontal,
    Code,
    Cpu,
    Database,
    ArrowRight,
    TrendingUp,
    MessageSquare,
    Lock,
    Home,
    FolderKanban,
    Compass,
    X,
  } from "@lucide/svelte";

  interface Props {
    bots: any[];
    selectedBotId?: string | null;
    onSelectBot?: (id: string) => void;
    onBotsUpdated?: (bots: any[]) => void;
  }

  let { bots = [], selectedBotId = null, onSelectBot, onBotsUpdated }: Props = $props();

  export interface McpServerSummary {
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

  // Active view state
  let currentBotId = $state<string | null>(null);
  let servers = $state<McpServerSummary[]>([]);
  let botServersMap = $state<Record<string, Set<string>>>({});
  let query = $state("");
  let selectedCategory = $state("All");
  let activeTab = $state<"all" | "active" | "global" | "configured" | "custom">("all");
  let syncing = $state(false);

  // Multi-Selection State for Batch Operations
  let selectedConnectorIds = $state<Set<string>>(new Set());

  // Assign Multi-Agents Modal State
  let showAssignAgentsModal = $state(false);
  let selectedConnectorForAgentAssignment = $state<McpServerSummary | null>(null);
  let assignedAgentIds = $state<Set<string>>(new Set());
  let isSavingAgentAssignment = $state(false);

  // Custom MCP Server Modal State
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

  // Environment / Key Configuration Drawer State
  let showConfigEnv = $state(false);
  let selectedServerForEnv = $state<McpServerSummary | null>(null);
  let envValues = $state<Record<string, string>>({});
  let showSecrets = $state<Record<string, boolean>>({});
  let isSavingEnv = $state(false);
  let envSaveSuccess = $state(false);

  // Connection Test & Tool Inspection State
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

  // Preset Stacks Drawer State
  let showPresetModal = $state(false);
  let applyingPreset = $state(false);

  // Copy command toast indicator
  let copiedId = $state<string | null>(null);

  // Custom connector delete confirmation
  let serverToDelete = $state<McpServerSummary | null>(null);
  let isDeletingServer = $state(false);

  // Initialize and react to selected bot
  $effect(() => {
    if (selectedBotId && selectedBotId !== currentBotId) {
      currentBotId = selectedBotId;
    } else if (!currentBotId && bots.length > 0) {
      currentBotId = bots[0].id;
    }
  });

  const categoryDefs = [
    { id: "All", label: "All Connectors", icon: Globe },
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

  const PRESET_STACKS = [
    {
      id: "fullstack",
      name: "Full-Stack Engineer",
      iconComponent: Terminal,
      description: "Code search, GitHub PRs, Postgres, Redis, Docker, and terminal shell execution.",
      connectors: ["github", "postgres", "redis", "docker", "sentry", "shell", "filesystem", "postman"],
    },
    {
      id: "data_ai",
      name: "Data & AI Researcher",
      iconComponent: Database,
      description: "Ultra-fast DuckDB, academic papers, Wolfram Alpha, neural search, and vector databases.",
      connectors: ["duckdb", "arxiv", "wikipedia", "wolfram_alpha", "brave_search", "openai", "pinecone", "exa"],
    },
    {
      id: "devops",
      name: "DevOps & Cloud Architect",
      iconComponent: Server,
      description: "AWS cloud, Kubernetes pods, GitHub Actions, Datadog observability, and Terraform.",
      connectors: ["aws", "kubernetes", "github_actions", "terraform", "datadog", "grafana", "cloudflare", "argocd"],
    },
    {
      id: "finance_crypto",
      name: "Finance & Web3 Analyst",
      iconComponent: TrendingUp,
      description: "Stock quotes, crypto market caps, Ethereum RPC, smart contracts, and spreadsheet rows.",
      connectors: ["yfinance", "alpha_vantage", "coingecko", "etherscan", "alchemy", "gsheets", "duckdb"],
    },
    {
      id: "growth_marketing",
      name: "Social & Growth Hacker",
      iconComponent: MessageSquare,
      description: "Telegram channels, Twitter sentiment, Reddit research, transactional emails, and CRM.",
      connectors: ["telegram", "twitter", "reddit", "resend", "hubspot", "stripe", "notion", "slack"],
    },
    {
      id: "security",
      name: "Security & Threat Recon",
      iconComponent: Shield,
      description: "SAST static analysis, vulnerability scanning, password vaults, and network recon.",
      connectors: ["semgrep", "snyk", "vault", "onepassword", "tailscale", "shodan", "virustotal"],
    },
  ];

  let currentBot = $derived(bots.find((b) => b.id === currentBotId));
  let currentBotServers = $derived(currentBotId && botServersMap[currentBotId] ? botServersMap[currentBotId] : new Set<string>());

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
      if (activeTab === "active" && currentBotId) {
        return currentBotServers.has(s.id);
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

  let activeBotCount = $derived(currentBotServers.size);
  let globalCount = $derived(servers.filter((s) => s.enabled).length);
  let missingKeysCount = $derived(servers.filter((s) => s.env_keys.length > 0 && !s.env_configured).length);
  let customCount = $derived(servers.filter((s) => s.is_custom).length);

  function getCategoryCount(catId: string): number {
    if (catId === "All") return servers.length;
    if (catId === "Custom") return servers.filter((s) => s.is_custom).length;
    return servers.filter((s) => s.category === catId).length;
  }

  async function load() {
    syncing = true;
    try {
      servers = await invoke("list_mcp_servers", { category: null });
      
      // Load assignments for all bots
      const map: Record<string, Set<string>> = {};
      for (const b of bots) {
        try {
          const assigned: string[] = await invoke("list_bot_mcp_servers", { botId: b.id });
          map[b.id] = new Set(assigned);
        } catch {
          map[b.id] = new Set();
        }
      }
      botServersMap = map;
    } catch (e) {
      console.error("Failed to load connectors:", e);
    } finally {
      syncing = false;
    }
  }

  async function toggleForCurrentBot(serverId: string) {
    if (!currentBotId) return;
    const isAssigned = currentBotServers.has(serverId);
    const nextState = !isAssigned;

    try {
      await invoke("toggle_bot_mcp_server", {
        botId: currentBotId,
        serverId,
        enabled: nextState,
      });

      const updatedSet = new Set(currentBotServers);
      if (nextState) {
        updatedSet.add(serverId);
      } else {
        updatedSet.delete(serverId);
      }
      botServersMap = {
        ...botServersMap,
        [currentBotId]: updatedSet,
      };
    } catch (e) {
      alert("Failed to toggle connector for agent: " + String(e));
    }
  }

  async function toggleGlobal(serverId: string, currentEnabled: boolean) {
    try {
      await invoke("toggle_mcp_server", { serverId, enabled: !currentEnabled });
      servers = servers.map((s) => (s.id === serverId ? { ...s, enabled: !currentEnabled } : s));
    } catch (e) {
      alert("Failed to toggle global connector: " + String(e));
    }
  }

  // Multi-Selection Logic
  function toggleSelectConnector(id: string) {
    const next = new Set(selectedConnectorIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selectedConnectorIds = next;
  }

  function selectAllVisible() {
    const next = new Set(selectedConnectorIds);
    for (const s of filtered) {
      next.add(s.id);
    }
    selectedConnectorIds = next;
  }

  function clearSelection() {
    selectedConnectorIds = new Set();
  }

  async function batchAssignSelectedToCurrentBot(enable: boolean) {
    if (!currentBotId || selectedConnectorIds.size === 0) return;
    try {
      const currentList = Array.from(currentBotServers);
      let nextList: string[];
      if (enable) {
        const set = new Set([...currentList, ...Array.from(selectedConnectorIds)]);
        nextList = Array.from(set);
      } else {
        nextList = currentList.filter((id) => !selectedConnectorIds.has(id));
      }

      await invoke("batch_set_bot_mcp", {
        botId: currentBotId,
        serverIds: nextList,
      });

      botServersMap = {
        ...botServersMap,
        [currentBotId]: new Set(nextList),
      };
      selectedConnectorIds = new Set();
    } catch (e) {
      alert("Failed to batch update agent connectors: " + String(e));
    }
  }

  async function batchEnableSelectedGlobally() {
    if (selectedConnectorIds.size === 0) return;
    try {
      for (const id of selectedConnectorIds) {
        await invoke("toggle_mcp_server", { serverId: id, enabled: true });
      }
      await load();
      selectedConnectorIds = new Set();
    } catch (e) {
      alert("Failed to batch enable global connectors: " + String(e));
    }
  }

  // Preset Stack Application
  async function applyPresetStack(preset: (typeof PRESET_STACKS)[0]) {
    if (!currentBotId) return;
    applyingPreset = true;
    try {
      const currentList = Array.from(currentBotServers);
      const combined = Array.from(new Set([...currentList, ...preset.connectors]));
      
      await invoke("batch_set_bot_mcp", {
        botId: currentBotId,
        serverIds: combined,
      });

      botServersMap = {
        ...botServersMap,
        [currentBotId]: new Set(combined),
      };
      showPresetModal = false;
    } catch (e) {
      alert("Failed to apply preset stack: " + String(e));
    } finally {
      applyingPreset = false;
    }
  }

  // Open Multi-Agent Assignment Modal for a specific connector
  function openAssignAgentsModal(server: McpServerSummary) {
    selectedConnectorForAgentAssignment = server;
    const assigned = new Set<string>();
    for (const b of bots) {
      if (botServersMap[b.id]?.has(server.id)) {
        assigned.add(b.id);
      }
    }
    assignedAgentIds = assigned;
    showAssignAgentsModal = true;
  }

  async function saveAgentAssignment() {
    if (!selectedConnectorForAgentAssignment) return;
    isSavingAgentAssignment = true;
    try {
      const botIdArray = Array.from(assignedAgentIds);
      await invoke("batch_assign_bot_mcp", {
        serverId: selectedConnectorForAgentAssignment.id,
        botIds: botIdArray,
      });

      const updatedMap = { ...botServersMap };
      for (const b of bots) {
        const set = new Set(updatedMap[b.id] || []);
        if (assignedAgentIds.has(b.id)) {
          set.add(selectedConnectorForAgentAssignment.id);
        } else {
          set.delete(selectedConnectorForAgentAssignment.id);
        }
        updatedMap[b.id] = set;
      }
      botServersMap = updatedMap;
      showAssignAgentsModal = false;
    } catch (e) {
      alert("Failed to save agent assignments: " + String(e));
    } finally {
      isSavingAgentAssignment = false;
    }
  }

  // Credentials drawer
  async function openEnvConfig(server: McpServerSummary) {
    selectedServerForEnv = server;
    envValues = {};
    showSecrets = {};
    envSaveSuccess = false;
    try {
      const stored: Record<string, string> = await invoke("get_mcp_server_env", { serverId: server.id });
      for (const k of server.env_keys) {
        envValues[k] = stored[k] || "";
      }
    } catch {
      for (const k of server.env_keys) {
        envValues[k] = "";
      }
    }
    showConfigEnv = true;
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
      await load();
      setTimeout(() => {
        showConfigEnv = false;
        envSaveSuccess = false;
      }, 1000);
    } catch (e) {
      alert("Failed to save credentials: " + String(e));
    } finally {
      isSavingEnv = false;
    }
  }

  // Testing modal
  async function openTestModal(server: McpServerSummary) {
    selectedServerForTest = server;
    testResult = null;
    showTestModal = true;
    isTesting = true;
    try {
      const res: any = await invoke("test_mcp_server", { serverId: server.id });
      testResult = res;
    } catch (e) {
      testResult = {
        success: false,
        server_id: server.id,
        message: String(e),
        latency_ms: 0,
        tools: [],
      };
    } finally {
      isTesting = false;
    }
  }

  // Custom Server Save
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
      description: customDesc.trim() || "Custom MCP server connector",
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
      alert("Failed to delete connector: " + String(e));
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
    load();
  });
</script>

<div class="flex flex-col h-full w-full min-h-0 bg-[#08080f] text-zinc-100 overflow-hidden select-none font-sans">
  <!-- Top Command Center Header -->
  <div class="px-6 py-4 border-b border-white/10 shrink-0 bg-[#0c0c16]/90 backdrop-blur-xl">
    <div class="flex flex-col md:flex-row md:items-center justify-between gap-4">
      <!-- Title & Branding -->
      <div class="flex items-center gap-3.5">
        <div
          class="size-11 rounded-2xl bg-gradient-to-br from-purple-600/30 to-indigo-700/30 border border-purple-500/40 flex items-center justify-center text-purple-300 shadow-[0_0_25px_rgba(168,85,247,0.25)] shrink-0"
        >
          <Layers class="size-6" />
        </div>
        <div>
          <div class="flex items-center gap-2.5">
            <h2 class="text-base font-extrabold text-white tracking-wide flex items-center gap-2">
              <span>Connectors & Tools Command Center</span>
            </h2>
            <span class="text-[10px] bg-purple-950/80 text-purple-300 border border-purple-500/30 px-2.5 py-0.5 rounded-full font-mono font-bold shadow-sm">
              {servers.length} CONNECTORS
            </span>
          </div>
          <p class="text-xs text-zinc-400 mt-0.5">
            Official vector-connected MCP tools (135+), REST APIs, specialized databases, anti-bot scrapers & local OS bridges.
          </p>
        </div>
      </div>

      <!-- Header Action Controls -->
      <div class="flex items-center gap-2.5 flex-wrap">
        <Button
          size="sm"
          variant="outline"
          class="h-8.5 gap-1.5 text-xs bg-[#141422] border-purple-500/30 text-purple-300 hover:bg-purple-950/40 hover:text-white cursor-pointer shadow-sm"
          onclick={() => (showPresetModal = true)}
        >
          <Sparkles class="size-3.5 text-purple-400" />
          <span>Preset Stacks</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          class="h-8.5 gap-1.5 text-xs bg-[#141422] border-[#222234] text-zinc-300 hover:bg-[#1c1c2e] hover:text-white cursor-pointer shadow-sm"
          onclick={() => load()}
          disabled={syncing}
        >
          <RefreshCw class={cn("size-3.5", syncing && "animate-spin text-purple-400")} />
          <span>{syncing ? "Syncing..." : "Refresh"}</span>
        </Button>

        <Button
          size="sm"
          class="h-8.5 gap-1.5 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium shadow-[0_0_20px_rgba(168,85,247,0.35)] cursor-pointer"
          onclick={() => (showAddCustom = true)}
        >
          <Plus class="size-3.5" />
          <span>+ Add Custom Connector</span>
        </Button>
      </div>
    </div>

    <!-- Agent Quick Selector Strip -->
    {#if bots.length > 0}
      <div class="mt-4 pt-3.5 border-t border-white/5 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div class="flex items-center gap-2 overflow-x-auto pb-1 scrollbar-none">
          <span class="text-xs font-semibold text-zinc-400 uppercase font-mono shrink-0 mr-1 flex items-center gap-1.5">
            <BotIcon class="size-3.5 text-purple-400" />
            <span>Select Agent:</span>
          </span>
          {#each bots as b (b.id)}
            {@const isSelected = currentBotId === b.id}
            {@const botCount = botServersMap[b.id]?.size || 0}
            <button
              type="button"
              class={cn(
                "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-2 cursor-pointer shrink-0 border",
                isSelected
                  ? "bg-purple-600 text-white border-purple-400 shadow-[0_0_15px_rgba(168,85,247,0.35)] font-bold scale-[1.02]"
                  : "bg-[#11111d] text-zinc-300 border-[#202032] hover:bg-[#18182b] hover:text-white"
              )}
              onclick={() => {
                currentBotId = b.id;
                onSelectBot?.(b.id);
              }}
            >
              <img
                src={b.avatar_url || getDiceBearUrl(b.name, b.avatar_style || "bottts")}
                alt={b.name}
                class="size-4.5 rounded-full object-cover border border-white/20"
              />
              <span>{b.name}</span>
              <span
                class={cn(
                  "text-[10px] font-mono px-1.5 py-0.2 rounded-full",
                  isSelected ? "bg-white/20 text-white" : "bg-black/40 text-purple-300"
                )}
              >
                {botCount}
              </span>
            </button>
          {/each}
        </div>

        {#if currentBot}
          <div class="flex items-center gap-2 shrink-0">
            <span class="text-xs text-zinc-400">
              Active for <span class="font-bold text-purple-300">{currentBot.name}</span>:
              <span class="font-mono font-bold text-white ml-1">{activeBotCount}</span> connectors
            </span>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Search Bar & Scope Filters -->
    <div class="flex flex-col lg:flex-row gap-2.5 mt-3.5 items-stretch lg:items-center justify-between">
      <!-- Search input -->
      <div class="relative flex-1 min-w-[260px]">
        <Search class="absolute left-3 top-1/2 -translate-y-1/2 size-3.5 text-zinc-400 pointer-events-none" />
        <Input
          bind:value={query}
          placeholder="Search 135+ connectors by name, ID, category, or command (e.g. postgres, telegram, duckdb, github, aws)..."
          class="pl-9 pr-8 h-9 text-xs bg-[#11111c] border-[#222234] focus-visible:border-purple-500/60 focus-visible:ring-purple-500/20 text-zinc-100 placeholder:text-zinc-500 rounded-xl"
        />
        {#if query}
          <button
            type="button"
            class="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white p-1 cursor-pointer"
            onclick={() => (query = "")}
          >
            <X class="size-3.5" />
          </button>
        {/if}
      </div>

      <!-- Scope Filter Tabs -->
      <div class="flex flex-wrap items-center gap-1.5 shrink-0">
        <button
          type="button"
          class={cn(
            "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
            activeTab === "all"
              ? "bg-purple-600 text-white border-purple-400 shadow-[0_0_12px_rgba(147,51,234,0.35)] font-bold"
              : "bg-[#121220] text-zinc-400 border-[#202032] hover:bg-[#18182c] hover:text-zinc-200"
          )}
          onclick={() => (activeTab = "all")}
        >
          <Globe class="size-3" />
          <span>All</span>
          <span class="text-[10px] font-mono opacity-80">({servers.length})</span>
        </button>

        {#if currentBot}
          <button
            type="button"
            class={cn(
              "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              activeTab === "active"
                ? "bg-purple-600 text-white border-purple-400 shadow-[0_0_12px_rgba(147,51,234,0.35)] font-bold"
                : "bg-[#121220] text-zinc-400 border-[#202032] hover:bg-[#18182c] hover:text-zinc-200"
            )}
            onclick={() => (activeTab = "active")}
          >
            <BotIcon class="size-3 text-purple-300" />
            <span>Active for {currentBot.name}</span>
            <span class="text-[10px] font-mono opacity-80">({activeBotCount})</span>
          </button>
        {/if}

        <button
          type="button"
          class={cn(
            "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
            activeTab === "global"
              ? "bg-cyan-600 text-white border-cyan-400 shadow-[0_0_12px_rgba(6,182,212,0.35)] font-bold"
              : "bg-[#121220] text-zinc-400 border-[#202032] hover:bg-[#18182c] hover:text-zinc-200"
          )}
          onclick={() => (activeTab = "global")}
        >
          <Server class="size-3 text-cyan-300" />
          <span>Global Active</span>
          <span class="text-[10px] font-mono opacity-80">({globalCount})</span>
        </button>

        {#if missingKeysCount > 0}
          <button
            type="button"
            class={cn(
              "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              activeTab === "configured"
                ? "bg-amber-600 text-white border-amber-400 shadow-[0_0_12px_rgba(245,158,11,0.35)] font-bold"
                : "bg-[#121220] text-amber-300/90 border-amber-500/30 hover:bg-[#18182c] hover:text-amber-200"
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
              "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              activeTab === "custom"
                ? "bg-indigo-600 text-white border-indigo-400 shadow-[0_0_12px_rgba(99,102,241,0.35)] font-bold"
                : "bg-[#121220] text-zinc-400 border-[#202032] hover:bg-[#18182c] hover:text-zinc-200"
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

    <!-- Category Filter Chips with Proper Lucide Icons (No horizontal scrollbars) -->
    <div class="flex flex-wrap items-center gap-1.5 mt-3 pt-2.5 border-t border-white/5">
      {#each categoryDefs as cat}
        {@const count = getCategoryCount(cat.id)}
        {@const IconComponent = cat.icon}
        {#if count > 0 || cat.id === "All"}
          <button
            type="button"
            class={cn(
              "px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              selectedCategory === cat.id
                ? "bg-purple-900/70 text-purple-100 border-purple-400/80 shadow-[0_0_10px_rgba(168,85,247,0.3)] font-bold"
                : "bg-[#0f0f1c] text-zinc-400 border-[#1c1c2e] hover:bg-[#18182c] hover:text-zinc-200 hover:border-purple-500/30"
            )}
            onclick={() => (selectedCategory = cat.id)}
          >
            <IconComponent class="size-3.5 text-purple-400 shrink-0" />
            <span>{cat.label}</span>
            <span
              class={cn(
                "text-[10px] font-mono px-1 rounded",
                selectedCategory === cat.id ? "bg-purple-800 text-purple-100" : "text-zinc-500"
              )}
            >
              {count}
            </span>
          </button>
        {/if}
      {/each}
    </div>
  </div>

  <!-- Multi-Selection Action Toolbar Ribbon -->
  {#if selectedConnectorIds.size > 0}
    <div class="px-6 py-2.5 bg-gradient-to-r from-purple-950/90 via-indigo-950/90 to-purple-950/90 border-b border-purple-500/40 flex items-center justify-between gap-4 z-10 shrink-0 shadow-lg animate-in fade-in slide-in-from-top-2">
      <div class="flex items-center gap-3">
        <div class="size-6 rounded-lg bg-purple-500/30 border border-purple-400/50 flex items-center justify-center text-purple-200 text-xs font-bold font-mono">
          {selectedConnectorIds.size}
        </div>
        <span class="text-xs font-semibold text-white">
          connectors selected
        </span>
      </div>

      <div class="flex items-center gap-2">
        {#if currentBot}
          <Button
            size="sm"
            class="h-7.5 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium gap-1.5 shadow-sm cursor-pointer"
            onclick={() => batchAssignSelectedToCurrentBot(true)}
          >
            <Check class="size-3.5" />
            <span>Enable for {currentBot.name}</span>
          </Button>

          <Button
            size="sm"
            variant="outline"
            class="h-7.5 text-xs bg-[#161626] border-[#29293e] text-zinc-300 hover:bg-red-950/40 hover:text-red-300 hover:border-red-500/40 cursor-pointer"
            onclick={() => batchAssignSelectedToCurrentBot(false)}
          >
            <span>Disable for {currentBot.name}</span>
          </Button>
        {/if}

        <Button
          size="sm"
          variant="outline"
          class="h-7.5 text-xs bg-cyan-950/40 border-cyan-500/40 text-cyan-300 hover:bg-cyan-900/50 cursor-pointer"
          onclick={batchEnableSelectedGlobally}
        >
          <Globe class="size-3.5 mr-1" />
          <span>Enable Globally</span>
        </Button>

        <Button
          size="sm"
          variant="ghost"
          class="h-7.5 text-xs text-zinc-400 hover:text-white cursor-pointer"
          onclick={clearSelection}
        >
          <span>Clear Selection</span>
        </Button>
      </div>
    </div>
  {/if}

  <!-- Main Scrollable Connector Grid (Native Smooth Scroll) -->
  <div class="flex-1 min-h-0 overflow-y-auto px-6 py-5 overscroll-contain scroll-smooth focus:outline-none">
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 pb-12">
      {#each filtered as s (s.id)}
        {@const isCurrentBotEnabled = currentBotId ? currentBotServers.has(s.id) : false}
        {@const isGlobalEnabled = s.enabled}
        {@const hasRequiredKeys = s.env_keys.length > 0}
        {@const isKeyConfigured = s.env_configured}
        {@const isCardSelected = selectedConnectorIds.has(s.id)}
        {@const assignedBotsCount = s.assigned_bot_ids.length}

        <div
          class={cn(
            "rounded-2xl border p-4 transition-all flex flex-col justify-between gap-3 relative group backdrop-blur-md",
            isCardSelected
              ? "ring-2 ring-purple-400 border-purple-400/90 bg-purple-950/30 shadow-[0_0_25px_rgba(168,85,247,0.25)]"
              : isCurrentBotEnabled
                ? "border-purple-500/70 bg-purple-950/20 shadow-[0_0_15px_rgba(168,85,247,0.12)] ring-1 ring-purple-500/30"
                : isGlobalEnabled
                  ? "border-cyan-500/40 bg-[#0d1220]/85 hover:border-cyan-500/60"
                  : "border-[#1c1c2e] bg-[#0c0c16]/90 hover:border-purple-500/40 hover:bg-[#111120]"
          )}
        >
          <!-- Top Row: Checkbox, Icon, Title, ID, Category & Diagnostic Buttons -->
          <div class="space-y-2.5">
            <div class="flex items-start justify-between gap-2">
              <div class="flex items-start gap-2.5 min-w-0">
                <!-- Checkbox for multi-select -->
                <button
                  type="button"
                  class="mt-1 text-zinc-500 hover:text-purple-300 transition-colors cursor-pointer"
                  onclick={() => toggleSelectConnector(s.id)}
                  title={isCardSelected ? "Deselect" : "Select connector"}
                >
                  {#if isCardSelected}
                    <CheckSquare class="size-4 text-purple-400" />
                  {:else}
                    <Square class="size-4" />
                  {/if}
                </button>

                <!-- Vector Brand SVG Icon -->
                <ConnectorIcon id={s.id} name={s.name} size="md" />

                <!-- Name & Meta -->
                <div class="min-w-0">
                  <div class="flex items-center gap-1.5 flex-wrap">
                    <h4 class="font-bold text-xs text-white truncate max-w-[130px]" title={s.name}>
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

              <!-- Diagnostic & Actions Icons -->
              <div class="flex items-center gap-1 shrink-0">
                <button
                  type="button"
                  class="size-7 rounded-lg bg-[#161626] border border-[#252538] hover:border-purple-500/40 hover:bg-purple-950/30 text-zinc-400 hover:text-purple-300 flex items-center justify-center transition-all cursor-pointer"
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
                    class="size-7 rounded-lg bg-[#161626] border border-[#252538] hover:border-red-500/40 hover:bg-red-950/30 text-zinc-400 hover:text-red-400 flex items-center justify-center transition-all cursor-pointer"
                    title="Delete Custom Connector"
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

            <!-- Credential Status Banner -->
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

            <!-- Command Snippet with 1-Click Copy -->
            <div class="flex items-center justify-between gap-1 bg-[#080812] border border-[#1a1a2b] rounded-lg px-2 py-1 text-[10px] font-mono text-zinc-400">
              <span class="truncate" title={`${s.command} ${s.args.join(" ")}`}>
                <span class="text-purple-400">{s.command}</span> {s.args.slice(0, 3).join(" ")}
              </span>
              <button
                type="button"
                class="shrink-0 text-zinc-500 hover:text-white transition-colors cursor-pointer"
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

          <!-- Footer Action Buttons -->
          <div class="pt-2.5 border-t border-[#1c1c2e] flex flex-col gap-2 mt-1">
            <div class="flex items-center gap-1.5">
              {#if currentBot}
                <Button
                  size="sm"
                  variant={isCurrentBotEnabled ? "default" : "outline"}
                  class={cn(
                    "h-8 text-xs font-medium flex-1 cursor-pointer transition-all",
                    isCurrentBotEnabled
                      ? "bg-purple-600 hover:bg-purple-500 text-white shadow-[0_0_12px_rgba(168,85,247,0.3)] font-bold"
                      : "bg-[#131322] border-[#26263b] text-zinc-300 hover:bg-purple-950/30 hover:border-purple-500/40 hover:text-white"
                  )}
                  onclick={() => toggleForCurrentBot(s.id)}
                >
                  {#if isCurrentBotEnabled}
                    <Check class="size-3 mr-1" />
                    <span>Active for {currentBot.name}</span>
                  {:else}
                    <Plus class="size-3 mr-1" />
                    <span>Enable for {currentBot.name}</span>
                  {/if}
                </Button>
              {/if}

              <!-- Assign to Multiple Agents Button -->
              <Button
                size="sm"
                variant="outline"
                class="h-8 px-2.5 text-xs bg-[#131322] border-[#26263b] text-zinc-400 hover:text-white hover:border-purple-500/30 hover:bg-purple-950/30 cursor-pointer shrink-0"
                title="Assign this connector to multiple agents"
                onclick={() => openAssignAgentsModal(s)}
              >
                <BotIcon class="size-3.5" />
              </Button>
            </div>

            <!-- Global State indicator -->
            <div class="flex items-center justify-between text-[11px] text-zinc-500 px-1">
              <button
                type="button"
                class="hover:text-purple-300 transition-colors flex items-center gap-1 cursor-pointer"
                onclick={() => openAssignAgentsModal(s)}
              >
                <BotIcon class="size-3" />
                <span>Assigned to {assignedBotsCount} agent{assignedBotsCount === 1 ? '' : 's'}</span>
              </button>

              <button
                type="button"
                class={cn(
                  "flex items-center gap-1 transition-colors cursor-pointer",
                  isGlobalEnabled ? "text-cyan-400 font-semibold" : "text-zinc-500 hover:text-zinc-300"
                )}
                onclick={() => toggleGlobal(s.id, isGlobalEnabled)}
                title={isGlobalEnabled ? "Globally enabled for all agents" : "Click to enable globally"}
              >
                <Globe class="size-3" />
                <span>{isGlobalEnabled ? "Global Active" : "Global Off"}</span>
              </button>
            </div>
          </div>
        </div>
      {:else}
        <!-- Empty State -->
        <div class="col-span-full py-20 text-center border-2 border-dashed border-[#202034] rounded-3xl bg-[#0c0c16]/50 space-y-4">
          <div class="size-14 rounded-2xl bg-purple-950/40 border border-purple-800/30 flex items-center justify-center text-purple-400 mx-auto shadow-inner">
            <Wrench class="size-7" />
          </div>
          <div class="space-y-1">
            <p class="text-base font-bold text-white">No connectors match your current filter</p>
            <p class="text-xs text-zinc-400 max-w-md mx-auto leading-relaxed">
              Try clearing your search query, switching categories, or register a new custom stdio/SSE connector.
            </p>
          </div>
          <div class="flex justify-center gap-2 pt-2">
            {#if query || selectedCategory !== "All" || activeTab !== "all"}
              <Button
                size="sm"
                variant="outline"
                class="h-8 text-xs bg-[#161626] border-[#252538] text-zinc-300 cursor-pointer"
                onclick={() => {
                  query = "";
                  selectedCategory = "All";
                  activeTab = "all";
                }}
              >
                Clear All Filters
              </Button>
            {/if}
            <Button
              size="sm"
              class="h-8 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium gap-1.5 cursor-pointer"
              onclick={() => (showAddCustom = true)}
            >
              <Plus class="size-3.5" />
              <span>Add Custom Connector</span>
            </Button>
          </div>
        </div>
      {/each}
    </div>
  </div>
</div>

<!-- Modal 1: Preset Stacks Drawer -->
{#if showPresetModal}
  <Dialog.Root open={showPresetModal} onOpenChange={(o) => !o && (showPresetModal = false)}>
    <Dialog.Content class="sm:max-w-2xl max-w-2xl bg-[#0c0c16] border border-purple-500/30 rounded-3xl p-6 text-zinc-100 shadow-[0_0_50px_rgba(147,51,234,0.25)]">
      <Dialog.Header class="pb-3 border-b border-white/10">
        <Dialog.Title class="text-base font-bold text-white flex items-center gap-2">
          <Sparkles class="size-4.5 text-purple-400" />
          <span>One-Click Connector Stacks for {currentBot ? currentBot.name : "Agents"}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-zinc-400">
          Apply curated multi-connector bundles designed for specific workflows and engineering domains.
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-3 py-4 max-h-[65vh] overflow-y-auto pr-1">
        {#each PRESET_STACKS as stack}
          {@const IconComp = stack.iconComponent}
          <div class="rounded-2xl border p-4 bg-[#10101f] border-[#222238] flex flex-col justify-between gap-3 hover:border-purple-500/40 transition-colors">
            <div class="flex items-start justify-between gap-3">
              <div class="flex items-start gap-3">
                <div class="size-10 rounded-xl bg-purple-950/40 border border-purple-500/30 flex items-center justify-center text-purple-300 shrink-0">
                  <IconComp class="size-5" />
                </div>
                <div>
                  <h4 class="font-bold text-sm text-white">{stack.name}</h4>
                  <p class="text-xs text-zinc-400 mt-0.5">{stack.description}</p>
                </div>
              </div>

              <Button
                size="sm"
                class="h-8 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium shrink-0 cursor-pointer shadow-sm"
                disabled={applyingPreset || !currentBotId}
                onclick={() => applyPresetStack(stack)}
              >
                <span>Apply Stack</span>
                <ArrowRight class="size-3.5 ml-1" />
              </Button>
            </div>

            <!-- Connector Tags Included -->
            <div class="flex flex-wrap items-center gap-1.5 pt-2 border-t border-white/5">
              <span class="text-[10px] font-mono text-zinc-500 uppercase">Includes:</span>
              {#each stack.connectors as cid}
                <span class="text-[10px] bg-purple-950/60 border border-purple-500/25 text-purple-300 px-2 py-0.5 rounded-md font-mono">
                  {cid}
                </span>
              {/each}
            </div>
          </div>
        {/each}
      </div>

      <div class="flex justify-end pt-3 border-t border-white/10">
        <Button variant="outline" class="h-8 text-xs border-[#29293e] text-zinc-300" onclick={() => (showPresetModal = false)}>
          Close
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 2: Assign Connector to Multiple Agents -->
{#if showAssignAgentsModal && selectedConnectorForAgentAssignment}
  <Dialog.Root open={showAssignAgentsModal} onOpenChange={(o) => !o && (showAssignAgentsModal = false)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[#0c0c16] border border-purple-500/30 rounded-3xl p-6 text-zinc-100 shadow-[0_0_50px_rgba(147,51,234,0.25)]">
      <Dialog.Header class="pb-3 border-b border-white/10">
        <Dialog.Title class="text-base font-bold text-white flex items-center gap-2">
          <ConnectorIcon id={selectedConnectorForAgentAssignment.id} name={selectedConnectorForAgentAssignment.name} size="sm" />
          <span>Assign '{selectedConnectorForAgentAssignment.name}'</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-zinc-400">
          Check which agents should have access to this connector.
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-2 py-4 max-h-[50vh] overflow-y-auto">
        {#each bots as b (b.id)}
          {@const isAssigned = assignedAgentIds.has(b.id)}
          <button
            type="button"
            class={cn(
              "w-full flex items-center justify-between p-3 rounded-xl border transition-all text-left cursor-pointer",
              isAssigned
                ? "bg-purple-950/40 border-purple-500/60 text-white"
                : "bg-[#11111e] border-[#222234] text-zinc-400 hover:bg-[#161628] hover:text-zinc-200"
            )}
            onclick={() => {
              const next = new Set(assignedAgentIds);
              if (next.has(b.id)) {
                next.delete(b.id);
              } else {
                next.add(b.id);
              }
              assignedAgentIds = next;
            }}
          >
            <div class="flex items-center gap-3">
              <img
                src={b.avatar_url || getDiceBearUrl(b.name, b.avatar_style || "bottts")}
                alt={b.name}
                class="size-8 rounded-full object-cover border border-purple-400/40"
              />
              <div>
                <span class="font-bold text-xs text-white block">{b.name}</span>
                <span class="text-[10px] text-zinc-400 font-mono">{b.model || "Default Model"}</span>
              </div>
            </div>

            <div class={cn("size-5 rounded-md border flex items-center justify-center", isAssigned ? "bg-purple-600 border-purple-400 text-white" : "border-zinc-700")}>
              {#if isAssigned}
                <Check class="size-3.5" />
              {/if}
            </div>
          </button>
        {/each}
      </div>

      <div class="flex items-center justify-between pt-3 border-t border-white/10">
        <Button
          size="sm"
          variant="ghost"
          class="h-8 text-xs text-zinc-400 hover:text-white"
          onclick={() => {
            if (assignedAgentIds.size === bots.length) {
              assignedAgentIds = new Set();
            } else {
              assignedAgentIds = new Set(bots.map((b) => b.id));
            }
          }}
        >
          {assignedAgentIds.size === bots.length ? "Deselect All" : "Select All Agents"}
        </Button>

        <div class="flex gap-2">
          <Button size="sm" variant="outline" class="h-8 text-xs border-[#29293e]" onclick={() => (showAssignAgentsModal = false)}>
            Cancel
          </Button>
          <Button
            size="sm"
            class="h-8 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium"
            disabled={isSavingAgentAssignment}
            onclick={saveAgentAssignment}
          >
            <span>{isSavingAgentAssignment ? "Saving..." : "Save Assignments"}</span>
          </Button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 3: Add Custom Connector -->
{#if showAddCustom}
  <Dialog.Root open={showAddCustom} onOpenChange={(o) => !o && (showAddCustom = false)}>
    <Dialog.Content class="sm:max-w-lg max-w-lg bg-[#0c0c16] border border-purple-500/30 rounded-3xl p-6 text-zinc-100 shadow-[0_0_50px_rgba(147,51,234,0.25)]">
      <Dialog.Header class="pb-3 border-b border-white/10">
        <Dialog.Title class="text-base font-bold text-white flex items-center gap-2">
          <Plus class="size-4.5 text-purple-400" />
          <span>Register Custom MCP / Stdio Connector</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-zinc-400">
          Connect any local binary, Python script, Docker container, or SSE endpoint.
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-3.5 py-4 max-h-[60vh] overflow-y-auto pr-1">
        {#if customError}
          <div class="p-2.5 rounded-xl bg-red-950/40 border border-red-500/40 text-red-300 text-xs font-mono">
            {customError}
          </div>
        {/if}

        <div class="grid grid-cols-3 gap-3">
          <div class="col-span-2 space-y-1.5">
            <Label class="text-xs font-bold text-white">Connector ID</Label>
            <Input
              bind:value={customId}
              placeholder="e.g. my-custom-db"
              class="h-8.5 text-xs font-mono bg-[#141420] border-[#252538]"
            />
          </div>
          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-white">Icon Emoji / Symbol</Label>
            <Input
              bind:value={customIcon}
              placeholder="⚡"
              class="h-8.5 text-xs text-center bg-[#141420] border-[#252538]"
            />
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-white">Display Name</Label>
            <Input
              bind:value={customName}
              placeholder="e.g. Custom Analytics DB"
              class="h-8.5 text-xs bg-[#141420] border-[#252538]"
            />
          </div>
          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-white">Category</Label>
            <select
              bind:value={customCategory}
              class="w-full h-8.5 rounded-xl text-xs bg-[#141424] border border-[#28283e] text-zinc-200 px-3 pr-8 focus:outline-none focus:border-purple-500 cursor-pointer"
            >
              {#each categoryDefs.filter((c) => c.id !== "All") as c}
                <option value={c.id} class="bg-[#0e0e1a] text-zinc-100">{c.label || c.id}</option>
              {/each}
            </select>
          </div>
        </div>

        <div class="space-y-1.5">
          <Label class="text-xs font-bold text-white">Description</Label>
          <Textarea
            bind:value={customDesc}
            placeholder="Describe what capabilities and operations this connector provides to agents..."
            class="text-xs bg-[#141420] border-[#252538] min-h-[50px]"
          />
        </div>

        <div class="grid grid-cols-3 gap-3">
          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-white">Command</Label>
            <Input
              bind:value={customCommand}
              placeholder="npx / python / uvx"
              class="h-8.5 text-xs font-mono bg-[#141420] border-[#252538]"
            />
          </div>
          <div class="col-span-2 space-y-1.5">
            <Label class="text-xs font-bold text-white">Arguments</Label>
            <Input
              bind:value={customArgs}
              placeholder="-y @my-org/mcp-server --port 8000"
              class="h-8.5 text-xs font-mono bg-[#141420] border-[#252538]"
            />
          </div>
        </div>

        <div class="space-y-1.5">
          <Label class="text-xs font-bold text-white">Required Environment Keys</Label>
          <Input
            bind:value={customEnvKeys}
            placeholder="API_KEY, DB_URL (comma separated)"
            class="h-8.5 text-xs font-mono bg-[#141420] border-[#252538]"
          />
        </div>
      </div>

      <div class="flex justify-end gap-2 pt-3 border-t border-white/10">
        <Button size="sm" variant="outline" class="h-8 text-xs border-[#29293e]" onclick={() => (showAddCustom = false)}>
          Cancel
        </Button>
        <Button
          size="sm"
          class="h-8 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium"
          disabled={isSavingCustom}
          onclick={saveCustomServer}
        >
          <span>{isSavingCustom ? "Registering..." : "Save Connector"}</span>
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 4: Configure Credentials Drawer -->
{#if showConfigEnv && selectedServerForEnv}
  <Dialog.Root open={showConfigEnv} onOpenChange={(o) => !o && (showConfigEnv = false)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[#0c0c16] border border-purple-500/30 rounded-3xl p-6 text-zinc-100 shadow-[0_0_50px_rgba(147,51,234,0.25)]">
      <Dialog.Header class="pb-3 border-b border-white/10">
        <Dialog.Title class="text-base font-bold text-white flex items-center gap-2">
          <Key class="size-4.5 text-amber-400" />
          <span>Configure '{selectedServerForEnv.name}' Credentials</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-zinc-400">
          Encrypted locally in OS keychain. Never transmitted to third-party servers.
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-3.5 py-4 max-h-[50vh] overflow-y-auto">
        {#each selectedServerForEnv.env_keys as k}
          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-zinc-300 font-mono flex items-center justify-between">
              <span>{k}</span>
              <span class="text-[9px] text-zinc-500 uppercase">Secret Token</span>
            </Label>
            <div class="relative">
              <Input
                type={showSecrets[k] ? "text" : "password"}
                bind:value={envValues[k]}
                placeholder={`Enter ${k}...`}
                class="pr-9 h-8.5 text-xs font-mono bg-[#141420] border-[#252538] text-zinc-200"
              />
              <button
                type="button"
                class="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white cursor-pointer"
                onclick={() => (showSecrets[k] = !showSecrets[k])}
              >
                {#if showSecrets[k]}
                  <EyeOff class="size-3.5" />
                {:else}
                  <Eye class="size-3.5" />
                {/if}
              </button>
            </div>
          </div>
        {/each}
      </div>

      <div class="flex justify-end gap-2 pt-3 border-t border-white/10">
        <Button size="sm" variant="outline" class="h-8 text-xs border-[#29293e]" onclick={() => (showConfigEnv = false)}>
          Cancel
        </Button>
        <Button
          size="sm"
          class={cn("h-8 text-xs font-medium", envSaveSuccess ? "bg-emerald-600 text-white" : "bg-purple-600 hover:bg-purple-500 text-white")}
          disabled={isSavingEnv}
          onclick={saveEnvConfig}
        >
          {#if envSaveSuccess}
            <Check class="size-3.5 mr-1" />
            <span>Saved Successfully</span>
          {:else}
            <span>{isSavingEnv ? "Saving..." : "Save Credentials"}</span>
          {/if}
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 5: Live Test & Tool Inspector -->
{#if showTestModal && selectedServerForTest}
  <Dialog.Root open={showTestModal} onOpenChange={(o) => !o && (showTestModal = false)}>
    <Dialog.Content class="sm:max-w-xl max-w-xl bg-[#0c0c16] border border-purple-500/30 rounded-3xl p-6 text-zinc-100 shadow-[0_0_50px_rgba(147,51,234,0.25)]">
      <Dialog.Header class="pb-3 border-b border-white/10">
        <Dialog.Title class="text-base font-bold text-white flex items-center gap-2">
          <Zap class="size-4.5 text-purple-400" />
          <span>Diagnostic Inspector — {selectedServerForTest.name}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-zinc-400">
          Verifying MCP protocol endpoint, measuring round-trip latency, and discovering exposed tools.
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-4 py-4 max-h-[55vh] overflow-y-auto">
        {#if isTesting}
          <div class="py-12 text-center space-y-2">
            <RefreshCw class="size-8 animate-spin text-purple-400 mx-auto" />
            <p class="text-xs text-zinc-300 font-semibold">Testing MCP process & handshake...</p>
          </div>
        {:else if testResult}
          <div class={cn("p-3.5 rounded-2xl border flex items-center justify-between", testResult.success ? "bg-emerald-950/30 border-emerald-500/40 text-emerald-300" : "bg-red-950/30 border-red-500/40 text-red-300")}>
            <div class="flex items-center gap-2.5">
              <span class="text-lg">{testResult.success ? "✅" : "❌"}</span>
              <div>
                <span class="font-bold text-xs block text-white">{testResult.success ? "Connection Online" : "Connection Failed"}</span>
                <span class="text-[11px] font-mono opacity-80">{testResult.message}</span>
              </div>
            </div>
            {#if testResult.success}
              <Badge variant="outline" class="bg-emerald-900/40 border-emerald-500/40 text-emerald-300 font-mono text-[10px]">
                {testResult.latency_ms}ms latency
              </Badge>
            {/if}
          </div>

          {#if testResult.tools.length > 0}
            <div class="space-y-2">
              <h5 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                <Code class="size-3.5 text-purple-400" />
                <span>Discovered Native Tools ({testResult.tools.length}):</span>
              </h5>
              <div class="space-y-2">
                {#each testResult.tools as tool}
                  <div class="p-3 rounded-xl bg-[#111120] border border-[#202034] space-y-1">
                    <div class="flex items-center justify-between">
                      <span class="font-mono font-bold text-xs text-purple-300">{tool.name}</span>
                    </div>
                    <p class="text-[11px] text-zinc-400">{tool.description}</p>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        {/if}
      </div>

      <div class="flex justify-end pt-3 border-t border-white/10">
        <Button size="sm" variant="outline" class="h-8 text-xs border-[#29293e]" onclick={() => (showTestModal = false)}>
          Close Inspector
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Custom Connector Delete Confirmation Modal -->
{#if serverToDelete}
  <Dialog.Root open={Boolean(serverToDelete)} onOpenChange={(o) => !o && (serverToDelete = null)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[#0e0e1a] border border-red-500/40 rounded-3xl p-6 text-zinc-100 shadow-[0_0_60px_rgba(239,68,68,0.3)] flex flex-col gap-4 font-sans select-none z-50">
      <div class="flex items-start gap-3.5">
        <div class="size-11 rounded-2xl bg-red-950/70 border border-red-500/50 flex items-center justify-center text-red-400 shrink-0 shadow-[0_0_20px_rgba(239,68,68,0.35)]">
          <AlertCircle class="size-6" />
        </div>
        <div class="space-y-1">
          <Dialog.Title class="text-base font-extrabold text-white">
            Delete Custom Connector?
          </Dialog.Title>
          <Dialog.Description class="text-xs text-zinc-400 leading-relaxed">
            Are you sure you want to delete custom connector <span class="text-white font-bold font-mono">"{serverToDelete.name}"</span>?
            This will remove the tool registration and unbind it from all agents.
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
          <span>{isDeletingServer ? "Deleting..." : "Delete Connector"}</span>
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}
