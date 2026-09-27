<script lang="ts">
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { onMount } from "svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import ScreenReader from "$lib/components/ScreenReader.svelte";
  import Toaster from "$lib/components/Toaster.svelte";
  import CommandPalette from "$lib/components/CommandPalette.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import KillSwitch from "$lib/components/KillSwitch.svelte";
  import ThreadView from "$lib/components/ThreadView.svelte";
  import ChatRoomView from "$lib/components/ChatRoomView.svelte";
  import ConnectorCenter from "$lib/components/ConnectorCenter.svelte";
  import RoutinesPanel from "$lib/components/RoutinesPanel.svelte";
  import SkillManager from "$lib/components/SkillManager.svelte";
  import PluginsStore from "$lib/components/PluginsStore.svelte";
  import WorkspaceRail from "$lib/components/workspace/WorkspaceRail.svelte";
  import WorkspaceSidebar from "$lib/components/workspace/WorkspaceSidebar.svelte";
  import HomePane from "$lib/components/workspace/HomePane.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Loader2, Bot as BotIcon } from "@lucide/svelte";
  import { getDiceBearUrl } from "$lib/utils";
  import { workspace } from "$lib/workspace.svelte";
  import { fleetActivity } from "$lib/fleetActivity.svelte";
  import { initI18n, t } from "$lib/i18n";
  import { prefersReducedMotion, keyboardShortcuts } from "$lib/a11y";
  import { getStoredTheme, applyTheme, subscribeTheme, type ThemeDefinition } from "$lib/theme";

  let currentTheme = $state<ThemeDefinition>(getStoredTheme());
  let routinesBotId = $state<string | null>(null);
  let skillsModalBotId = $state<string | null>(null);
  let marketplaceModalBotId = $state<string | null>(null);

  let skillsModalBot = $derived(workspace.bots.find((b: any) => b.id === skillsModalBotId));
  let marketplaceModalBot = $derived(workspace.bots.find((b: any) => b.id === marketplaceModalBotId));

  // Rail launchers act on the agent you're looking at, falling back to the first one.
  function targetBotId(): string | null {
    return workspace.selectedBotId || workspace.bots[0]?.id || null;
  }
  function openSkills() {
    const id = targetBotId();
    if (!id) { workspace.showAgents(); return; }
    skillsModalBotId = id;
  }
  function openMarketplace() {
    const id = targetBotId();
    if (!id) { workspace.showAgents(); return; }
    marketplaceModalBotId = id;
  }

  let newAgentName = $state("");
  let creatingAgent = $state(false);
  async function createAgent() {
    const name = newAgentName.trim();
    if (!name || creatingAgent) return;
    creatingAgent = true;
    try {
      await workspace.createAgent(name);
      newAgentName = "";
    } catch (e) {
      console.error("Failed to create agent:", e);
    } finally {
      creatingAgent = false;
    }
  }

  let showSidebar = $derived(
    !workspace.sidebarCollapsed && workspace.dest !== "connectors" && workspace.dest !== "routines",
  );
  let routinesBot = $derived(
    workspace.bots.find((b: any) => b.id === routinesBotId) || workspace.bots[0],
  );

  initI18n();

  onMount(() => {
    applyTheme(getStoredTheme().id);
    const unsubTheme = subscribeTheme((theme) => (currentTheme = theme));

    workspace.init();
    fleetActivity.start();

    const unsubK = keyboardShortcuts.register("mod+k", () => workspace.togglePalette());
    const unsubComma = keyboardShortcuts.register("mod+,", () => {
      workspace.showSettings = !workspace.showSettings;
    });
    const unsubB = keyboardShortcuts.register("mod+b", () => {
      workspace.sidebarCollapsed = !workspace.sidebarCollapsed;
    });
    const unsubN = keyboardShortcuts.register("mod+n", () => workspace.newChat());
    const unsubEsc = keyboardShortcuts.register("escape", () => {
      workspace.showPalette = false;
      workspace.showSettings = false;
    });

    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handler = (e: MediaQueryListEvent) => {
      document.documentElement.classList.toggle("reduce-motion", e.matches);
    };
    if (prefersReducedMotion()) document.documentElement.classList.add("reduce-motion");
    mediaQuery.addEventListener("change", handler);

    const handleOpenSettings = () => workspace.openSettings("keys");
    const onResize = () => (workspace.windowWidth = window.innerWidth);
    window.addEventListener("resize", onResize);
    const handleOpenConnectors = (e: Event) => {
      const botId = (e as CustomEvent<{ botId?: string }>).detail?.botId;
      if (botId) workspace.selectedBotId = botId;
      workspace.goto("connectors");
    };
    const handleOfficeDeleted = (e: Event) => workspace.handleRoomDeleted(e);
    const handleOfficeUpdated = (e: Event) => workspace.handleRoomUpdated(e);
    const handleBotsChanged = () => workspace.refreshBots();
    window.addEventListener("open-settings", handleOpenSettings);
    window.addEventListener("open-connectors", handleOpenConnectors);
    window.addEventListener("office-deleted", handleOfficeDeleted);
    window.addEventListener("office-updated", handleOfficeUpdated);
    window.addEventListener("bots-changed", handleBotsChanged);

    return () => {
      fleetActivity.stop();
      unsubTheme();
      unsubK();
      unsubComma();
      unsubB();
      unsubN();
      unsubEsc();
      mediaQuery.removeEventListener("change", handler);
      window.removeEventListener("resize", onResize);
      window.removeEventListener("open-settings", handleOpenSettings);
      window.removeEventListener("open-connectors", handleOpenConnectors);
      window.removeEventListener("office-deleted", handleOfficeDeleted);
      window.removeEventListener("office-updated", handleOfficeUpdated);
      window.removeEventListener("bots-changed", handleBotsChanged);
    };
  });
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
  <TitleBar
    sidebarCollapsed={workspace.sidebarCollapsed}
    onToggleSidebar={() => (workspace.sidebarCollapsed = !workspace.sidebarCollapsed)}
  />

  <div
    class="app-frame flex-1 min-h-0"
    style="--frame-sidebar: {showSidebar ? workspace.sidebarWidth + 'px' : '0px'}"
  >
    <WorkspaceRail
      onOpenSettings={() => workspace.openSettings()}
      onOpenSkills={openSkills}
      onOpenMarketplace={openMarketplace}
    />

    {#if showSidebar}
      <WorkspaceSidebar theme={currentTheme} />
    {:else}
      <div aria-hidden="true"></div>
    {/if}

    <main class="min-w-0 flex flex-col overflow-hidden relative bg-[var(--surface-0)]" aria-label={t("a11y.thread")}>
      {#if workspace.killSwitchActive}
        <div class="p-2 px-4 bg-[var(--danger-soft)] border-b border-[var(--danger-border)] flex items-center justify-center" style="z-index: var(--layer-dropdown)" role="alert">
          <KillSwitch />
        </div>
      {/if}

      {#if workspace.loading}
        <div class="flex-1 flex flex-col items-center justify-center gap-3 p-8 text-[var(--text-muted)]" role="status">
          <div class="size-12 rounded-2xl flex items-center justify-center ring-4 animate-pulse border border-[var(--hairline)] bg-[var(--brand-soft)] text-[var(--brand-text)]">
            <Loader2 class="size-6 animate-spin" />
          </div>
          <div class="text-center space-y-0.5">
            <p class="text-sm font-semibold text-[var(--text-primary)]">{t("workspace.boot", { brand: currentTheme.brand.brandTitle })}</p>
            <p class="text-xs text-[var(--text-muted)]">{t("workspace.bootDesc")}</p>
          </div>
        </div>
      {:else if workspace.dest === "connectors"}
        <div class="flex-1 min-h-0 overflow-hidden">
          <ConnectorCenter
            bots={workspace.bots}
            selectedBotId={workspace.selectedBotId}
            onSelectBot={(id) => (workspace.selectedBotId = id)}
            onBotsUpdated={() => workspace.refreshBots()}
          />
        </div>
      {:else if workspace.dest === "routines"}
        <!-- RoutinesPanel owns the pane header (title chips, bot switcher, New);
             the old duplicate 46px bar was removed. -->
        <div class="flex-1 min-h-0 overflow-y-auto px-4 py-3">
          {#if routinesBot}
            <div class="max-w-3xl">
              <RoutinesPanel
                bot={routinesBot}
                bots={workspace.bots}
                onBotChange={(id: string) => (routinesBotId = id)}
              />
            </div>
          {:else}
            <p class="py-10 text-sm text-[var(--text-muted)] text-center">{t("routines.needBot")}</p>
          {/if}
        </div>
      {:else if workspace.dest === "agents" && !workspace.selectedBot}
        <div class="flex-1 min-h-0 overflow-y-auto flex items-center justify-center p-8">
          <div class="w-full max-w-md space-y-5 text-center">
            <div class="size-14 mx-auto rounded-2xl bg-[var(--brand-soft)] border border-[var(--brand)]/30 flex items-center justify-center text-[var(--brand-text)]">
              <BotIcon class="size-7" />
            </div>
            <div class="space-y-1">
              <h2 class="text-base font-bold text-[var(--text-primary)]">{t("rail.agentsTitle")}</h2>
              <p class="text-xs text-[var(--text-tertiary)]">{t("rail.agentsHint")}</p>
            </div>
            {#if workspace.bots.length > 0}
              <div class="flex flex-wrap justify-center gap-2">
                {#each workspace.bots as b (b.id)}
                  <button
                    type="button"
                    class="flex items-center gap-2 px-3 py-1.5 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] text-xs font-medium text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] hover:border-[var(--brand)]/40 cursor-pointer transition-colors"
                    onclick={() => workspace.selectBot(b.id)}
                  >
                    <RavenAvatar name={b.name} imageUrl={b.avatar_url} decorative />
                    <span>{b.name}</span>
                  </button>
                {/each}
              </div>
            {/if}
            <form class="flex gap-2" onsubmit={(e) => { e.preventDefault(); createAgent(); }}>
              <Input bind:value={newAgentName} placeholder={t("rail.agentsPh")} maxlength={32} class="h-9 text-xs" />
              <Button type="submit" size="sm" class="h-9 shrink-0" disabled={creatingAgent || !newAgentName.trim()}>
                {creatingAgent ? t("ui.saving") : t("rail.agentsCreate")}
              </Button>
            </form>
          </div>
        </div>
      {:else if workspace.selectedRoom}
        <ChatRoomView room={workspace.selectedRoom} bots={workspace.bots} />
      {:else if workspace.selectedBot}
        <ThreadView bot={workspace.selectedBot} />
      {:else}
        <HomePane theme={currentTheme} />
      {/if}
    </main>
  </div>
</div>

<CommandPalette
  open={workspace.showPalette}
  onClose={() => (workspace.showPalette = false)}
  bots={workspace.bots}
  onSelectBot={(id) => {
    workspace.selectBot(id);
    workspace.showPalette = false;
  }}
  onCreateBot={() => {
    workspace.showPalette = false;
    workspace.showAgents();
  }}
  onOpenSettings={() => {
    workspace.showPalette = false;
    workspace.openSettings();
  }}
/>

<Settings
  open={workspace.showSettings}
  onClose={() => (workspace.showSettings = false)}
  bots={workspace.bots}
  initialSection={workspace.settingsTab}
/>

<SkillManager
  bot={skillsModalBot}
  open={!!skillsModalBot}
  onClose={() => (skillsModalBotId = null)}
  onUpdated={(b: any) => workspace.handleBotUpdated(b)}
/>

<PluginsStore
  bot={marketplaceModalBot}
  open={!!marketplaceModalBot}
  onClose={() => (marketplaceModalBotId = null)}
/>

<ScreenReader message={workspace.srMessage} />
<Toaster />
