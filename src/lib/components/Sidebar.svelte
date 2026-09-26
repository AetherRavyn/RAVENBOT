<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { getDiceBearUrl } from "$lib/utils";
  import { notify } from "$lib/toast";
  import { fleetActivity } from "$lib/fleetActivity.svelte";
  import { t } from "$lib/i18n";
  import { cn } from "$lib/utils.js";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Badge } from "$lib/components/ui/badge";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Avatar from "$lib/components/ui/avatar";
  import * as Tabs from "$lib/components/ui/tabs";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import {
    Bot,
    Plus,
    Settings as SettingsIcon,
    Search,
    Clock,
    AlertCircle,
    Crown,
    Server,
    Pause,
    MoreVertical,
    Wrench,
    Trash2,
    Sliders,
    Sparkles,
    Check,
    Circle,
    Palette,
    UserCheck,
    Layers,
    Pin,
    MailOpen,
    Copy,
    Eye,
    EyeOff,
    TriangleAlert,
  } from "@lucide/svelte";

  interface Props {
    bots: any[];
    selectedBotId: string | null;
    onSelectBot: (id: string) => void;
    onBotCreated: (bot: any) => void;
    onBotUpdated: (bot: any) => void;
    onBotDeleted: (botId: string) => void;
    openSettings: () => void;
    onNewChat?: () => void;
  }

  let {
    bots = [],
    selectedBotId,
    onSelectBot,
    onBotCreated,
    onBotUpdated,
    onBotDeleted,
    openSettings,
    onNewChat,
  }: Props = $props();

  let showCreateModal = $state(false);
  let createModalTab = $state("profile");
  let newBotName = $state("");
  let newBotDescription = $state("");
  let newBotAvatarUrl = $state<string | null>(null);
  let newBotAvatarStyle = $state("bottts");

  let searchQuery = $state("");
  let showOnlyWaiting = $state(false);
  let selectedBotForSettings = $state<any>(null);
  let showBotSettings = $state(false);
  let selectedBotForSkills = $state<any>(null);
  let showSkillManager = $state(false);
  let isCreating = $state(false);
  let activeActionMenuBotId = $state<string | null>(null);

  // ── Contact state: pinned / hidden / unread ────────────────────────────
  type Contact = { pinned: boolean; hidden: boolean };
  let contacts = $state<Record<string, Contact>>({});
  let unread = $state<Record<string, number>>({});
  let showHidden = $state(false);

  async function loadContacts() {
    try {
      const rows = (await invoke<any[]>("list_bot_contacts")) || [];
      const map: Record<string, Contact> = {};
      for (const row of rows) {
        map[row.bot_id] = { pinned: Boolean(row.pinned), hidden: Boolean(row.hidden) };
      }
      contacts = map;
    } catch (e) {
      console.error("Failed to load contacts:", e);
    }
  }

  async function loadUnread() {
    try {
      unread = (await invoke<Record<string, number>>("get_unread_counts")) || {};
    } catch (e) {
      console.error("Failed to load unread counts:", e);
    }
  }

  async function refreshContacts() {
    await Promise.all([loadContacts(), loadUnread()]);
  }

  onMount(() => {
    refreshContacts();
    const onBotsChanged = () => refreshContacts();
    // New assistant messages → refresh badges (unless this bot is open).
    const onStream = (e: Event) => {
      const kind = (e as CustomEvent)?.detail?.kind;
      if (kind === "done" || kind === "status") loadUnread();
    };
    window.addEventListener("bots-changed", onBotsChanged);
    window.addEventListener("agent-stream", onStream);
    return () => {
      window.removeEventListener("bots-changed", onBotsChanged);
      window.removeEventListener("agent-stream", onStream);
    };
  });

  async function togglePinned(botId: string) {
    const next = !contacts[botId]?.pinned;
    contacts = { ...contacts, [botId]: { pinned: next, hidden: contacts[botId]?.hidden ?? false } };
    try {
      await invoke("set_bot_pinned", { botId, pinned: next });
    } catch (e) {
      console.error("Failed to pin bot:", e);
    }
  }

  async function toggleHidden(botId: string) {
    const next = !contacts[botId]?.hidden;
    contacts = { ...contacts, [botId]: { pinned: contacts[botId]?.pinned ?? false, hidden: next } };
    try {
      await invoke("set_bot_hidden", { botId, hidden: next });
    } catch (e) {
      console.error("Failed to hide bot:", e);
    }
  }

  async function markRead(botId: string) {
    unread = { ...unread, [botId]: 0 };
    try {
      await invoke("mark_bot_read", { botId });
    } catch (e) {
      console.error("Failed to mark read:", e);
    }
  }

  async function duplicateBot(botId: string) {
    try {
      const copy = await invoke<any>("duplicate_bot", { botId });
      onBotCreated(copy);
      notify(t("sidebar.duplicated", { name: copy.name }), "success");
      await refreshContacts();
    } catch (e) {
      console.error("Failed to duplicate bot:", e);
    }
  }

  function selectBot(botId: string) {
    onSelectBot(botId);
    if (unread[botId]) markRead(botId);
  }

  let filteredBots = $derived(
    bots
      .filter((bot) => {
        const matchesSearch =
          bot.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          (bot.description && bot.description.toLowerCase().includes(searchQuery.toLowerCase()));
        const matchesFilter = !showOnlyWaiting || bot.status === "waiting_on_user";
        const matchesHidden = showHidden || !contacts[bot.id]?.hidden;
        return matchesSearch && matchesFilter && matchesHidden;
      })
      .sort((a, b) => {
        const ap = contacts[a.id]?.pinned ? 1 : 0;
        const bp = contacts[b.id]?.pinned ? 1 : 0;
        if (ap !== bp) return bp - ap;
        return 0;
      })
  );

  let effectiveAvatarUrl = $derived(
    newBotAvatarUrl || getDiceBearUrl(newBotName || "Agent", newBotAvatarStyle)
  );

  async function createBot() {
    if (!newBotName.trim()) return;
    isCreating = true;
    try {
      const bot = await invoke("create_bot", {
        name: newBotName,
        description: newBotDescription,
        avatarUrl: newBotAvatarUrl || effectiveAvatarUrl,
        avatarStyle: newBotAvatarStyle,
      });
      onBotCreated(bot);
      showCreateModal = false;
      newBotName = "";
      newBotDescription = "";
      newBotAvatarUrl = null;
      newBotAvatarStyle = "bottts";
      createModalTab = "profile";
    } catch (e) {
      console.error("Failed to create bot:", e);
    } finally {
      isCreating = false;
    }
  }

  function getStatusTheme(status: string) {
    switch (status) {
      case "idle":
        return { bg: "bg-[var(--status-idle)]", text: "text-[var(--text-muted)]", label: t("sidebar.idle") };
      case "thinking":
        return { bg: "bg-[var(--status-thinking)]", text: "text-[var(--status-thinking)]", label: t("sidebar.thinking") };
      case "running_tool":
        return { bg: "bg-[var(--status-running)]", text: "text-[var(--status-running)]", label: t("sidebar.runningTool") };
      case "waiting_on_user":
        return { bg: "bg-[var(--status-waiting)]", text: "text-[var(--status-waiting)]", label: t("sidebar.waitingOnYou") };
      case "paused":
        return { bg: "bg-[var(--status-paused)]", text: "text-[var(--status-paused)]", label: t("sidebar.paused") };
      default:
        return { bg: "bg-[var(--status-idle)]", text: "text-[var(--text-muted)]", label: status };
    }
  }
