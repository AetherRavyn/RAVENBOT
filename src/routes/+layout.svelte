<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
  import ThemeBackground from "$lib/components/ThemeBackground.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ThreadView from "$lib/components/ThreadView.svelte";
  import CommandPalette from "$lib/components/CommandPalette.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import KillSwitch from "$lib/components/KillSwitch.svelte";
  import ScreenReader from "$lib/components/ScreenReader.svelte";
  import "../app.css";
  import { initI18n, t } from "$lib/i18n";
  import { prefersReducedMotion, keyboardShortcuts, announce } from "$lib/a11y";
  import { getStoredTheme, subscribeTheme, type ThemeDefinition } from "$lib/theme";
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import ChatRoomList from "$lib/components/ChatRoomList.svelte";
  import ChatRoomView from "$lib/components/ChatRoomView.svelte";
  import {
    Bot,
    Building2,
    Briefcase,
    Settings as SettingsIcon,
    Shield,
    ShieldCheck,
    Loader2,
    Layers,
    Lock,
    ArrowRight,
    BookOpen,
    Cpu,
    Zap,
    Flame,
    Sparkles,
    Terminal,
    ArrowUp,
    Globe,
    Brain,
    PanelLeft,
    Plus,
    Paperclip,
    Mic,
  } from "@lucide/svelte";

  let { children }: { children?: Snippet } = $props();

  let bots = $state<any[]>([]);
  let selectedBotId = $state<string | null>(null);
  let selectedRoomId = $state<string | null>(null);
  let activeTab = $state<"bots" | "offices">("bots");
  let settingsInitialTab = $state("keys");
  let loading = $state(true);
  let showCommandPalette = $state(false);
  let showSettings = $state(false);
  let killSwitchActive = $state(false);
  let srMessage = $state("");
  let chatrooms = $state<any[]>([]);
  let currentTheme = $state<ThemeDefinition>(getStoredTheme());

  // Grok Home & Sidebar Layout state
  let sidebarCollapsed = $state(false);
  let homePrompt = $state("");
  let homeDeepSearch = $state(false);
  let homeThink = $state(false);
  let homeSelectedBotId = $state<string | null>(null);
  let homeSending = $state(false);

  // Initialize i18n
  initI18n();

  function startNewChat() {
    selectedBotId = null;
    selectedRoomId = null;
    homePrompt = "";
    announce("New Chat Home");
  }

  async function sendHomePrompt(presetPrompt?: string) {
    const rawText = (presetPrompt || homePrompt).trim();
    if (!rawText || homeSending) return;

    homeSending = true;
    let targetBot = bots.find((b) => b.id === homeSelectedBotId) || bots[0];

    try {
      if (!targetBot) {
        targetBot = await invoke("create_bot", {
          name: "Raven Prime",
          description: "Primary Sovereign Fleet Assistant",
          avatarUrl: "/ravenicon.png",
          avatarStyle: "bottts",
        });
        bots = [...bots, targetBot];
      }

      let text = rawText;
      if (homeDeepSearch && !text.startsWith("[DeepSearch]")) text = `[DeepSearch] ${text}`;
      if (homeThink && !text.startsWith("[Think]")) text = `[Think] ${text}`;

      const thread: any = await invoke("create_thread", {
        botId: targetBot.id,
        title: rawText.slice(0, 35) + (rawText.length > 35 ? "..." : ""),
      });

      await invoke("send_message", {
        threadId: thread.id,
        content: text,
      });

      selectedBotId = targetBot.id;
      selectedRoomId = null;
      homePrompt = "";
    } catch (e) {
      console.error("Home prompt dispatch error:", e);
    } finally {
      homeSending = false;
    }
  }

  let homeListening = $state(false);
  let homeRecognition: any = null;

  function attachHomeFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".txt,.md,.rs,.ts,.js,.py,.json,.toml,.yaml,.yml,.css,.html,.sh";
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (file) {
        const text = await file.text();
        const ext = file.name.split(".").pop() || "text";
        homePrompt = (homePrompt ? homePrompt + "\n\n" : "") + `Attached file [${file.name}]:\n\`\`\`${ext}\n${text}\n\`\`\`\n`;
      }
    };
    input.click();
  }

  function toggleHomeVoice() {
    const SpeechRecognition = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    if (!SpeechRecognition) {
      alert("Speech recognition is not supported in this environment. You can type directly in the composer.");
      return;
    }
    if (homeListening) {
      homeRecognition?.stop();
      homeListening = false;
      return;
    }
    try {
      homeRecognition = new SpeechRecognition();
      homeRecognition.continuous = false;
      homeRecognition.interimResults = true;
      homeRecognition.lang = "en-US";
      homeRecognition.onstart = () => { homeListening = true; };
      homeRecognition.onresult = (event: any) => {
        let text = "";
        for (let i = event.resultIndex; i < event.results.length; ++i) {
          text += event.results[i][0].transcript;
        }
        if (text) {
          homePrompt = (homePrompt ? homePrompt + " " : "") + text.trim();
        }
      };
      homeRecognition.onerror = () => { homeListening = false; };
      homeRecognition.onend = () => { homeListening = false; };
      homeRecognition.start();
    } catch {
      homeListening = false;
    }
  }

  onMount(() => {
    const unsubTheme = subscribeTheme((theme) => {
      currentTheme = theme;
    });

    (async () => {
      try {
        bots = await invoke("list_bots");
        if (bots.length > 0 && !homeSelectedBotId) {
          homeSelectedBotId = bots[0].id;
        }
        try {
          chatrooms = await invoke("list_chatrooms");
        } catch {}
        const status = await invoke("get_status");
        killSwitchActive = (status as any).kill_switch_active;
      } catch (e) {
        console.error("Failed to load data:", e);
        srMessage = t("errors.loadFailed");
      }
      loading = false;
    })();

    // Register keyboard shortcuts
    const unsubK = keyboardShortcuts.register("mod+k", () => {
      showCommandPalette = !showCommandPalette;
      announce(showCommandPalette ? t("commandPalette.placeholder") : "");
    });

    const unsubComma = keyboardShortcuts.register("mod+,", () => {
      showSettings = !showSettings;
      announce(showSettings ? t("settings.title") : "");
    });

    const unsubB = keyboardShortcuts.register("mod+b", () => {
      sidebarCollapsed = !sidebarCollapsed;
    });

    const unsubN = keyboardShortcuts.register("mod+n", () => {
      startNewChat();
    });

    const unsubEsc = keyboardShortcuts.register("escape", () => {
      if (showCommandPalette) {
        showCommandPalette = false;
        announce("");
      } else if (showSettings) {
        showSettings = false;
        announce("");
      }
    });

    if (prefersReducedMotion()) {
      document.documentElement.classList.add("reduce-motion");
    }

    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handler = (e: MediaQueryListEvent) => {
      document.documentElement.classList.toggle("reduce-motion", e.matches);
    };
    mediaQuery.addEventListener("change", handler);

    const handleOpenSettings = () => {
      settingsInitialTab = "keys";
      showSettings = true;
    };
    window.addEventListener("open-settings", handleOpenSettings);

    const handleOpenConnectors = () => {
      settingsInitialTab = "mcp";
      showSettings = true;
    };
    window.addEventListener("open-connectors", handleOpenConnectors);

    const handleOfficeDeleted = (e: Event) => {
      const custom = e as CustomEvent;
      const roomId = custom.detail?.roomId;
      if (roomId) {
        chatrooms = chatrooms.filter((r: any) => r.id !== roomId);
        if (selectedRoomId === roomId) {
          if (chatrooms.length > 0) {
            selectedRoomId = chatrooms[0].id;
          } else {
            selectedRoomId = null;
            if (bots.length > 0) selectedBotId = bots[0].id;
          }
        }
      }
    };
    window.addEventListener("office-deleted", handleOfficeDeleted);

    const handleOfficeUpdated = (e: Event) => {
      const custom = e as CustomEvent;
      const updated = custom.detail?.room;
      if (updated) {
        chatrooms = chatrooms.map((r: any) => (r.id === updated.id ? updated : r));
      }
    };
    window.addEventListener("office-updated", handleOfficeUpdated);

    return () => {
      unsubTheme();
      unsubK();
      unsubComma();
      unsubB();
      unsubN();
      unsubEsc();
      window.removeEventListener("open-settings", handleOpenSettings);
      window.removeEventListener("open-connectors", handleOpenConnectors);
      window.removeEventListener("office-deleted", handleOfficeDeleted);
      window.removeEventListener("office-updated", handleOfficeUpdated);
      mediaQuery.removeEventListener("change", handler);
    };
  });

  let selectedBot = $derived(bots.find((b) => b.id === selectedBotId));
  let selectedRoom = $derived(chatrooms.find((r: any) => r.id === selectedRoomId));

  function handleBotCreated(bot: any) {
    bots = [...bots, bot];
    selectedBotId = bot.id;
    selectedRoomId = null;
    srMessage = currentTheme.brand.brandTitle + " " + bot.name + " created";
  }

  function handleBotUpdated(updatedBot: any) {
    bots = bots.map((b) => (b.id === updatedBot.id ? updatedBot : b));
    if (selectedBotId === updatedBot.id) {
      selectedBotId = selectedBotId;
    }
    srMessage = updatedBot.name + " updated";
  }

  function handleBotDeleted(botId: string) {
    const bot = bots.find((b) => b.id === botId);
    bots = bots.filter((b) => b.id !== botId);
    if (selectedBotId === botId) {
      selectedBotId = bots.length > 0 ? bots[0].id : null;
    }
    srMessage = (bot?.name || "Agent") + " deleted";
  }

  function openSettings() {
    showSettings = true;
  }
