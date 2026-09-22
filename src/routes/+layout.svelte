<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ThreadView from "$lib/components/ThreadView.svelte";
  import CommandPalette from "$lib/components/CommandPalette.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import KillSwitch from "$lib/components/KillSwitch.svelte";
  import ScreenReader from "$lib/components/ScreenReader.svelte";
  import Toaster from "$lib/components/Toaster.svelte";
  import "../app.css";
  import { initI18n, t } from "$lib/i18n";
  import { prefersReducedMotion, keyboardShortcuts, announce } from "$lib/a11y";
  import { getStoredTheme, applyTheme, subscribeTheme, type ThemeDefinition } from "$lib/theme";
  import { onMount } from "svelte";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
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
    // Apply the saved theme's palette to the design tokens before first paint
    // (buttons, borders, focus rings and surfaces all follow the theme).
    applyTheme(getStoredTheme().id);

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

    // Bots may be created outside the sidebar (e.g. an office auto-hires its
    // team). Reload the roster so new agents appear everywhere.
    const handleBotsChanged = async () => {
      try {
        bots = await invoke("list_bots");
      } catch (e) {
        console.error("Failed to reload bots:", e);
      }
    };
    window.addEventListener("bots-changed", handleBotsChanged);

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
      window.removeEventListener("bots-changed", handleBotsChanged);
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
  class="flex flex-col h-screen w-screen overflow-hidden select-none font-sans bg-[var(--surface-0)] text-[var(--text-primary)]"
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

  <!-- Global transient notifications -->
  <Toaster />

  <!-- Main Body Content Area -->
  <div class="flex-1 flex overflow-hidden relative">
    <!-- Left Sidebar Panel -->
    <div
      class="{sidebarCollapsed ? 'hidden' : 'w-[280px] shrink-0 flex flex-col border-r border-[var(--hairline)] overflow-hidden z-20 transition-all duration-300 bg-[var(--surface-1)]'}"
    >
      <!-- Sidebar topbar — 38px, flat neutral chrome (OpenBot) -->
      <div class="h-[38px] shrink-0 px-3 flex items-center justify-between border-b border-[var(--hairline)]">
        <div class="flex items-center gap-2 min-w-0">
          <ThemeLogo theme={currentTheme} size="sm" class="!size-[18px] shrink-0" />
          <span class="text-xs font-semibold tracking-tight text-[var(--text-primary)] truncate">
            {currentTheme.brand.brandTitle}<span class="text-[var(--brand-text)]">{currentTheme.brand.brandAccent}</span>
          </span>
          <span class="text-[9px] font-mono uppercase px-1.5 py-px rounded border border-[var(--hairline)] text-[var(--text-muted)] truncate">
            {currentTheme.brand.badgeLabel}
          </span>
        </div>

        <!-- Quick Command Shortcut -->
        <button
          type="button"
          class="h-5 px-1.5 rounded border border-[var(--hairline)] bg-[var(--surface-2)] text-[10px] font-mono text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer shrink-0"
          onclick={() => (showCommandPalette = true)}
          title="Open Command Palette (⌘K)"
        >
          ⌘K
        </button>
      </div>

      <!-- Switcher Tabs: Agents / Offices — neutral segmented control -->
      <div class="p-2.5 pb-1.5">
        <div class="p-0.5 rounded-lg flex gap-0.5 border border-[var(--hairline)] bg-[var(--surface-0)]">
          <button
            type="button"
            class="flex items-center justify-center gap-1.5 flex-1 py-1.5 px-3 rounded-md text-xs transition-all cursor-pointer {activeTab === 'bots'
              ? 'bg-[var(--surface-3)] text-[var(--text-primary)] font-semibold'
              : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
            onclick={() => (activeTab = "bots")}
          >
            <Briefcase class="size-3.5" />
            <span>Agents ({bots.length})</span>
          </button>

          <button
            type="button"
            class="flex items-center justify-center gap-1.5 flex-1 py-1.5 px-3 rounded-md text-xs transition-all cursor-pointer {activeTab === 'offices'
              ? 'bg-[var(--surface-3)] text-[var(--text-primary)] font-semibold'
              : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
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
      class="flex-1 flex flex-col overflow-hidden relative bg-[var(--surface-0)]"
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
        <div class="flex-1 flex flex-col items-center justify-center gap-3 p-8 text-[var(--text-muted)]" role="status">
          <div
            class="size-12 rounded-2xl flex items-center justify-center ring-4 animate-pulse border"
            style="background-color: {currentTheme.primaryColor}20; color: {currentTheme.accentColor}; border-color: {currentTheme.primaryColor}40;"
          >
            <Loader2 class="size-6 animate-spin" />
          </div>
          <div class="text-center space-y-0.5">
            <p class="text-sm font-semibold text-[var(--text-primary)]">
              Initializing {currentTheme.brand.brandTitle} Runtime
            </p>
            <p class="text-xs text-[var(--text-muted)]">Connecting local engine and database...</p>
          </div>
        </div>
      {:else if selectedRoom}
        <ChatRoomView room={selectedRoom} {bots} />
      {:else if selectedBot}
        <ThreadView bot={selectedBot} />
      {:else}
        <!-- Flat neutral welcome hub (OpenBot) -->
        <div class="flex-1 relative overflow-y-auto flex flex-col justify-between p-6 sm:p-10 select-none">

          <!-- Top Status Strip -->
          <div class="flex items-center justify-between relative z-10 w-full max-w-4xl mx-auto">
            <div class="flex items-center gap-2">
              <span class="size-2 rounded-full bg-[var(--status-success)]"></span>
              <span class="text-xs uppercase tracking-wider text-[var(--text-tertiary)]">
                Sovereign Enclave Active
              </span>
            </div>

            <div class="flex items-center gap-2">
              <span class="text-xs text-[var(--text-muted)]">
                Local-First • Zero Telemetry
              </span>
            </div>
          </div>

          <!-- Central Grok Composer & Prompt Hub -->
          <div class="max-w-2xl mx-auto w-full my-auto space-y-6 relative z-10 py-8">
            <!-- Emblem & Headline -->
            <div class="text-center space-y-3">
              <div class="flex justify-center">
                <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2">
                  <ThemeLogo theme={currentTheme} size="lg" class="!size-16 sm:!size-20" />
                </div>
              </div>

              <div class="space-y-1">
                <h1 class="text-2xl font-semibold text-[var(--text-primary)] tracking-tight">
                  What's on your mind today?
                </h1>
                <p class="text-sm text-[var(--text-tertiary)]">
                  {currentTheme.brand.subtitle} — {currentTheme.brand.tagline}
                </p>
              </div>
            </div>

            <!-- Composer card (OpenBot: #212121 fill, hairline border, r12) -->
            <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3.5 shadow-sm focus-within:border-[var(--brand)] transition-colors">
              <textarea
                bind:value={homePrompt}
                placeholder="Ask anything, analyze repository, run code, or delegate tasks..."
                rows={2}
                class="w-full bg-transparent text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none min-h-[52px] leading-relaxed font-sans"
                onkeydown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    sendHomePrompt();
                  }
                }}
              ></textarea>

              <!-- Toolbar inside Home Composer -->
              <div class="flex items-center justify-between pt-3 border-t border-[var(--hairline)] mt-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <!-- DeepSearch Toggle -->
                  <button
                    type="button"
                    class="h-7 px-2.5 rounded-md border text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer {homeDeepSearch
                      ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]'
                      : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                    onclick={() => (homeDeepSearch = !homeDeepSearch)}
                    title="Toggle DeepSearch Web Intelligence"
                  >
                    <Globe class="size-3" />
                    <span>DeepSearch</span>
                  </button>

                  <!-- Think Mode Toggle -->
                  <button
                    type="button"
                    class="h-7 px-2.5 rounded-md border text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer {homeThink
                      ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]'
                      : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                    onclick={() => (homeThink = !homeThink)}
                    title="Toggle Deep Reasoning Mode"
                  >
                    <Brain class="size-3" />
                    <span>Think</span>
                  </button>

                  <!-- Target Agent Picker (if bots exist) -->
                  {#if bots.length > 0}
                    <SimpleSelect
                      value={homeSelectedBotId ?? ""}
                      options={bots.map((bot: any) => ({ value: bot.id, label: bot.name }))}
                      onValueChange={(v) => (homeSelectedBotId = v || null)}
                      class="h-7 w-40 rounded-md text-xs"
                    />
                  {/if}

                  <!-- Attach File Button -->
                  <button
                    type="button"
                    class="size-7 rounded-md text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center transition-colors cursor-pointer"
                    onclick={attachHomeFile}
                    title="Attach workspace code or text file"
                  >
                    <Paperclip class="size-3.5" />
                  </button>

                  <!-- Voice STT Button -->
                  <button
                    type="button"
                    class="size-7 rounded-md flex items-center justify-center transition-all cursor-pointer {homeListening ? 'text-[var(--status-danger)] bg-[var(--brand-soft)] animate-pulse' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                    onclick={toggleHomeVoice}
                    title={homeListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
                  >
                    <Mic class="size-3.5" />
                  </button>
                </div>

                <!-- Send Button (light primary, OpenBot) -->
                <button
                  type="button"
                  onclick={() => sendHomePrompt()}
                  disabled={!homePrompt.trim() || homeSending}
                  class="size-8 rounded-md flex items-center justify-center transition-all cursor-pointer {homePrompt.trim() && !homeSending
                    ? 'bg-[var(--surface-light)] text-[var(--text-on-light)] hover:bg-white shadow-sm'
                    : 'bg-[var(--surface-3)] text-[var(--text-muted)] cursor-not-allowed opacity-60'}"
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
                class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer text-left group"
                onclick={() => sendHomePrompt("Analyze this codebase repository and identify optimization points")}
              >
                <div class="flex items-center gap-2 text-xs font-semibold text-[var(--text-primary)] group-hover:text-[var(--brand-text)]">
                  <Terminal class="size-3.5 text-[var(--brand-text)]" />
                  <span>Analyze Codebase</span>
                </div>
                <p class="text-[11px] text-[var(--text-muted)] mt-1">Inspect repo architecture, performance & bottlenecks</p>
              </button>

              <button
                type="button"
                class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer text-left group"
                onclick={() => sendHomePrompt("Draft a parallel multi-agent execution strategy")}
              >
                <div class="flex items-center gap-2 text-xs font-semibold text-[var(--text-primary)] group-hover:text-[var(--brand-text)]">
                  <Brain class="size-3.5 text-[var(--brand-text)]" />
                  <span>Multi-Agent Strategy</span>
                </div>
                <p class="text-[11px] text-[var(--text-muted)] mt-1">Orchestrate task graph across persistent specialist fleet</p>
              </button>

              <button
                type="button"
                class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer text-left group"
                onclick={() => sendHomePrompt("Audit system security, local sandboxes and resource quotas")}
              >
                <div class="flex items-center gap-2 text-xs font-semibold text-[var(--text-primary)] group-hover:text-[var(--brand-text)]">
                  <Shield class="size-3.5 text-[var(--brand-text)]" />
                  <span>Security & Sandbox Audit</span>
                </div>
                <p class="text-[11px] text-[var(--text-muted)] mt-1">Verify zero-telemetry hardware enclave isolation</p>
              </button>

              <button
                type="button"
                class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer text-left group"
                onclick={() => sendHomePrompt("DeepSearch technical documentation and APIs")}
              >
                <div class="flex items-center gap-2 text-xs font-semibold text-[var(--text-primary)] group-hover:text-[var(--brand-text)]">
                  <Globe class="size-3.5 text-[var(--brand-text)]" />
                  <span>DeepSearch & RAG</span>
                </div>
                <p class="text-[11px] text-[var(--text-muted)] mt-1">Search web knowledge & query local vector embeddings</p>
              </button>
            </div>
          </div>

          <!-- Bottom Actions & Fleet Safety Protocol Card -->
          <div class="max-w-4xl mx-auto w-full space-y-3 relative z-10 pt-4">
            <div class="w-full p-2.5 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] flex items-center justify-between shadow-sm">
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  class="flex items-center gap-1.5 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] px-2.5 py-1.5 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--hairline-strong)] text-xs transition-colors cursor-pointer"
                  onclick={() => (showCommandPalette = true)}
                >
                  <span class="font-mono text-[10px] text-[var(--text-muted)]">⌘K</span>
                  <span>Command Palette</span>
                </button>

                <button
                  type="button"
                  class="flex items-center gap-1.5 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] px-2.5 py-1.5 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--hairline-strong)] text-xs transition-colors cursor-pointer"
                  onclick={openSettings}
                >
                  <span class="font-mono text-[10px] text-[var(--text-muted)]">⌘,</span>
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
  initialSection={settingsInitialTab}
/>