</script>

<svelte:window onclick={() => (activeActionMenuBotId = null)} />

<div class="flex flex-col h-full overflow-hidden select-none">
  <!-- New Chat Action Button (Grok Signature) -->
  <div class="p-3 pb-1.5">
    <button
      type="button"
      class="btn-brand w-full flex items-center justify-between px-3.5 py-2.5 text-xs group"
      onclick={() => {
        if (onNewChat) onNewChat();
        else onSelectBot("");
      }}
    >
      <div class="flex items-center gap-2">
        <Plus class="size-4 group-hover:rotate-90 transition-transform duration-300" />
        <span>{t("sidebar.newChat")}</span>
      </div>
      <span class="text-[10px] font-mono text-black/60 bg-black/10 px-1.5 py-0.5 rounded">⌘N</span>
    </button>
  </div>

  <!-- Section Header: FLEET AGENTS + Actions -->
  <div class="px-3 pt-2 pb-1.5 flex items-center justify-between">
    <div class="flex items-center gap-2">
      <span class="font-semibold text-[10px] tracking-wider uppercase text-[var(--text-muted)] font-mono">{t("sidebar.fleetAgents")}</span>
      <span class="bg-[var(--surface-2)] text-[var(--text-muted)] text-[10px] font-mono font-medium px-1.5 py-0.5 rounded border border-[var(--hairline)]">
        {bots.length}
      </span>
    </div>

    <div class="flex items-center gap-1">
      <button
        type="button"
        class="icon-btn size-7 border border-[var(--hairline)] bg-[var(--surface-2)]"
        aria-label={t("sidebar.createTip")}
        onclick={() => {
          showCreateModal = true;
          createModalTab = "profile";
        }}
        title={t("sidebar.createTip")}
      >
        <Plus class="size-3.5" />
      </button>
      <button
        type="button"
        class="icon-btn size-7 border border-[var(--hairline)] bg-[var(--surface-2)] {showHidden ? 'text-[var(--brand-text)] border-[var(--brand)]' : ''}"
        aria-label={showHidden ? t("sidebar.hideHiddenTip") : t("sidebar.showHiddenTip")}
        aria-pressed={showHidden}
        onclick={() => (showHidden = !showHidden)}
        title={showHidden ? t("sidebar.hideHiddenTip") : t("sidebar.showHiddenTip")}
      >
        {#if showHidden}<Eye class="size-3.5" />{:else}<EyeOff class="size-3.5" />{/if}
      </button>
      <button
        type="button"
        class="icon-btn size-7 border border-[var(--hairline)] bg-[var(--surface-2)]"
        aria-label={t("sidebar.settings")}
        onclick={openSettings}
        title={t("sidebar.settings")}
      >
        <SettingsIcon class="size-3.5" />
      </button>
    </div>
  </div>

  <!-- Search & Filter Controls -->
  <div class="px-3 py-1 space-y-1.5">
    <div class="relative group/search">
      <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)] pointer-events-none transition-colors group-focus-within/search:text-[var(--text-secondary)]" />
      <input
        type="text"
        placeholder={t("sidebar.search")}
        aria-label={t("sidebar.search")}
        bind:value={searchQuery}
        class="w-full h-8 pl-8 pr-3 bg-[var(--surface-2)] border border-[var(--hairline)] rounded-md text-xs text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--brand)] focus:ring-2 focus:ring-[var(--brand)]/25 transition-all font-sans"
      />
    </div>

    <!-- Waiting on me filter toggle -->
    {#if bots.some((b) => b.status === "waiting_on_user")}
      <button
        type="button"
        class="flex items-center gap-2 text-xs text-[var(--text-tertiary)] hover:text-[var(--text-primary)] px-1 py-1 transition-colors cursor-pointer"
        aria-pressed={showOnlyWaiting}
        onclick={() => (showOnlyWaiting = !showOnlyWaiting)}
      >
        <div class="size-3.5 rounded-full border border-[var(--hairline-strong)] flex items-center justify-center {showOnlyWaiting ? 'border-danger bg-danger/20' : ''}">
          {#if showOnlyWaiting}
            <div class="size-1.5 rounded-full bg-danger"></div>
          {/if}
        </div>
        <span class="text-[11px] font-medium {showOnlyWaiting ? 'text-danger' : 'text-[var(--text-muted)]'}">
          {t("sidebar.filterWaiting")}
        </span>
      </button>
    {/if}
  </div>

  <!-- Agent Card List -->
  <div class="flex-1 overflow-y-auto px-3 py-1.5 space-y-1.5">
    {#each filteredBots as bot (bot.id)}
      {@const isSelected = selectedBotId === bot.id}
      {@const statusTheme = getStatusTheme(bot.status)}
      {@const activity = fleetActivity.get(bot.id)}
      <div class="relative group/item">
        {#if isSelected}
          <span class="absolute left-0 top-1/2 -translate-y-1/2 h-6 w-[3px] rounded-full bg-[var(--rail-selected)] z-10"></span>
        {/if}
        <button
          type="button"
          class={cn(
            "w-full text-left px-2.5 py-2 min-h-[54px] rounded-xl flex items-center gap-3 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60",
            isSelected
              ? "bg-[var(--surface-1)] text-[var(--text-primary)]"
              : "bg-transparent hover:bg-[var(--surface-1)] text-[var(--text-secondary)]"
          )}
          style="transition-duration: var(--duration-hover)"
          aria-current={isSelected ? "true" : undefined}
          onclick={() => selectBot(bot.id)}
        >
          <!-- Avatar Container -->
          <div class="relative size-9 shrink-0">
            <div class="size-9 rounded-lg overflow-hidden bg-[var(--surface-3)] border border-[var(--hairline)]">
              <img
                src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "bottts")}
                alt={bot.name}
                class="size-full object-cover"
                loading="lazy"
              />
            </div>

            <!-- Status Badge — live fleet activity overrides stored status -->
            {#if activity === "working"}
              <div
                class="absolute -bottom-1.5 -right-1.5 flex items-center justify-center h-3.5 px-1 rounded-full bg-[var(--surface-3)] ring-2 ring-[var(--surface-1)] text-[var(--brand)]"
                aria-hidden="true"
              >
                <span class="typing-dot"></span><span class="typing-dot"></span><span class="typing-dot"></span>
              </div>
            {:else if activity === "attention"}
              <div
                class="absolute -bottom-1.5 -right-1.5 size-4 rounded-full flex items-center justify-center bg-[var(--surface-3)] ring-2 ring-[var(--surface-1)] animate-pulse"
                aria-hidden="true"
              >
                <TriangleAlert class="size-[9px] text-[var(--warning-text)]" strokeWidth={2.5} />
              </div>
            {:else}
              <div
                class={cn(
                  "absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full ring-2 ring-[var(--surface-1)]",
                  activity === "responded"
                    ? "bg-[var(--success-text)]"
                    : statusTheme.bg
                )}
              ></div>
            {/if}
          </div>

          <!-- Name & Status -->
          <div class="flex-1 min-w-0">
            <div class="flex items-center justify-between gap-1.5">
              <span class="flex items-center gap-1.5 min-w-0">
                <span class="font-semibold text-[13px] text-[var(--text-primary)] truncate">{bot.name}</span>
                {#if bot.specialty}
                  <span class="text-[10px] px-1.5 py-px rounded shrink-0 bg-[var(--surface-3)] text-[var(--text-muted)] truncate max-w-[96px]">{bot.specialty}</span>
                {/if}
              </span>
              <span class="flex items-center gap-1 shrink-0">
                {#if contacts[bot.id]?.pinned}
                  <Pin class="size-3 text-[var(--brand-text)]" />
                {/if}
                {#if bot.is_orchestrator}
                  <Crown class="size-3 text-warning" />
                {/if}
                {#if (unread[bot.id] ?? 0) > 0}
                  <span
                    class="min-w-4 h-4 px-1 rounded-full bg-[var(--brand)] text-[var(--text-on-light)] text-[9px] font-semibold flex items-center justify-center"
                    title={t("sidebar.unread", { n: unread[bot.id] })}
                  >
                    {(unread[bot.id] ?? 0) > 99 ? "99+" : unread[bot.id]}
                  </span>
                {/if}
              </span>
            </div>
            <span class="text-[12.5px] truncate block mt-px {activity === 'attention' ? 'text-[var(--warning-text)]' : activity === 'working' ? 'text-[var(--brand-text)]' : 'text-[var(--text-muted)]'}">
              {activity === "working"
                ? t("fleet.working")
                : activity === "attention"
                  ? t("fleet.attention")
                  : activity === "responded"
                    ? t("fleet.replied")
                    : statusTheme.label}
            </span>
          </div>
        </button>

        <!-- Quick Action Menu Trigger -->
        <button
          type="button"
          class="absolute right-2 top-2 size-6 rounded-md bg-[var(--surface-2)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] opacity-0 group-hover/item:opacity-100 transition-opacity flex items-center justify-center border border-[var(--hairline)] cursor-pointer"
          aria-label={t("sidebar.agentOptions")}
          aria-haspopup="true"
          aria-expanded={activeActionMenuBotId === bot.id}
          onclick={(e) => {
            e.stopPropagation();
            activeActionMenuBotId = activeActionMenuBotId === bot.id ? null : bot.id;
          }}
          title={t("sidebar.agentOptions")}
        >
          <MoreVertical class="size-3.5" />
        </button>

        <!-- Popover Action Menu -->
        {#if activeActionMenuBotId === bot.id}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute right-2 top-8 z-40 w-40 bg-[var(--surface-1)] border border-[var(--hairline-strong)] rounded-xl shadow-xl p-1 space-y-0.5 animate-in fade-in zoom-in-95 text-xs"
            onclick={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] text-left transition-colors cursor-pointer"
              onclick={() => {
                selectedBotForSettings = bot;
                showBotSettings = true;
                activeActionMenuBotId = null;
              }}
            >
              <Sliders class="size-3.5 text-[var(--text-tertiary)]" />
              {t("sidebar.settings")}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] text-left transition-colors cursor-pointer"
              onclick={() => {
                selectedBotForSkills = bot;
                showSkillManager = true;
                activeActionMenuBotId = null;
              }}
            >
              <Wrench class="size-3.5 text-[var(--text-tertiary)]" />
              {t("sidebar.skills")}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--brand-text)] hover:bg-[var(--brand-soft)] text-left transition-colors cursor-pointer"
              onclick={() => {
                onSelectBot(bot.id);
                window.dispatchEvent(new CustomEvent("open-connectors"));
                activeActionMenuBotId = null;
              }}
            >
              <Layers class="size-3.5 text-[var(--brand-text)]" />
              {t("sidebar.connectorsHub")}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--brand-text)] hover:bg-[var(--brand-soft)] text-left transition-colors cursor-pointer"
              onclick={() => {
                onSelectBot(bot.id);
                window.dispatchEvent(new CustomEvent("open-connectors", { detail: { botId: bot.id } }));
                activeActionMenuBotId = null;
              }}
            >
              <Server class="size-3.5 text-[var(--brand-text)]" />
              {t("sidebar.mcpTools")}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] text-left transition-colors cursor-pointer"
              onclick={() => {
                togglePinned(bot.id);
                activeActionMenuBotId = null;
              }}
            >
              <Pin class="size-3.5 text-[var(--brand-text)]" />
              {contacts[bot.id]?.pinned ? t("sidebar.unpin") : t("sidebar.pin")}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] text-left transition-colors cursor-pointer"
              onclick={() => {
                markRead(bot.id);
                activeActionMenuBotId = null;
              }}
            >
              <MailOpen class="size-3.5 text-[var(--text-tertiary)]" />
              {t("sidebar.markRead")}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] text-left transition-colors cursor-pointer"
              onclick={() => {
                duplicateBot(bot.id);
                activeActionMenuBotId = null;
              }}
            >
              <Copy class="size-3.5 text-[var(--text-tertiary)]" />
              {t("sidebar.duplicate")}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] text-left transition-colors cursor-pointer"
              onclick={() => {
                toggleHidden(bot.id);
                activeActionMenuBotId = null;
              }}
            >
              {#if contacts[bot.id]?.hidden}
                <Eye class="size-3.5 text-[var(--text-tertiary)]" />
                {t("sidebar.unhide")}
              {:else}
                <EyeOff class="size-3.5 text-[var(--text-tertiary)]" />
                {t("sidebar.hide")}
              {/if}
            </button>
            <button
              type="button"
              class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-xl text-danger hover:bg-danger/10 text-left transition-colors cursor-pointer"
              onclick={() => {
                activeActionMenuBotId = null;
                selectedBotForSettings = bot;
                showBotSettings = true;
              }}
            >
              <Trash2 class="size-3.5" />
              {t("sidebar.remove")}
            </button>
          </div>
        {/if}
      </div>
    {:else}
      <div class="py-12 px-3 text-center text-[var(--text-muted)]">
        <Bot class="size-8 mx-auto mb-2 opacity-30 text-[var(--text-tertiary)]" />
        <p class="text-xs">{t("sidebar.noBots")}</p>
      </div>
    {/each}
  </div>

  <!-- Bottom Pause Fleet Bar -->
  <div class="p-3 border-t border-[var(--hairline)] bg-[var(--surface-0)]">
    <button
      type="button"
      class="w-full bg-[var(--surface-2)] border border-[var(--hairline)] hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-3)] rounded-md py-2 px-3 flex items-center justify-between text-xs text-[var(--text-secondary)] transition-colors font-medium cursor-pointer"
      onclick={() => invoke("pause_all")}
    >
      <div class="flex items-center gap-2">
        <Pause class="size-3.5 fill-current text-[var(--text-tertiary)]" />
        <span>{t("sidebar.pauseAll")}</span>
      </div>
      <span class="font-mono text-[10px] text-[var(--text-muted)] bg-[var(--surface-3)] px-1.5 py-0.5 rounded">⌘P</span>
    </button>
  </div>
</div>

<!-- Create Bot Dialog (Clean, Responsive 2-Tab Design with Fixed Footer) -->
<Dialog.Root open={showCreateModal} onOpenChange={(o) => (!o && (showCreateModal = false))}>
  <Dialog.Content class="sm:max-w-xl max-h-[85vh] flex flex-col bg-[var(--surface-1)] border border-[var(--hairline-strong)] shadow-[var(--shadow-xl)] rounded-xl p-0 overflow-hidden">
    <!-- Fixed Dialog Header -->
    <div class="px-6 pt-5 pb-3 border-b border-[var(--hairline)] shrink-0">
      <Dialog.Header class="gap-1">
        <Dialog.Title class="text-base font-semibold flex items-center gap-2 text-[var(--text-primary)]">
          <Bot class="size-5 text-[var(--brand-text)]" />
          {t("sidebar.createBot")}
        </Dialog.Title>
        <Dialog.Description class="text-xs text-[var(--text-muted)]">
          {t("sidebar.provisionDesc")}
        </Dialog.Description>
      </Dialog.Header>

      <!-- Sub-tabs: Profile vs Avatar Picker -->
      <div class="grid grid-cols-2 bg-[var(--surface-2)] border border-[var(--hairline)] p-1 rounded-xl mt-3">
        <button
          type="button"
          class="flex items-center justify-center gap-1.5 py-1 px-3 rounded-lg text-xs font-medium transition-all cursor-pointer {createModalTab === 'profile'
            ? 'bg-[var(--surface-3)] text-[var(--text-primary)] font-semibold shadow-sm'
            : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          onclick={() => (createModalTab = "profile")}
        >
          <UserCheck class="size-3.5" />
          <span>{t("sidebar.agentDetails")}</span>
        </button>

        <button
          type="button"
          class="flex items-center justify-center gap-1.5 py-1 px-3 rounded-lg text-xs font-medium transition-all cursor-pointer {createModalTab === 'avatar'
            ? 'bg-[var(--surface-3)] text-[var(--text-primary)] font-semibold shadow-sm'
            : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          onclick={() => (createModalTab = "avatar")}
        >
          <Palette class="size-3.5" />
          <span>{t("sidebar.chooseAvatar", { style: newBotAvatarStyle })}</span>
        </button>
      </div>
    </div>

    <!-- Scrollable Dialog Body (Guarantees no modal overflow) -->
    <form onsubmit={(e) => { e.preventDefault(); createBot(); }} class="flex-1 flex flex-col overflow-hidden">
      <div class="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        {#if createModalTab === "profile"}
          <!-- Live Avatar Preview Card -->
          <div class="p-3.5 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] flex items-center justify-between">
            <div class="flex items-center gap-3.5">
              <div class="size-14 rounded-xl overflow-hidden bg-[var(--surface-3)] border-2 border-[var(--brand)]/60 p-0.5 shrink-0">
                <img
                  src={effectiveAvatarUrl}
                  alt="Agent Avatar"
                  class="size-full rounded-xl object-cover"
                />
              </div>
              <div class="flex flex-col">
                <span class="text-sm font-medium text-[var(--text-primary)]">
                  {newBotName || t("sidebar.newAgent")}
                </span>
                <span class="text-xs text-[var(--brand-text)] capitalize font-mono mt-0.5">
                  {t("sidebar.styleLabel", { style: newBotAvatarStyle })}
                </span>
              </div>
            </div>

            <Button
              type="button"
              variant="outline"
              size="sm"
              class="h-8 gap-1.5 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
              onclick={() => (createModalTab = "avatar")}
            >
              <Palette class="size-3.5 text-[var(--brand-text)]" />
              {t("sidebar.customize")}
            </Button>
          </div>

          <div class="space-y-1.5">
            <Label for="new-bot-name" class="text-xs font-semibold uppercase tracking-wider text-[var(--text-muted)]">
              {t("sidebar.agentName")}
            </Label>
            <Input
              id="new-bot-name"
              type="text"
              bind:value={newBotName}
              placeholder={t("sidebar.agentNamePh")}
              class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)]"
              required
            />
          </div>

          <div class="space-y-1.5">
            <Label for="new-bot-desc" class="text-xs font-semibold uppercase tracking-wider text-[var(--text-muted)]">
              {t("sidebar.mission")}
            </Label>
            <Textarea
              id="new-bot-desc"
              bind:value={newBotDescription}
              placeholder={t("sidebar.missionPh")}
              rows={3}
              class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] resize-none"
            />
          </div>
        {:else}
          <!-- Avatar Picker View inside tab -->
          <div class="space-y-2">
            <AvatarPicker
              seed={newBotName || "Agent"}
              style={newBotAvatarStyle}
              customUrl={newBotAvatarUrl}
              onSelect={(url, style) => {
                newBotAvatarUrl = url;
                newBotAvatarStyle = style;
                createModalTab = "profile";
              }}
            />
          </div>
        {/if}
      </div>

      <!-- Always Fixed Pinned Footer (Never gets pushed out of view) -->
      <div class="px-6 py-3.5 border-t border-[var(--hairline)] bg-[var(--surface-1)] flex items-center justify-end gap-2 shrink-0">
        <Button variant="outline" size="sm" type="button" onclick={() => (showCreateModal = false)}>
          {t("ui.cancel")}
        </Button>
        <Button
          size="sm"
          type="submit"
          class="gap-1.5 font-medium"
          disabled={!newBotName.trim() || isCreating}
        >
          <Plus class="size-3.5" />
          {isCreating ? t("sidebar.creating") : t("sidebar.createAgent")}
        </Button>
      </div>
    </form>
  </Dialog.Content>
</Dialog.Root>

<!-- Bot Settings Modal -->
{#if showBotSettings && selectedBotForSettings}
  {#await import("$lib/components/BotSettings.svelte") then BotSettings}
    <BotSettings.default
      bot={selectedBotForSettings}
      open={showBotSettings}
      onClose={() => { showBotSettings = false; selectedBotForSettings = null; }}
      onUpdated={(updatedBot) => {
        if (updatedBot === null) {
          onBotDeleted(selectedBotForSettings.id);
        } else {
          onBotUpdated(updatedBot);
        }
        showBotSettings = false;
        selectedBotForSettings = null;
      }}
    />
  {/await}
{/if}

<!-- Skill Manager Modal -->
{#if showSkillManager && selectedBotForSkills}
  {#await import("$lib/components/SkillManager.svelte") then SkillManager}
    <SkillManager.default
      bot={selectedBotForSkills}
      open={showSkillManager}
      onClose={() => { showSkillManager = false; selectedBotForSkills = null; }}
      onUpdated={(updatedBot) => {
        if (updatedBot) {
          onBotUpdated(updatedBot);
        }
        showSkillManager = false;
        selectedBotForSkills = null;
      }}
    />
  {/await}
{/if}