</script>

<svelte:head>
  <title>{currentTheme.brand.brandTitle}{currentTheme.brand.brandAccent} — {currentTheme.brand.subtitle}</title>
  <meta name="description" content={currentTheme.brand.tagline} />
</svelte:head>

<div
  class="flex flex-col h-screen w-screen overflow-hidden select-none font-sans transition-colors duration-300"
  style="background-color: {currentTheme.bgHex}; color: #f4f4f5;"
  class:reduce-motion={prefersReducedMotion()}
  role="application"
  aria-label={currentTheme.brand.brandTitle}
>
  <!-- Top Custom TitleBar with Native Window Controls (Minimize, Maximize, Close) -->
  <TitleBar
    {sidebarCollapsed}
    onToggleSidebar={() => (sidebarCollapsed = !sidebarCollapsed)}
  />

  <!-- Screen Reader Announcements -->
  <ScreenReader message={srMessage} />

  <!-- Main Body Content Area -->
  <div class="flex-1 flex overflow-hidden relative">
    <!-- Left Sidebar Panel -->
    <div
      class="{sidebarCollapsed ? 'hidden' : 'w-72 shrink-0 flex flex-col border-r overflow-hidden z-20 transition-all duration-300'}"
      style="background-color: {currentTheme.cardHex}; border-color: {currentTheme.borderHex};"
    >
      <!-- Top Branding Row with Theme-specific Logo & Identity -->
      <div class="p-3.5 border-b flex items-center justify-between transition-colors duration-300" style="border-color: {currentTheme.borderHex};">
        <div class="flex items-center gap-3 min-w-0">
          <!-- Dynamic Theme Logo Icon -->
          <div class="size-10 flex items-center justify-center shrink-0">
            <ThemeLogo theme={currentTheme} size="md" class="!size-10" />
          </div>

          <div class="flex flex-col min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-black text-sm tracking-wider text-white truncate">
                {currentTheme.brand.brandTitle}<span style="color: {currentTheme.primaryColor}">{currentTheme.brand.brandAccent}</span>
              </span>
            </div>
            <div class="flex items-center gap-1.5 mt-0.5">
              <span
                class="text-[9px] font-bold px-1.5 py-0.2 rounded tracking-widest font-mono border truncate"
                style="color: {currentTheme.accentColor}; background-color: {currentTheme.primaryColor}20; border-color: {currentTheme.borderHex};"
              >
                {currentTheme.brand.badgeLabel}
              </span>
            </div>
            <span class="text-[10px] text-zinc-500 mt-0.5 truncate">
              {currentTheme.brand.subtitle}
            </span>
          </div>
        </div>

        <!-- Quick Command Shortcut Badge -->
        <button
          type="button"
          class="px-2 py-0.5 rounded-lg border border-white/10 bg-white/5 text-[11px] font-mono text-zinc-400 hover:text-zinc-200 hover:border-zinc-500 transition-colors cursor-pointer shrink-0 ml-1"
          onclick={() => (showCommandPalette = true)}
          title="Open Command Palette (⌘K)"
        >
          ⌘K
        </button>
      </div>

      <!-- Switcher Tabs: Agents / Offices -->
      <div class="p-3 pb-1">
        <div class="p-1 rounded-xl flex gap-1 border bg-black/20" style="border-color: {currentTheme.borderHex};">
          <button
            type="button"
            class="flex items-center justify-center gap-1.5 flex-1 py-1.5 px-3 rounded-lg text-xs font-medium transition-all cursor-pointer {activeTab === 'bots'
              ? 'bg-white/15 border border-white/20 text-white shadow-sm font-bold'
              : 'text-zinc-400 hover:text-zinc-200'}"
            onclick={() => (activeTab = "bots")}
          >
            <Briefcase class="size-3.5" />
            <span>Agents ({bots.length})</span>
          </button>

          <button
            type="button"
            class="flex items-center justify-center gap-1.5 flex-1 py-1.5 px-3 rounded-lg text-xs font-medium transition-all cursor-pointer {activeTab === 'offices'
              ? 'bg-white/15 border border-white/20 text-white shadow-sm font-bold'
              : 'text-zinc-400 hover:text-zinc-200'}"
            onclick={() => (activeTab = "offices")}
          >
            <Building2 class="size-3.5" />
            <span>Offices ({chatrooms.length})</span>
          </button>
        </div>
      </div>

      <!-- Tab Contents -->
      <div class="flex-1 overflow-hidden">
        {#if activeTab === "bots"}
          <Sidebar
            {bots}
            {selectedBotId}
            onSelectBot={(id) => {
              selectedBotId = id;
              selectedRoomId = null;
              const bot = bots.find((b) => b.id === id);
              announce(bot?.name || "");
            }}
            onBotCreated={handleBotCreated}
            onBotUpdated={handleBotUpdated}
            onBotDeleted={handleBotDeleted}
            {openSettings}
            onNewChat={startNewChat}
          />
        {:else}
          <ChatRoomList
            {bots}
            selectedRoomId={selectedRoomId}
            onSelectRoom={(id) => {
              selectedRoomId = id;
              selectedBotId = null;
              const room = chatrooms.find((r: any) => r.id === id);
              announce(room?.name || "");
            }}
            onRoomCreated={(room) => {
              chatrooms = [...chatrooms, room];
              selectedRoomId = room.id;
              selectedBotId = null;
            }}
          />
        {/if}
      </div>
    </div>

    <!-- Main Workspace Area -->
    <main
      class="flex-1 flex flex-col overflow-hidden relative transition-colors duration-300"
      style="background-color: {currentTheme.bgHex};"
      aria-label={t("a11y.thread")}
    >
      {@render children?.()}

      <!-- Global Top Kill Switch Ribbon when active -->
      {#if killSwitchActive}
        <div class="p-2 px-4 bg-red-950/40 border-b border-red-900/50 flex items-center justify-center z-30" role="alert">
          <KillSwitch />
        </div>
      {/if}

      {#if loading}
        <div class="flex-1 flex flex-col items-center justify-center gap-3 p-8 text-zinc-500" role="status">
          <div
            class="size-12 rounded-2xl flex items-center justify-center ring-4 animate-pulse border"
            style="background-color: {currentTheme.primaryColor}20; color: {currentTheme.accentColor}; border-color: {currentTheme.primaryColor}40;"
          >
            <Loader2 class="size-6 animate-spin" />
          </div>
          <div class="text-center space-y-0.5">
            <p class="text-sm font-semibold text-zinc-200">
              Initializing {currentTheme.brand.brandTitle} Runtime
            </p>
            <p class="text-xs text-zinc-500">Connecting local engine and database...</p>
          </div>
        </div>
      {:else if selectedRoom}
        <ChatRoomView room={selectedRoom} {bots} />
      {:else if selectedBot}
        <ThreadView bot={selectedBot} />
      {:else}
        <!-- Grok Sovereign AI Welcome Hub -->
        <div class="flex-1 relative overflow-y-auto flex flex-col justify-between p-6 sm:p-10 select-none">
          <!-- Background Ambient Artwork / Horizon Glow -->
          <ThemeBackground theme={currentTheme} />

          <!-- Top Status Strip -->
          <div class="flex items-center justify-between relative z-10 w-full max-w-4xl mx-auto">
            <div class="flex items-center gap-2">
              <span class="size-2 rounded-full bg-emerald-400 shadow-[0_0_8px_#34d399]"></span>
              <span class="text-[11px] font-mono text-zinc-400 uppercase tracking-wider">
                Sovereign Enclave Active
              </span>
            </div>

            <div class="flex items-center gap-2">
              <span class="text-[11px] font-mono text-zinc-500">
                Local-First • Zero Telemetry
              </span>
            </div>
          </div>

          <!-- Central Grok Composer & Prompt Hub -->
          <div class="max-w-2xl mx-auto w-full my-auto space-y-6 relative z-10 py-8">
            <!-- Dynamic Theme Emblem & Headline -->
            <div class="text-center space-y-3">
              <div class="flex justify-center">
                <div class="p-1.5 rounded-2xl bg-white/5 border border-white/10 shadow-2xl backdrop-blur-md">
                  <ThemeLogo theme={currentTheme} size="lg" class="!size-16 sm:!size-20" />
                </div>
              </div>

              <div class="space-y-1">
                <h1 class="text-2xl sm:text-4xl font-black text-white tracking-tight">
                  What's on your mind today?
                </h1>
                <p class="text-xs sm:text-sm text-zinc-400">
                  {currentTheme.brand.subtitle} — {currentTheme.brand.tagline}
                </p>
              </div>
            </div>

            <!-- Grok Signature Floating Capsule Composer -->
            <div class="rounded-3xl border border-zinc-800 bg-[#0c0c11]/95 backdrop-blur-2xl p-3.5 shadow-2xl focus-within:border-zinc-500 transition-all">
              <textarea
                bind:value={homePrompt}
                placeholder="Ask anything, analyze repository, run code, or delegate tasks..."
                rows={2}
                class="w-full bg-transparent text-sm text-white placeholder:text-zinc-500 resize-none focus:outline-none min-h-[52px] leading-relaxed font-sans"
                onkeydown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    sendHomePrompt();
                  }
                }}
              ></textarea>

              <!-- Toolbar inside Home Capsule -->
              <div class="flex items-center justify-between pt-3 border-t border-white/5 mt-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <!-- DeepSearch Toggle Pill -->
                  <button
                    type="button"
                    class="h-7 px-2.5 rounded-full border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {homeDeepSearch
                      ? 'bg-sky-500/20 text-sky-300 border-sky-500/50 shadow-[0_0_10px_rgba(56,189,248,0.25)]'
                      : 'border-white/10 text-zinc-400 hover:text-zinc-200 hover:bg-white/5'}"
                    onclick={() => (homeDeepSearch = !homeDeepSearch)}
                    title="Toggle DeepSearch Web Intelligence"
                  >
                    <Globe class="size-3" />
                    <span>DeepSearch</span>
                  </button>

                  <!-- Think Mode Toggle Pill -->
                  <button
                    type="button"
                    class="h-7 px-2.5 rounded-full border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {homeThink
                      ? 'bg-indigo-500/20 text-indigo-300 border-indigo-500/50 shadow-[0_0_10px_rgba(99,102,241,0.25)]'
                      : 'border-white/10 text-zinc-400 hover:text-zinc-200 hover:bg-white/5'}"
                    onclick={() => (homeThink = !homeThink)}
                    title="Toggle Deep Reasoning Mode"
                  >
                    <Brain class="size-3" />
                    <span>Think</span>
                  </button>

                  <!-- Target Agent Picker (if bots exist) -->
                  {#if bots.length > 0}
                    <select
                      bind:value={homeSelectedBotId}
                      class="h-7 text-[11px] font-mono bg-[#14141d] border border-white/10 text-zinc-300 rounded-full px-2.5 py-0 focus:outline-none cursor-pointer"
                    >
                      {#each bots as bot}
                        <option value={bot.id}>{bot.name}</option>
                      {/each}
                    </select>
                  {/if}

                  <!-- Attach File Button -->
                  <button
                    type="button"
                    class="size-7 rounded-full text-zinc-400 hover:text-zinc-200 hover:bg-white/5 flex items-center justify-center transition-colors cursor-pointer"
                    onclick={attachHomeFile}
                    title="Attach workspace code or text file"
                  >
                    <Paperclip class="size-3.5" />
                  </button>

                  <!-- Voice STT Button -->
                  <button
                    type="button"
                    class="size-7 rounded-full flex items-center justify-center transition-all cursor-pointer {homeListening ? 'text-rose-400 bg-rose-500/20 border border-rose-500/50 animate-pulse shadow-sm' : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5'}"
                    onclick={toggleHomeVoice}
                    title={homeListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
                  >
                    <Mic class="size-3.5" />
                  </button>
                </div>

                <!-- Circular Grok Send Button -->
                <button
                  type="button"
                  onclick={() => sendHomePrompt()}
                  disabled={!homePrompt.trim() || homeSending}
                  class="size-8 rounded-full flex items-center justify-center transition-all cursor-pointer {homePrompt.trim() && !homeSending
                    ? 'bg-white text-black hover:bg-zinc-200 shadow-md scale-105'
                    : 'bg-zinc-800 text-zinc-500 cursor-not-allowed opacity-50'}"
                  title="Dispatch prompt (Enter)"
                >
                  {#if homeSending}
                    <Loader2 class="size-3.5 animate-spin" />
                  {:else}
                    <ArrowUp class="size-4 stroke-[2.5]" />
                  {/if}
                </button>
              </div>
            </div>

            <!-- Grok Prompt Suggestion Chips Grid -->
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 text-left">
              <button
                type="button"
                class="p-3 rounded-2xl border border-white/5 bg-[#0e0e13]/80 hover:bg-[#15151e] hover:border-white/20 transition-all cursor-pointer text-left group"
                onclick={() => sendHomePrompt("Analyze this codebase repository and identify optimization points")}
              >
                <div class="flex items-center gap-2 text-xs font-bold text-white group-hover:text-sky-300">
                  <Terminal class="size-3.5 text-sky-400" />
                  <span>Analyze Codebase</span>
                </div>
                <p class="text-[11px] text-zinc-400 mt-1">Inspect repo architecture, performance & bottlenecks</p>
              </button>

              <button
                type="button"
                class="p-3 rounded-2xl border border-white/5 bg-[#0e0e13]/80 hover:bg-[#15151e] hover:border-white/20 transition-all cursor-pointer text-left group"
                onclick={() => sendHomePrompt("Draft a parallel multi-agent execution strategy")}
              >
                <div class="flex items-center gap-2 text-xs font-bold text-white group-hover:text-sky-300">
                  <Brain class="size-3.5 text-sky-400" />
                  <span>Multi-Agent Strategy</span>
                </div>
                <p class="text-[11px] text-zinc-400 mt-1">Orchestrate task graph across persistent specialist fleet</p>
              </button>

              <button
                type="button"
                class="p-3 rounded-2xl border border-white/5 bg-[#0e0e13]/80 hover:bg-[#15151e] hover:border-white/20 transition-all cursor-pointer text-left group"
                onclick={() => sendHomePrompt("Audit system security, local sandboxes and resource quotas")}
              >
                <div class="flex items-center gap-2 text-xs font-bold text-white group-hover:text-sky-300">
                  <Shield class="size-3.5 text-sky-400" />
                  <span>Security & Sandbox Audit</span>
                </div>
                <p class="text-[11px] text-zinc-400 mt-1">Verify zero-telemetry hardware enclave isolation</p>
              </button>

              <button
                type="button"
                class="p-3 rounded-2xl border border-white/5 bg-[#0e0e13]/80 hover:bg-[#15151e] hover:border-white/20 transition-all cursor-pointer text-left group"
                onclick={() => sendHomePrompt("DeepSearch technical documentation and APIs")}
              >
                <div class="flex items-center gap-2 text-xs font-bold text-white group-hover:text-sky-300">
                  <Globe class="size-3.5 text-sky-400" />
                  <span>DeepSearch & RAG</span>
                </div>
                <p class="text-[11px] text-zinc-400 mt-1">Search web knowledge & query local vector embeddings</p>
              </button>
            </div>
          </div>

          <!-- Bottom Actions & Fleet Safety Protocol Card -->
          <div class="max-w-4xl mx-auto w-full space-y-3 relative z-10 pt-4">
            <div class="w-full p-3.5 rounded-2xl border border-white/10 bg-[#0e0e13]/80 flex items-center justify-between shadow-xl">
              <div class="flex items-center gap-3">
                <button
                  type="button"
                  class="flex items-center gap-1.5 text-zinc-400 hover:text-white px-2.5 py-1 rounded-lg bg-white/5 border border-white/10 text-xs transition-colors cursor-pointer"
                  onclick={() => (showCommandPalette = true)}
                >
                  <span class="font-mono text-[10px] text-zinc-500">⌘K</span>
                  <span>Command Palette</span>
                </button>

                <button
                  type="button"
                  class="flex items-center gap-1.5 text-zinc-400 hover:text-white px-2.5 py-1 rounded-lg bg-white/5 border border-white/10 text-xs transition-colors cursor-pointer"
                  onclick={openSettings}
                >
                  <span class="font-mono text-[10px] text-zinc-500">⌘,</span>
                  <span>Settings & Keys</span>
                </button>
              </div>

              <!-- Emergency Stop / KillSwitch -->
              <KillSwitch />
            </div>
          </div>
        </div>
      {/if}
    </main>
  </div>
</div>

<!-- Command Palette Modal -->
<CommandPalette
  open={showCommandPalette}
  onClose={() => {
    showCommandPalette = false;
    announce("");
  }}
  {bots}
  onSelectBot={(id) => {
    selectedBotId = id;
    selectedRoomId = null;
    showCommandPalette = false;
    const bot = bots.find((b) => b.id === id);
    announce(bot?.name || "");
  }}
  onCreateBot={() => {
    showCommandPalette = false;
    activeTab = "bots";
  }}
  onOpenSettings={() => {
    showCommandPalette = false;
    showSettings = true;
  }}
/>

<!-- Settings Modal -->
<Settings
  open={showSettings}
  onClose={() => {
    showSettings = false;
    announce("");
  }}
  {bots}
  initialTab={settingsInitialTab}
/>
