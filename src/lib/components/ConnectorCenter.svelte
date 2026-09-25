<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
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
  import { notify } from "$lib/toast";
  import { t } from "$lib/i18n";
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
    MoreHorizontal,
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
    verified: boolean;
    env_configured: boolean;
    assigned_bot_ids: string[];
    tools_count: number;
    /** Remote (HTTP) MCP servers — P8. Absent/null for stdio servers. */
    url?: string | null;
    transport?: string;
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
  // Overflow menu (one open at a time)
  let activeMenu = $state<string | null>(null);

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
  let customArgs = $state("-y\n@my-org/mcp-server");
  let customEnvKeys = $state("");
  // Remote (HTTP) transport — P8. URL wins over command when set.
  let customTransport = $state<"stdio" | "http">("stdio");
  let customUrl = $state("");
  let customHeaders = $state("");
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

  const categoryDefs = $derived([
    { id: "All", label: t("connector.catAll"), icon: Globe },
    { id: "Development & Coding", label: t("connector.catDev"), icon: Code },
    { id: "Databases", label: t("connector.catDb"), icon: Database },
    { id: "Web & Research", label: t("connector.catWeb"), icon: Search },
    { id: "Cloud & Infrastructure", label: t("connector.catCloud"), icon: Server },
    { id: "Productivity", label: t("connector.catProd"), icon: Zap },
    { id: "Design & Creative", label: t("connector.catDesign"), icon: Layers },
    { id: "AI / ML", label: t("connector.catAi"), icon: BotIcon },
    { id: "Business / Commerce", label: t("connector.catBiz"), icon: Key },
    { id: "Finance & Web3", label: t("connector.catFin"), icon: TrendingUp },
    { id: "Social & Messaging", label: t("connector.catSocial"), icon: MessageSquare },
    { id: "Security / Observability", label: t("connector.catSec"), icon: Shield },
    { id: "Smart Home & IoT", label: t("connector.catHome"), icon: Home },
    { id: "Local Computer", label: t("connector.catLocal"), icon: Terminal },
    { id: "Custom", label: t("connector.catCustom"), icon: Sparkles },
  ]);

  const PRESET_STACKS = $derived([
    {
      id: "fullstack",
      name: t("connector.p1n"),
      iconComponent: Terminal,
      description: t("connector.p1d"),
      connectors: ["github", "postgres", "redis", "docker", "sentry", "shell", "filesystem", "postman"],
    },
    {
      id: "data_ai",
      name: t("connector.p2n"),
      iconComponent: Database,
      description: t("connector.p2d"),
      connectors: ["duckdb", "arxiv", "wikipedia", "wolfram_alpha", "brave_search", "openai", "pinecone", "exa"],
    },
    {
      id: "devops",
      name: t("connector.p3n"),
      iconComponent: Server,
      description: t("connector.p3d"),
      connectors: ["aws", "kubernetes", "github_actions", "terraform", "datadog", "grafana", "cloudflare", "argocd"],
    },
    {
      id: "finance_crypto",
      name: t("connector.p4n"),
      iconComponent: TrendingUp,
      description: t("connector.p4d"),
      connectors: ["yfinance", "alpha_vantage", "coingecko", "etherscan", "alchemy", "gsheets", "duckdb"],
    },
    {
      id: "growth_marketing",
      name: t("connector.p5n"),
      iconComponent: MessageSquare,
      description: t("connector.p5d"),
      connectors: ["telegram", "twitter", "reddit", "resend", "hubspot", "stripe", "notion", "slack"],
    },
    {
      id: "security",
      name: t("connector.p6n"),
      iconComponent: Shield,
      description: t("connector.p6d"),
      connectors: ["semgrep", "snyk", "vault", "onepassword", "tailscale", "shodan", "virustotal"],
    },
  ]);

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

  // Backend category ids are English constants — render their translated labels.
  function categoryLabel(catId: string): string {
    return categoryDefs.find((c) => c.id === catId)?.label ?? catId;
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
      notify(nextState ? t("connector.nEnabledForAgent") : t("connector.nRemovedFromAgent"), "success");
    } catch (e) {
      notify(t("connector.nFailToggle") + String(e), "error");
    }
  }

  async function toggleGlobal(serverId: string, currentEnabled: boolean) {
    try {
      await invoke("toggle_mcp_server", { serverId, enabled: !currentEnabled });
      servers = servers.map((s) => (s.id === serverId ? { ...s, enabled: !currentEnabled } : s));
      notify(!currentEnabled ? t("connector.nGlobalOn") : t("connector.nGlobalOff"), "success");
    } catch (e) {
      notify(t("connector.nFailGlobal") + String(e), "error");
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
      notify(t("connector.nFailBatch") + String(e), "error");
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
      notify(t("connector.nFailBatchEnable") + String(e), "error");
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
      notify(t("connector.nFailPreset") + String(e), "error");
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
      notify(t("connector.nFailAssign") + String(e), "error");
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
      notify(t("connector.nCredsSaved"), "success");
      await load();
      setTimeout(() => {
        showConfigEnv = false;
        envSaveSuccess = false;
      }, 1000);
    } catch (e) {
      notify(t("connector.nFailCreds") + String(e), "error");
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

  const MCP_NAME_RE = /^[a-z][a-z0-9_-]{0,31}$/;
  const ENV_NAME_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;

  // Custom Server Save
  async function saveCustomServer() {
    const cleanId = customId.trim().toLowerCase().replace(/\s+/g, "-");
    if (!customId.trim() || !customName.trim()) {
      customError = t("connector.errRequired");
      return;
    }
    if (!MCP_NAME_RE.test(cleanId)) {
      customError = t("connector.errIdFormat");
      return;
    }
    customError = "";
    isSavingCustom = true;

    // URL XOR command (P8, mirrors backend validate_server_for_save):
    // HTTP servers carry only a URL (+ optional headers), stdio only a command.
    let command = "";
    let parsedArgs: string[] = [];
    const parsedHeaders: Record<string, string> = {};
    if (customTransport === "http") {
      const url = customUrl.trim();
      if (!url) {
        customError = t("connector.errUrlRequired");
        isSavingCustom = false;
        return;
      }
      if (!/^https?:\/\//i.test(url)) {
        customError = t("connector.errUrlScheme");
        isSavingCustom = false;
        return;
      }
      const lines = customHeaders.split(/\r?\n/).filter((l) => l.trim());
      if (lines.length > 32) {
        customError = t("connector.errTooManyHeaders");
        isSavingCustom = false;
        return;
      }
      for (const rawLine of lines) {
        const line = rawLine.trim();
        const colon = line.indexOf(":");
        const name = colon < 0 ? "" : line.slice(0, colon).trim();
        const value = colon < 0 ? "" : line.slice(colon + 1).trim();
        if (!name) {
          customError = t("connector.errHeaderName", { line });
          isSavingCustom = false;
          return;
        }
        parsedHeaders[name] = value;
      }
    } else {
      command = customCommand.trim() || "npx";
      parsedArgs = customArgs
        .split(/\r?\n/)
        .map((s) => s.trim())
        .filter(Boolean);
      if (parsedArgs.length > 64) {
        customError = t("connector.errArgs");
        isSavingCustom = false;
        return;
      }
    }
    const parsedEnv: Record<string, string> = {};
    for (const rawLine of customEnvKeys.split(/\r?\n/)) {
      const line = rawLine.trim();
      if (!line) continue;
      const eq = line.indexOf("=");
      const key = (eq < 0 ? line : line.slice(0, eq)).trim();
      const val = eq < 0 ? "" : line.slice(eq + 1);
      if (!ENV_NAME_RE.test(key)) {
        customError = t("connector.errEnvName", { key });
        isSavingCustom = false;
        return;
      }
      if (key === "ELECTRON_RUN_AS_NODE" || key.startsWith("OMB_") || key.startsWith("OGB_")) {
        customError = t("connector.errEnvReserved", { key });
        isSavingCustom = false;
        return;
      }
      parsedEnv[key] = val;
    }
    const parsedEnvKeys = Object.keys(parsedEnv);

    const config = {
      id: customId.trim().toLowerCase().replace(/\s+/g, "-"),
      name: customName.trim(),
      description: customDesc.trim() || "Custom MCP server connector",
      category: customCategory,
      icon: customIcon.trim() || "⚡",
      command,
      args: parsedArgs,
      env_keys: parsedEnvKeys,
      url: customTransport === "http" ? customUrl.trim() : null,
      transport: customTransport,
      headers: parsedHeaders,
      enabled_by_default: false,
      is_custom: true,
    };

    try {
      await invoke("save_custom_mcp_server", { server: config });
      showAddCustom = false;
      customId = "";
      customName = "";
      customDesc = "";
      customArgs = "-y\n@my-org/mcp-server";
      customEnvKeys = "";
      customTransport = "stdio";
      customUrl = "";
      customHeaders = "";
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
      notify(t("connector.nDeleted"), "success");
      await load();
    } catch (e) {
      notify(t("connector.nFailDelete") + String(e), "error");
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

<svelte:window onclick={() => (activeMenu = null)} />

<div class="flex flex-col h-full w-full min-h-0 bg-[var(--surface-0)] text-[var(--text-primary)] overflow-hidden select-none font-sans">
  <!-- Compact pane header (OpenBot design system) -->
  <div class="px-4 py-2.5 border-b border-[var(--hairline)] shrink-0 bg-[var(--surface-1)]">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <!-- Title -->
      <div class="flex items-center gap-2 min-w-0">
        <Layers class="size-4 text-[var(--brand-text)] shrink-0" />
        <h2 class="text-[13px] font-bold text-[var(--text-primary)] tracking-wide truncate">{t("connector.title")}</h2>
        <span class="text-[10px] bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/30 px-1.5 py-0.5 rounded-md font-mono font-bold shrink-0">
          {t("connector.count", { n: servers.length })}
        </span>
      </div>

      <!-- Header Action Controls -->
      <div class="flex items-center gap-2.5 flex-wrap">
        <Button
          size="sm"
          variant="outline"
          class="h-8.5 gap-1.5 text-xs bg-[var(--surface-2)] border-[var(--brand)]/30 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] hover:text-[var(--text-primary)] cursor-pointer shadow-sm"
          onclick={() => (showPresetModal = true)}
        >
          <Sparkles class="size-3.5 text-[var(--brand-text)]" />
          <span>{t("connector.presetStacks")}</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          class="h-8.5 gap-1.5 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--hairline)] hover:text-[var(--text-primary)] cursor-pointer shadow-sm"
          onclick={() => load()}
          disabled={syncing}
        >
          <RefreshCw class={cn("size-3.5", syncing && "animate-spin text-[var(--brand-text)]")} />
          <span>{syncing ? t("connector.syncing") : t("settings.refresh")}</span>
        </Button>

        <Button
          size="sm"
          class="h-8.5 gap-1.5 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium cursor-pointer"
          onclick={() => (showAddCustom = true)}
        >
          <Plus class="size-3.5" />
          <span>{t("connector.addCustom")}</span>
        </Button>
      </div>
    </div>

    <!-- Agent Quick Selector Strip -->
    {#if bots.length > 0}
      <div class="mt-2.5 pt-2.5 border-t border-[var(--hairline)] flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div class="flex items-center gap-2 overflow-x-auto pb-1 no-scrollbar">
          <span class="text-xs font-semibold text-[var(--text-tertiary)] uppercase font-mono shrink-0 mr-1 flex items-center gap-1.5">
            <BotIcon class="size-3.5 text-[var(--brand-text)]" />
            <span>{t("connector.selectAgent")}</span>
          </span>
          {#each bots as b (b.id)}
            {@const isSelected = currentBotId === b.id}
            {@const botCount = botServersMap[b.id]?.size || 0}
            <button
              type="button"
              class={cn(
 "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-2 cursor-pointer shrink-0 border",
                isSelected
                  ? "bg-[var(--brand)] text-[var(--text-on-light)] border-[var(--brand)] font-bold scale-[1.02]"
                  : "bg-[var(--surface-2)] text-[var(--text-secondary)] border-[var(--hairline)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
              )}
              onclick={() => {
                currentBotId = b.id;
                onSelectBot?.(b.id);
              }}
            >
              <img
                src={b.avatar_url || getDiceBearUrl(b.name, b.avatar_style || "bottts")}
                alt={b.name}
                class="size-4.5 rounded-full object-cover border border-[var(--hairline-strong)]"
              />
              <span>{b.name}</span>
              <span
                class={cn(
 "text-[10px] font-mono px-1.5 py-[2px] rounded-full",
                  isSelected ? "bg-[var(--surface-3)] text-[var(--text-primary)]" : "bg-[var(--surface-1)] text-[var(--brand-text)]"
                )}
              >
                {botCount}
              </span>
            </button>
          {/each}
        </div>

        {#if currentBot}
          <div class="flex items-center gap-2 shrink-0">
            <span class="text-xs text-[var(--text-tertiary)]">
              {t("connector.activeFor")} <span class="font-bold text-[var(--brand-text)]">{currentBot.name}</span>:
              <span class="font-mono font-bold text-[var(--text-primary)] ml-1">{activeBotCount}</span> {t("connector.connectorsUnit")}
            </span>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Search Bar & Scope Filters -->
    <div class="flex flex-col lg:flex-row gap-2.5 mt-3.5 items-stretch lg:items-center justify-between">
      <!-- Search input -->
      <div class="relative flex-1 min-w-[260px]">
        <Search class="absolute left-3 top-1/2 -translate-y-1/2 size-3.5 text-[var(--text-tertiary)] pointer-events-none" />
        <Input
          bind:value={query}
          aria-label={t("connector.searchPh")}
          placeholder={t("connector.searchPh")}
          class="pl-9 pr-8 h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] focus-visible:border-[var(--brand)]/60 focus-visible:ring-[var(--brand)]/20 text-[var(--text-primary)] placeholder:text-[var(--text-muted)] rounded-xl"
        />
        {#if query}
          <button
            type="button"
            class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)] hover:text-[var(--text-primary)] p-1 cursor-pointer"
            aria-label="Clear search"
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
              ? "bg-[var(--brand)] text-[var(--text-on-light)] border-[var(--brand)] font-bold"
              : "bg-[var(--surface-2)] text-[var(--text-tertiary)] border-[var(--hairline)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
          )}
          onclick={() => (activeTab = "all")}
          aria-pressed={activeTab === "all"}
        >
          <Globe class="size-3" />
          <span>{t("connector.tabAll")}</span>
          <span class="text-[10px] font-mono opacity-80">({servers.length})</span>
        </button>

        {#if currentBot}
          <button
            type="button"
            class={cn(
 "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              activeTab === "active"
                ? "bg-[var(--brand)] text-[var(--text-on-light)] border-[var(--brand)] font-bold"
                : "bg-[var(--surface-2)] text-[var(--text-tertiary)] border-[var(--hairline)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
            )}
            onclick={() => (activeTab = "active")}
            aria-pressed={activeTab === "active"}
          >
            <BotIcon class="size-3 text-[var(--brand-text)]" />
            <span>{t("connector.tabActiveFor", { name: currentBot.name })}</span>
            <span class="text-[10px] font-mono opacity-80">({activeBotCount})</span>
          </button>
        {/if}

        <button
          type="button"
          class={cn(
 "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
            activeTab === "global"
              ? "bg-[var(--brand)] text-[var(--text-on-light)] border-[var(--brand)] font-bold"
              : "bg-[var(--surface-2)] text-[var(--text-tertiary)] border-[var(--hairline)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
          )}
          onclick={() => (activeTab = "global")}
          aria-pressed={activeTab === "global"}
        >
          <Server class="size-3 text-[var(--brand-text)]" />
          <span>{t("connector.tabGlobal")}</span>
          <span class="text-[10px] font-mono opacity-80">({globalCount})</span>
        </button>

        {#if missingKeysCount > 0}
          <button
            type="button"
            class={cn(
 "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              activeTab === "configured"
                ? "bg-warning/15 text-warning border-warning/50 font-bold"
                : "bg-[var(--surface-2)] text-warning/90 border-warning/30 hover:bg-[var(--surface-3)] hover:text-warning"
            )}
            onclick={() => (activeTab = "configured")}
            aria-pressed={activeTab === "configured"}
          >
            <Key class="size-3 text-warning" />
            <span>{t("connector.tabNeedsKeys")}</span>
            <span class="text-[10px] font-mono opacity-80">({missingKeysCount})</span>
          </button>
        {/if}

        {#if customCount > 0}
          <button
            type="button"
            class={cn(
 "px-3 py-1.5 rounded-xl text-xs font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              activeTab === "custom"
                ? "bg-[var(--brand)] text-[var(--text-on-light)] border-[var(--brand)] font-bold"
                : "bg-[var(--surface-2)] text-[var(--text-tertiary)] border-[var(--hairline)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
            )}
            onclick={() => (activeTab = "custom")}
            aria-pressed={activeTab === "custom"}
          >
            <Sparkles class="size-3 text-[var(--brand-text)]" />
            <span>{t("connector.tabCustom")}</span>
            <span class="text-[10px] font-mono opacity-80">({customCount})</span>
          </button>
        {/if}
      </div>
    </div>

    <!-- Category Filter Chips with Proper Lucide Icons (No horizontal scrollbars) -->
    <div class="flex flex-wrap items-center gap-1.5 mt-3 pt-2.5 border-t border-[var(--hairline)]">
      {#each categoryDefs as cat}
        {@const count = getCategoryCount(cat.id)}
        {@const IconComponent = cat.icon}
        {#if count > 0 || cat.id === "All"}
          <button
            type="button"
            class={cn(
 "px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all flex items-center gap-1.5 cursor-pointer border",
              selectedCategory === cat.id
                ? "bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/80 font-bold"
                : "bg-[var(--surface-2)] text-[var(--text-tertiary)] border-[var(--hairline)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)]"
            )}
            onclick={() => (selectedCategory = cat.id)}
            aria-pressed={selectedCategory === cat.id}
          >
            <IconComponent class="size-3.5 text-[var(--brand-text)] shrink-0" />
            <span>{cat.label}</span>
            <span
              class={cn(
 "text-[10px] font-mono px-1 rounded",
                selectedCategory === cat.id ? "bg-[var(--brand-soft)] text-[var(--brand-text)]" : "text-[var(--text-muted)]"
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
    <div class="px-6 py-2.5 bg-[var(--brand-soft)] border-b border-[var(--hairline)] flex items-center justify-between gap-4 z-10 shrink-0 shadow-lg animate-in fade-in slide-in-from-top-2">
      <div class="flex items-center gap-3">
        <div class="size-6 rounded-lg bg-[var(--brand-soft)] border border-[var(--brand)]/50 flex items-center justify-center text-[var(--brand-text)] text-xs font-bold font-mono">
          {selectedConnectorIds.size}
        </div>
        <span class="text-xs font-semibold text-[var(--text-primary)]">
          {t("connector.selectedN", { n: selectedConnectorIds.size })}
        </span>
      </div>

      <div class="flex items-center gap-2">
        {#if currentBot}
          <Button
            size="sm"
            class="h-7.5 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium gap-1.5 shadow-sm cursor-pointer"
            onclick={() => batchAssignSelectedToCurrentBot(true)}
          >
            <Check class="size-3.5" />
            <span>{t("connector.enableFor", { name: currentBot.name })}</span>
          </Button>

          <Button
            size="sm"
            variant="outline"
            class="h-7.5 text-xs bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-red-950/40 hover:text-red-300 hover:border-red-500/40 cursor-pointer"
            onclick={() => batchAssignSelectedToCurrentBot(false)}
          >
            <span>{t("connector.disableFor", { name: currentBot.name })}</span>
          </Button>
        {/if}

        <Button
          size="sm"
          variant="outline"
          class="h-7.5 text-xs bg-[var(--brand-soft)] border-[var(--brand)]/40 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer"
          onclick={batchEnableSelectedGlobally}
        >
          <Globe class="size-3.5 mr-1" />
          <span>{t("connector.enableGlobally")}</span>
        </Button>

        <Button
          size="sm"
          variant="ghost"
          class="h-7.5 text-xs text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
          onclick={clearSelection}
        >
          <span>{t("connector.clearSelection")}</span>
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
 "rounded-2xl border transition-all flex flex-col gap-3 relative group p-3.5",
            isCardSelected
              ? "border-[var(--brand)] ring-2 ring-[var(--brand)]/40 bg-[var(--brand-soft)]"
              : isCurrentBotEnabled
                ? "border-[var(--brand)]/60 bg-[var(--brand-soft)]"
                : "border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-2)]"
          )}
        >
          <!-- Header: select, icon, identity, overflow -->
          <div class="flex items-start gap-2.5 min-w-0">
            <button
              type="button"
              class="mt-0.5 shrink-0 text-[var(--text-tertiary)] hover:text-[var(--brand)] transition-colors cursor-pointer"
              onclick={() => toggleSelectConnector(s.id)}
              aria-pressed={isCardSelected}
              aria-label={s.name}
              title={isCardSelected ? t("connector.deselect") : t("connector.selectOne")}
            >
              {#if isCardSelected}
                <CheckSquare class="size-4 text-[var(--brand)]" />
              {:else}
                <Square class="size-4" />
              {/if}
            </button>

            <ConnectorIcon id={s.id} name={s.name} size="md" />

            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5 min-w-0">
                <h4 class="font-bold text-xs text-[var(--text-primary)] truncate" title={s.name}>{s.name}</h4>
                {#if s.is_custom}
                  <span class="text-[9px] bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/40 px-1.5 py-[2px] rounded font-bold shrink-0">{t("connector.customBadge")}</span>
                {/if}
                {#if s.transport === "http"}
                  <span class="text-[9px] bg-info/10 text-info border border-info/25 px-1.5 py-[2px] rounded font-bold shrink-0" title={s.url || ""}>{t("connector.remoteBadge")}</span>
                {/if}
              </div>
              <div class="flex items-center gap-1.5 mt-0.5 text-[10px] text-[var(--text-muted)] min-w-0">
                <span class="truncate">{categoryLabel(s.category)}</span>
                <span>·</span>
                <span class="font-mono shrink-0">{t("connector.toolsCount", { n: s.tools_count })}</span>
              </div>
            </div>

            <!-- Overflow actions -->
            <div class="relative shrink-0">
              <button
                type="button"
                class="icon-btn size-7 border border-[var(--hairline)]"
                onclick={(e) => { e.stopPropagation(); activeMenu = activeMenu === s.id ? null : s.id; }}
                aria-expanded={activeMenu === s.id}
                aria-label={t("connector.moreActions")}
                title={t("connector.moreActions")}
              >
                <MoreHorizontal class="size-4" />
              </button>
              {#if activeMenu === s.id}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  class="absolute right-0 top-8 z-30 w-48 rounded-xl border border-[var(--hairline-strong)] bg-[var(--surface-1)] shadow-2xl p-1 space-y-0.5"
                  onclick={(e) => e.stopPropagation()}
                >
                  <button type="button" class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] text-left cursor-pointer" onclick={() => { activeMenu = null; openTestModal(s); }}>
                    <Zap class="size-3.5 text-[var(--text-tertiary)]" /> {t("connector.menuTest")}
                  </button>
                  {#if hasRequiredKeys}
                    <button type="button" class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] text-left cursor-pointer" onclick={() => { activeMenu = null; openEnvConfig(s); }}>
                      <Key class="size-3.5 text-[var(--text-tertiary)]" /> {t("connector.menuCredentials")}
                    </button>
                  {/if}
                  <button type="button" class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] text-left cursor-pointer" onclick={() => { activeMenu = null; copyCommand(s); }}>
                    {#if copiedId === s.id}<Check class="size-3.5 text-success" />{:else}<Copy class="size-3.5 text-[var(--text-tertiary)]" />{/if}
                    {copiedId === s.id ? t("connector.copied") : t("connector.copyCommand")}
                  </button>
                  {#if s.is_custom}
                    <button type="button" class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-[11px] text-danger hover:bg-danger/10 text-left cursor-pointer" onclick={() => { activeMenu = null; deleteServer(s); }}>
                      <Trash2 class="size-3.5" /> {t("connector.menuDelete")}
                    </button>
                  {/if}
                </div>
              {/if}
            </div>
          </div>

          <!-- Description -->
          <p class="text-[11px] text-[var(--text-tertiary)] line-clamp-2 leading-relaxed min-h-8">{s.description}</p>

          <!-- Single status line -->
          <button
            type="button"
            class="flex items-center gap-1.5 text-[10px] w-fit text-left cursor-pointer"
            aria-label={`${s.name}: ${s.verified ? (hasRequiredKeys && !isKeyConfigured ? t("connector.statusNeedsKeys", { keys: s.env_keys.slice(0, 2).join(", ") }) : isCurrentBotEnabled ? t("connector.tabActiveFor", { name: currentBot?.name }) : isGlobalEnabled ? t("connector.statusGlobalReady") : t("connector.statusReady")) : t("connector.statusUnverified")}`}
            title={hasRequiredKeys && !isKeyConfigured ? t("connector.tipConfigure") : s.description}
            onclick={() => { if (hasRequiredKeys) openEnvConfig(s); }}
          >
            {#if !s.verified}
              <span class="size-1.5 rounded-full bg-warning"></span><span class="text-warning">{t("connector.statusUnverified")}</span>
            {:else if hasRequiredKeys && !isKeyConfigured}
              <span class="size-1.5 rounded-full bg-warning"></span>
              <span class="text-warning">{t("connector.statusNeedsKeys", { keys: s.env_keys.slice(0, 2).join(", ") })}</span>
            {:else if isCurrentBotEnabled}
              <span class="size-1.5 rounded-full bg-success"></span><span class="text-success">{t("connector.tabActiveFor", { name: currentBot?.name })}</span>
            {:else if isGlobalEnabled}
              <span class="size-1.5 rounded-full bg-[var(--brand)]"></span><span class="text-[var(--brand-text)]">{t("connector.statusGlobalReady")}</span>
            {:else}
              <span class="size-1.5 rounded-full bg-[var(--surface-3)]"></span><span class="text-[var(--text-tertiary)]">{t("connector.statusReady")}</span>
            {/if}
          </button>

          <!-- Footer actions -->
          <div class="flex items-center gap-1.5 pt-2.5 border-t border-[var(--hairline)] mt-auto">
            {#if currentBot}
              <Button
                size="sm"
                variant={isCurrentBotEnabled ? "default" : "outline"}
                class={cn(
 "h-8 text-xs font-medium flex-1 cursor-pointer",
                  isCurrentBotEnabled ? "bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)]" : ""
                )}
                onclick={() => toggleForCurrentBot(s.id)}
                aria-pressed={isCurrentBotEnabled}
              >
                {#if isCurrentBotEnabled}<Check class="size-3 mr-1" /> {t("connector.btnActive")}{:else}<Plus class="size-3 mr-1" /> {t("connector.btnEnable")}{/if}
              </Button>
              <Button
                size="sm"
                variant="outline"
                class="h-8 px-2.5 text-xs cursor-pointer shrink-0"
                aria-label={t("connector.assignMultiTip")}
                title={t("connector.assignMultiTip")}
                onclick={() => openAssignAgentsModal(s)}
              >
                <BotIcon class="size-3.5" />
              </Button>
            {:else}
              <Button
                size="sm"
                variant="outline"
                class="h-8 text-xs font-medium flex-1 cursor-pointer gap-1.5"
                onclick={() => openAssignAgentsModal(s)}
              >
                <BotIcon class="size-3.5" />
                <span>{t("connector.assignAgents")}</span>
              </Button>
            {/if}
            <button
              type="button"
              class={cn(
 "h-8 px-2.5 rounded-lg border text-[10px] font-semibold flex items-center gap-1.5 transition-colors cursor-pointer shrink-0",
                isGlobalEnabled
                  ? "border-[var(--brand)]/40 bg-[var(--brand-soft)] text-[var(--brand-text)]"
                  : "border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]"
              )}
              onclick={() => toggleGlobal(s.id, isGlobalEnabled)}
              aria-pressed={isGlobalEnabled}
              title={isGlobalEnabled ? t("mcp.globalOnTip") : t("mcp.globalOffTip")}
            >
              <Globe class="size-3.5" />
              {t("connector.globalOn")}
            </button>
          </div>
        </div>
      {:else}
        <!-- Empty State -->
        <div class="col-span-full py-20 text-center border border-dashed border-[var(--hairline)] rounded-xl bg-[var(--surface-1)] space-y-4">
          <div class="size-14 rounded-2xl bg-[var(--brand-soft)] border border-[var(--brand)]/30 flex items-center justify-center text-[var(--brand-text)] mx-auto shadow-inner">
            <Wrench class="size-7" />
          </div>
          <div class="space-y-1">
            <p class="text-base font-bold text-[var(--text-primary)]">{t("connector.emptyTitle")}</p>
            <p class="text-xs text-[var(--text-tertiary)] max-w-md mx-auto leading-relaxed">
              {t("connector.emptyHint")}
            </p>
          </div>
          <div class="flex justify-center gap-2 pt-2">
            {#if query || selectedCategory !== "All" || activeTab !== "all"}
              <Button
                size="sm"
                variant="outline"
                class="h-8 text-xs bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] cursor-pointer"
                onclick={() => {
                  query = "";
                  selectedCategory = "All";
                  activeTab = "all";
                }}
              >
                {t("connector.clearFilters")}
              </Button>
            {/if}
            <Button
              size="sm"
              class="h-8 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium gap-1.5 cursor-pointer"
              onclick={() => (showAddCustom = true)}
            >
              <Plus class="size-3.5" />
              <span>{t("connector.addCustom")}</span>
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
    <Dialog.Content class="sm:max-w-2xl max-w-2xl bg-[var(--surface-1)] border border-[var(--brand)]/30 rounded-xl p-6 text-[var(--text-primary)]">
      <Dialog.Header class="pb-3 border-b border-[var(--hairline)]">
        <Dialog.Title class="text-base font-bold text-[var(--text-primary)] flex items-center gap-2">
          <Sparkles class="size-4.5 text-[var(--brand-text)]" />
          <span>{t("connector.presetTitle", { name: currentBot ? currentBot.name : t("connector.agents") })}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
          {t("connector.presetDesc")}
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-3 py-4 max-h-[65vh] overflow-y-auto pr-1">
        {#each PRESET_STACKS as stack}
          {@const IconComp = stack.iconComponent}
          <div class="rounded-2xl border p-4 bg-[var(--surface-1)] border-[var(--hairline)] flex flex-col justify-between gap-3 hover:border-[var(--brand)]/40 transition-colors">
            <div class="flex items-start justify-between gap-3">
              <div class="flex items-start gap-3">
                <div class="size-10 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/30 flex items-center justify-center text-[var(--brand-text)] shrink-0">
                  <IconComp class="size-5" />
                </div>
                <div>
                  <h4 class="font-bold text-sm text-[var(--text-primary)]">{stack.name}</h4>
                  <p class="text-xs text-[var(--text-tertiary)] mt-0.5">{stack.description}</p>
                </div>
              </div>

              <Button
                size="sm"
                class="h-8 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium shrink-0 cursor-pointer shadow-sm"
                disabled={applyingPreset || !currentBotId}
                onclick={() => applyPresetStack(stack)}
              >
                <span>{t("connector.applyStack")}</span>
                <ArrowRight class="size-3.5 ml-1" />
              </Button>
            </div>

            <!-- Connector Tags Included -->
            <div class="flex flex-wrap items-center gap-1.5 pt-2 border-t border-[var(--hairline)]">
              <span class="text-[10px] font-mono text-[var(--text-muted)] uppercase">{t("connector.includes")}</span>
              {#each stack.connectors as cid}
                <span class="text-[10px] bg-[var(--brand-soft)] border border-[var(--hairline)] text-[var(--brand-text)] px-2 py-0.5 rounded-md font-mono">
                  {cid}
                </span>
              {/each}
            </div>
          </div>
        {/each}
      </div>

      <div class="flex justify-end pt-3 border-t border-[var(--hairline)]">
        <Button variant="outline" class="h-8 text-xs border-[var(--hairline)] text-[var(--text-secondary)]" onclick={() => (showPresetModal = false)}>
          {t("ui.close")}
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 2: Assign Connector to Multiple Agents -->
{#if showAssignAgentsModal && selectedConnectorForAgentAssignment}
  <Dialog.Root open={showAssignAgentsModal} onOpenChange={(o) => !o && (showAssignAgentsModal = false)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[var(--surface-1)] border border-[var(--brand)]/30 rounded-xl p-6 text-[var(--text-primary)]">
      <Dialog.Header class="pb-3 border-b border-[var(--hairline)]">
        <Dialog.Title class="text-base font-bold text-[var(--text-primary)] flex items-center gap-2">
          <ConnectorIcon id={selectedConnectorForAgentAssignment.id} name={selectedConnectorForAgentAssignment.name} size="sm" />
          <span>{t("connector.assignTitle", { name: selectedConnectorForAgentAssignment.name })}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
          {t("connector.assignDesc")}
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
                ? "bg-[var(--brand-soft)] border-[var(--brand)]/60 text-[var(--text-primary)]"
                : "bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
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
                class="size-8 rounded-full object-cover border border-[var(--brand)]/40"
              />
              <div>
                <span class="font-bold text-xs text-[var(--text-primary)] block">{b.name}</span>
                <span class="text-[10px] text-[var(--text-tertiary)] font-mono">{b.model || t("connector.defaultModel")}</span>
              </div>
            </div>

            <div class={cn("size-5 rounded-md border flex items-center justify-center", isAssigned ? "bg-[var(--brand)] border-[var(--brand)] text-[var(--text-on-light)]" : "border-[var(--hairline-strong)]")}>
              {#if isAssigned}
                <Check class="size-3.5" />
              {/if}
            </div>
          </button>
        {/each}
      </div>

      <div class="flex items-center justify-between pt-3 border-t border-[var(--hairline)]">
        <Button
          size="sm"
          variant="ghost"
          class="h-8 text-xs text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
          onclick={() => {
            if (assignedAgentIds.size === bots.length) {
              assignedAgentIds = new Set();
            } else {
              assignedAgentIds = new Set(bots.map((b) => b.id));
            }
          }}
        >
          {assignedAgentIds.size === bots.length ? t("connector.deselectAll") : t("connector.selectAllAgents")}
        </Button>

        <div class="flex gap-2">
          <Button size="sm" variant="outline" class="h-8 text-xs border-[var(--hairline)]" onclick={() => (showAssignAgentsModal = false)}>
            {t("ui.cancel")}
          </Button>
          <Button
            size="sm"
            class="h-8 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium"
            disabled={isSavingAgentAssignment}
            onclick={saveAgentAssignment}
          >
            <span>{isSavingAgentAssignment ? t("ui.saving") : t("connector.saveAssignments")}</span>
          </Button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 3: Add Custom Connector -->
{#if showAddCustom}
  <Dialog.Root open={showAddCustom} onOpenChange={(o) => !o && (showAddCustom = false)}>
    <Dialog.Content class="sm:max-w-lg max-w-lg bg-[var(--surface-1)] border border-[var(--brand)]/30 rounded-xl p-6 text-[var(--text-primary)]">
      <Dialog.Header class="pb-3 border-b border-[var(--hairline)]">
        <Dialog.Title class="text-base font-bold text-[var(--text-primary)] flex items-center gap-2">
          <Plus class="size-4.5 text-[var(--brand-text)]" />
          <span>{t("connector.registerTitle")}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
          {t("connector.registerDesc")}
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-3.5 py-4 max-h-[60vh] overflow-y-auto pr-1">
        {#if customError}
          <div role="alert" class="p-2.5 rounded-xl bg-red-950/40 border border-red-500/40 text-red-300 text-xs font-mono">
            {customError}
          </div>
        {/if}

        <div class="grid grid-cols-3 gap-3">
          <div class="col-span-2 space-y-1.5">
            <Label for="cs-id" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblId")}</Label>
            <Input
              id="cs-id"
              bind:value={customId}
              placeholder={t("connector.phId")}
              class="h-8.5 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
            />
          </div>
          <div class="space-y-1.5">
            <Label for="cs-icon" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblIcon")}</Label>
            <Input
              id="cs-icon"
              bind:value={customIcon}
              placeholder="⚡"
              class="h-8.5 text-xs text-center bg-[var(--surface-2)] border-[var(--hairline)]"
            />
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <Label for="cs-name" class="text-xs font-bold text-[var(--text-primary)]">{t("settings.displayName")}</Label>
            <Input
              id="cs-name"
              bind:value={customName}
              placeholder={t("connector.phName")}
              class="h-8.5 text-xs bg-[var(--surface-2)] border-[var(--hairline)]"
            />
          </div>
          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblCategory")}</Label>
            <SimpleSelect
              value={customCategory}
              options={categoryDefs
                .filter((c) => c.id !== "All")
                .map((c) => ({ value: c.id, label: c.label || c.id }))}
              onValueChange={(v) => (customCategory = v)}
              class="h-8.5 rounded-xl"
            />
          </div>
        </div>

        <div class="space-y-1.5">
          <Label for="cs-desc" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblDesc")}</Label>
          <Textarea
            id="cs-desc"
            bind:value={customDesc}
            placeholder={t("connector.phDesc")}
            class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] min-h-[50px]"
          />
        </div>

        <div class="rounded-xl border border-warning/25 bg-warning/5 px-3 py-2 text-[11px] leading-relaxed text-warning/90">
          {t("connector.trustWarning")}
        </div>

        <div class="space-y-1.5">
          <Label class="text-xs font-bold text-[var(--text-primary)]">{t("connector.transportLabel")}</Label>
          <div role="radiogroup" aria-label={t("connector.transportLabel")} class="grid grid-cols-2 gap-1 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-1">
            {#each [["stdio", t("connector.transportStdio")], ["http", t("connector.transportHttp")]] as [mode, label] (mode)}
              <button
                type="button"
                role="radio"
                aria-checked={customTransport === mode}
                onclick={() => (customTransport = mode as "stdio" | "http")}
                class="rounded-lg px-2 py-1.5 text-[11px] font-semibold transition-colors cursor-pointer {customTransport === mode
                  ? 'bg-[var(--brand)] text-[var(--text-on-light)]'
                  : 'text-[var(--text-tertiary)] hover:bg-[var(--surface-3)]'}"
              >
                {label}
              </button>
            {/each}
          </div>
        </div>

        {#if customTransport === "http"}
          <div class="space-y-1.5">
            <Label for="cs-url" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblUrl")}</Label>
            <Input
              id="cs-url"
              bind:value={customUrl}
              placeholder={t("connector.phUrl")}
              class="h-8.5 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
            />
          </div>
          <div class="space-y-1.5">
            <Label for="cs-headers" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblHeaders")}</Label>
            <Textarea
              id="cs-headers"
              bind:value={customHeaders}
              placeholder={t("connector.phHeaders")}
              rows={3}
              class="text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
            />
            <p class="text-[10px] text-[var(--text-muted)]">{t("connector.headersHint")}</p>
          </div>
        {:else}
          <div class="grid grid-cols-3 gap-3">
            <div class="space-y-1.5">
              <Label for="cs-command" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblCommand")}</Label>
              <Input
                id="cs-command"
                bind:value={customCommand}
                placeholder="npx / python / uvx"
                class="h-8.5 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
              />
            </div>
            <div class="col-span-2 space-y-1.5">
              <Label for="cs-args" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblArgs")}</Label>
              <Textarea
                id="cs-args"
                bind:value={customArgs}
                placeholder={"-y\n@my-org/mcp-server\n--port 8000"}
                rows={4}
                class="text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
              />
            </div>
          </div>
        {/if}

        <div class="space-y-1.5">
          <Label for="cs-env" class="text-xs font-bold text-[var(--text-primary)]">{t("connector.lblEnv")}</Label>
          <Textarea
            id="cs-env"
            bind:value={customEnvKeys}
            placeholder={"API_KEY=sk-...\nDB_URL=postgres://..."}
            rows={3}
            class="text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
          />
          <p class="text-[10px] text-[var(--text-muted)]">{t("connector.envHint")}</p>
        </div>
      </div>

      <div class="flex justify-end gap-2 pt-3 border-t border-[var(--hairline)]">
        <Button size="sm" variant="outline" class="h-8 text-xs border-[var(--hairline)]" onclick={() => (showAddCustom = false)}>
          {t("ui.cancel")}
        </Button>
        <Button
          size="sm"
          class="h-8 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium"
          disabled={isSavingCustom}
          onclick={saveCustomServer}
        >
          <span>{isSavingCustom ? t("connector.registering") : t("connector.saveConnector")}</span>
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 4: Configure Credentials Drawer -->
{#if showConfigEnv && selectedServerForEnv}
  <Dialog.Root open={showConfigEnv} onOpenChange={(o) => !o && (showConfigEnv = false)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[var(--surface-1)] border border-[var(--brand)]/30 rounded-xl p-6 text-[var(--text-primary)]">
      <Dialog.Header class="pb-3 border-b border-[var(--hairline)]">
        <Dialog.Title class="text-base font-bold text-[var(--text-primary)] flex items-center gap-2">
          <Key class="size-4.5 text-warning" />
          <span>{t("connector.credTitle", { name: selectedServerForEnv.name })}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
          {t("connector.credDesc")}
        </Dialog.Description>
      </Dialog.Header>

      <p class="text-[11px] text-[var(--text-muted)]">{t("connector.secretWriteOnly")}</p>
      <div class="space-y-3.5 py-4 max-h-[50vh] overflow-y-auto">
        {#each selectedServerForEnv.env_keys as k}
          <div class="space-y-1.5">
            <Label class="text-xs font-bold text-[var(--text-secondary)] font-mono flex items-center justify-between">
              <span>{k}</span>
              <span class="text-[9px] text-[var(--text-muted)] uppercase">{t("connector.secretToken")}</span>
            </Label>
            <div class="relative">
              <Input
                type={showSecrets[k] ? "text" : "password"}
                aria-label={k}
                bind:value={envValues[k]}
                placeholder={selectedServerForEnv?.env_configured ? t("connector.savedKeep") : t("connector.enterKey", { key: k })}
                class="pr-9 h-8.5 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
              />
              <button
                type="button"
                class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
                aria-label={showSecrets[k] ? "Hide token" : "Show token"}
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

      <div class="flex justify-end gap-2 pt-3 border-t border-[var(--hairline)]">
        <Button size="sm" variant="outline" class="h-8 text-xs border-[var(--hairline)]" onclick={() => (showConfigEnv = false)}>
          {t("ui.cancel")}
        </Button>
        <Button
          size="sm"
          class={cn("h-8 text-xs font-medium", envSaveSuccess ? "bg-success text-[var(--text-on-light)]" : "bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)]")}
          disabled={isSavingEnv}
          onclick={saveEnvConfig}
        >
          {#if envSaveSuccess}
            <Check class="size-3.5 mr-1" />
            <span>{t("connector.savedOk")}</span>
          {:else}
            <span>{isSavingEnv ? t("ui.saving") : t("connector.saveCredentials")}</span>
          {/if}
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Modal 5: Live Test & Tool Inspector -->
{#if showTestModal && selectedServerForTest}
  <Dialog.Root open={showTestModal} onOpenChange={(o) => !o && (showTestModal = false)}>
    <Dialog.Content class="sm:max-w-xl max-w-xl bg-[var(--surface-1)] border border-[var(--brand)]/30 rounded-xl p-6 text-[var(--text-primary)]">
      <Dialog.Header class="pb-3 border-b border-[var(--hairline)]">
        <Dialog.Title class="text-base font-bold text-[var(--text-primary)] flex items-center gap-2">
          <Zap class="size-4.5 text-[var(--brand-text)]" />
          <span>{t("connector.diagTitle", { name: selectedServerForTest.name })}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
          {t("connector.diagDesc")}
        </Dialog.Description>
      </Dialog.Header>

      <div class="space-y-4 py-4 max-h-[55vh] overflow-y-auto">
        {#if isTesting}
          <div class="py-12 text-center space-y-2">
            <RefreshCw class="size-8 animate-spin text-[var(--brand-text)] mx-auto" />
            <p class="text-xs text-[var(--text-secondary)] font-semibold">{t("connector.testing")}</p>
          </div>
        {:else if testResult}
          <div class={cn("p-3.5 rounded-2xl border flex items-center justify-between", testResult.success ? "bg-success/30 border-success/40 text-success" : "bg-red-950/30 border-red-500/40 text-red-300")}>
            <div class="flex items-center gap-2.5">
              <span class="text-lg">{testResult.success ? "✅" : "❌"}</span>
              <div>
                <span class="font-bold text-xs block text-[var(--text-primary)]">{testResult.success ? t("connector.connOnline") : t("connector.connFailed")}</span>
                <span class="text-[11px] font-mono opacity-80">{testResult.message}</span>
              </div>
            </div>
            {#if testResult.success}
              <Badge variant="outline" class="bg-success/40 border-success/40 text-success font-mono text-[10px]">
                {t("connector.latency", { ms: testResult.latency_ms })}
              </Badge>
            {/if}
          </div>

          {#if testResult.tools.length > 0}
            <div class="space-y-2">
              <h5 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                <Code class="size-3.5 text-[var(--brand-text)]" />
                <span>{t("connector.discoveredTools", { n: testResult.tools.length })}</span>
              </h5>
              <div class="space-y-2">
                {#each testResult.tools as tool}
                  <div class="p-3 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] space-y-1">
                    <div class="flex items-center justify-between">
                      <span class="font-mono font-bold text-xs text-[var(--brand-text)]">{tool.name}</span>
                    </div>
                    <p class="text-[11px] text-[var(--text-tertiary)]">{tool.description}</p>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        {/if}
      </div>

      <div class="flex justify-end pt-3 border-t border-[var(--hairline)]">
        <Button size="sm" variant="outline" class="h-8 text-xs border-[var(--hairline)]" onclick={() => (showTestModal = false)}>
          {t("ui.close")}
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Custom Connector Delete Confirmation Modal -->
{#if serverToDelete}
  <Dialog.Root open={Boolean(serverToDelete)} onOpenChange={(o) => !o && (serverToDelete = null)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[var(--surface-1)] border border-red-500/40 rounded-xl p-6 text-[var(--text-primary)] flex flex-col gap-4 font-sans select-none z-50">
      <div class="flex items-start gap-3.5">
        <div class="size-11 rounded-2xl bg-red-950/70 border border-red-500/50 flex items-center justify-center text-red-400 shrink-0 ">
          <AlertCircle class="size-6" />
        </div>
        <div class="space-y-1">
          <Dialog.Title class="text-base font-extrabold text-[var(--text-primary)]">
            {t("connector.delTitle")}
          </Dialog.Title>
          <Dialog.Description class="text-xs text-[var(--text-tertiary)] leading-relaxed">
            {t("connector.delDesc", { name: serverToDelete.name })}
          </Dialog.Description>
        </div>
      </div>

      <div class="flex items-center justify-end gap-2.5 pt-2 border-t border-[var(--hairline)]">
        <Button
          variant="outline"
          class="h-8.5 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] cursor-pointer"
          onclick={() => (serverToDelete = null)}
          disabled={isDeletingServer}
        >
          {t("ui.cancel")}
        </Button>

        <Button
          class="h-8.5 text-xs bg-red-600 hover:bg-red-500 text-[var(--text-primary)] font-medium gap-1.5 cursor-pointer"
          disabled={isDeletingServer}
          onclick={confirmDeleteServer}
        >
          <Trash2 class="size-3.5" />
          <span>{isDeletingServer ? t("connector.deleting") : t("connector.menuDelete")}</span>
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}
