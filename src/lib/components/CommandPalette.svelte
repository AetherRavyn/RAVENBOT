<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t, type TranslationKey } from "$lib/i18n";
  import { highlightParts, matchesQuery } from "$lib/search/highlight";
  import { cn } from "$lib/utils.js";
  import {
    Activity,
    BarChart3,
    Bot,
    Building2,
    Command,
    CornerDownLeft,
    Cpu,
    DollarSign,
    GitBranch,
    Globe,
    Info,
    Key,
    Layers,
    Loader2,
    MessageSquare,
    Palette,
    Plug,
    Plus,
    RefreshCw,
    Search,
    Server,
    Settings as SettingsIcon,
    Sparkles,
    Store,
    User,
    Clock,
    X,
  } from "@lucide/svelte";

  interface MessageHit {
    message_id: string;
    thread_id: string;
    bot_id: string;
    thread_title: string;
    role: string;
    snippet: string;
    created_at: string;
  }

  interface Props {
    open: boolean;
    onClose: () => void;
    bots?: any[];
    chatrooms?: any[];
    onSelectBot: (id: string) => void;
    onSelectRoom?: (id: string) => void;
    onCreateBot: () => void;
    onNewChat?: () => void;
    onOpenSettings: (section?: string) => void;
    onNavigate?: (
      dest: "home" | "agents" | "offices" | "projects" | "channels" | "connectors" | "routines",
    ) => void;
    onOpenPlugins?: () => void;
    onOpenSkills?: () => void;
    onToggleBrowser?: () => void;
    onOpenMessage?: (hit: MessageHit) => void;
  }

  let {
    open,
    onClose,
    bots = [],
    chatrooms = [],
    onSelectBot,
    onSelectRoom,
    onCreateBot,
    onNewChat,
    onOpenSettings,
    onNavigate,
    onOpenPlugins,
    onOpenSkills,
    onToggleBrowser,
    onOpenMessage,
  }: Props = $props();

  type ResultKind = "agent" | "office" | "message" | "settings" | "action";
  type Tab = "all" | "agents" | "offices" | "messages" | "settings";

  interface SearchResult {
    key: string;
    kind: ResultKind;
    title: string;
    subtitle: string;
    badge?: string;
    keywords?: string;
    icon: any;
    run: () => void;
  }

  interface ResultSection {
    key: string;
    label: string;
    items: SearchResult[];
  }

  type PaletteRow =
    | { type: "header"; key: string; label: string; count: number }
    | { type: "item"; key: string; item: SearchResult; index: number };

  let query = $state("");
  let tab = $state<Tab>("all");
  let selectedIndex = $state(0);
  let inputRef = $state<HTMLInputElement | null>(null);
  let messageHits = $state<MessageHit[]>([]);
  let messageLoading = $state(false);
  let messageError = $state<string | null>(null);
  let messageSeq = 0;
  let messageTimer: ReturnType<typeof setTimeout> | null = null;
  let messageRetry = $state(0);

  const tabs: { id: Tab; labelKey: string }[] = [
    { id: "all", labelKey: "palette.tabAll" },
    { id: "agents", labelKey: "palette.tabAgents" },
    { id: "offices", labelKey: "palette.tabOffices" },
    { id: "messages", labelKey: "palette.tabMessages" },
    { id: "settings", labelKey: "palette.tabSettings" },
  ];

  function closeWith(action: () => void) {
    onClose();
    action();
  }

  const settingsLinks = $derived([
    {
      id: "providers",
      label: t("settings.navProviders"),
      icon: Key,
      keywords: "api key keys provider token connect model",
    },
    {
      id: "connectors",
      label: t("ui.connectors"),
      icon: Layers,
      keywords: "mcp connectors integrations tools servers",
    },
    {
      id: "models",
      label: t("settings.navModels"),
      icon: Cpu,
      keywords: "model manager ai context provider",
    },
    {
      id: "skills",
      label: t("settings.navSkills"),
      icon: Sparkles,
      keywords: "skills local capability",
    },
    {
      id: "budgets",
      label: t("settings.navBudgets"),
      icon: DollarSign,
      keywords: "budget cost spend usage limit",
    },
    {
      id: "usage",
      label: t("settings.navUsage"),
      icon: BarChart3,
      keywords: "usage cost tokens spend runs",
    },
    {
      id: "diagnostics",
      label: t("palette.diagnostics"),
      icon: Activity,
      keywords: "runs errors logs diagnostics audit",
    },
    {
      id: "profile",
      label: t("settings.navProfile"),
      icon: User,
      keywords: "profile avatar name identity",
    },
    {
      id: "themes",
      label: t("settings.navThemes"),
      icon: Palette,
      keywords: "theme appearance dark light color",
    },
    {
      id: "local",
      label: t("settings.navLocal"),
      icon: Server,
      keywords: "local ollama endpoint offline",
    },
    {
      id: "about",
      label: t("settings.about"),
      icon: Info,
      keywords: "about version update release",
    },
  ]);

  const quickActions = $derived.by(() => {
    const actions: SearchResult[] = [];
    if (onNewChat) {
      actions.push({
        key: "action:new-chat",
        kind: "action",
        title: t("palette.newChat"),
        subtitle: t("palette.newChatDesc"),
        icon: Plus,
        keywords: "new chat conversation home",
        run: onNewChat,
      });
    }
    actions.push({
      key: "action:new-agent",
      kind: "action",
      title: t("palette.createBot"),
      subtitle: t("palette.createBotDesc"),
      icon: Bot,
      keywords: "new agent bot create",
      run: onCreateBot,
    });
    if (onNavigate) {
      actions.push({
        key: "action:offices",
        kind: "action",
        title: t("rail.offices"),
        subtitle: t("palette.openOfficesDesc"),
        icon: Building2,
        keywords: "offices rooms team",
        run: () => onNavigate?.("offices"),
      });
      actions.push({
        key: "action:projects",
        kind: "action",
        title: t("rail.projects"),
        subtitle: t("palette.openProjectsDesc"),
        icon: GitBranch,
        keywords: "projects code files workspace",
        run: () => onNavigate?.("projects"),
      });
      actions.push({
        key: "action:connectors",
        kind: "action",
        title: t("rail.mcpsConnectors"),
        subtitle: t("palette.connectorsDesc"),
        icon: Plug,
        keywords: "connectors mcp tools integrations",
        run: () => onNavigate?.("connectors"),
      });
      actions.push({
        key: "action:routines",
        kind: "action",
        title: t("rail.routines"),
        subtitle: t("palette.openRoutinesDesc"),
        icon: Clock,
        keywords: "routines schedule cron automation",
        run: () => onNavigate?.("routines"),
      });
    }
    if (onOpenPlugins) {
      actions.push({
        key: "action:plugins",
        kind: "action",
        title: t("palette.openPlugins"),
        subtitle: t("palette.openPluginsDesc"),
        icon: Store,
        keywords: "plugins tools openapi marketplace install",
        run: onOpenPlugins,
      });
    }
    if (onOpenSkills) {
      actions.push({
        key: "action:skills",
        kind: "action",
        title: t("rail.skills"),
        subtitle: t("palette.openSkillsDesc"),
        icon: Sparkles,
        keywords: "skills capabilities marketplace",
        run: onOpenSkills,
      });
    }
    if (onToggleBrowser) {
      actions.push({
        key: "action:browser",
        kind: "action",
        title: t("rail.browser"),
        subtitle: t("palette.openBrowserDesc"),
        icon: Globe,
        keywords: "browser web dock",
        run: onToggleBrowser,
      });
    }
    actions.push({
      key: "action:settings",
      kind: "action",
      title: t("palette.settings"),
      subtitle: t("palette.settingsDesc"),
      icon: SettingsIcon,
      keywords: "settings preferences options",
      run: () => onOpenSettings(),
    });
    return actions;
  });

  function messageResult(hit: MessageHit): SearchResult {
    return {
      key: `message:${hit.message_id}`,
      kind: "message",
      title: hit.thread_title || t("palette.tabMessages"),
      subtitle: hit.snippet || hit.role,
      badge: hit.role,
      icon: MessageSquare,
      keywords: `${hit.snippet} ${hit.role}`,
      run: () => onOpenMessage?.(hit),
    };
  }

  const agentResults = $derived(
    bots
      .filter((bot) =>
        matchesQuery(
          [bot.name, bot.description, bot.rank, bot.specialty, bot.config?.model_provider],
          query,
        ),
      )
      .slice(0, tab === "agents" ? 50 : 6)
      .map(
        (bot): SearchResult => ({
          key: `agent:${bot.id}`,
          kind: "agent",
          title: bot.name || t("ui.fallbackAgent"),
          subtitle: bot.description || bot.rank || t("palette.openAgent"),
          badge: bot.config?.model_provider,
          icon: Bot,
          keywords: `${bot.rank ?? ""} ${bot.specialty ?? ""} ${bot.config?.model_provider ?? ""}`,
          run: () => onSelectBot(bot.id),
        }),
      ),
  );

  const officeResults = $derived(
    onSelectRoom
      ? chatrooms
          .filter((room) =>
            matchesQuery([room.name, room.description, room.goal, room.office_template], query),
          )
          .slice(0, tab === "offices" ? 50 : 6)
          .map(
            (room): SearchResult => ({
              key: `office:${room.id}`,
              kind: "office",
              title: room.name || t("rail.offices"),
              subtitle: room.description || room.goal || t("palette.openOffice"),
              badge: room.office_template,
              icon: Building2,
              keywords: `${room.goal ?? ""} ${room.office_template ?? ""}`,
              run: () => onSelectRoom?.(room.id),
            }),
          )
      : [],
  );

  const messageResults = $derived(
    tab === "all" || tab === "messages"
      ? messageHits.slice(0, tab === "messages" ? 30 : 5).map(messageResult)
      : [],
  );

  const settingsResults = $derived(
    tab === "settings" || query.trim()
      ? settingsLinks
          .filter((entry) => matchesQuery([entry.label, entry.id, entry.keywords], query))
          .slice(0, tab === "settings" ? 30 : 5)
          .map(
            (entry): SearchResult => ({
              key: `settings:${entry.id}`,
              kind: "settings",
              title: entry.label,
              subtitle: t("palette.openSettingsSection", { section: entry.label }),
              icon: entry.icon,
              keywords: entry.keywords,
              run: () => onOpenSettings(entry.id),
            }),
          )
      : [],
  );

  const actionResults = $derived(
    quickActions
      .filter((entry) => matchesQuery([entry.title, entry.subtitle, entry.keywords], query))
      .slice(0, tab === "settings" ? 30 : 6),
  );

  const sections = $derived.by(() => {
    const result: ResultSection[] = [];
    if (tab === "all" || tab === "agents") {
      result.push({ key: "agents", label: t("palette.tabAgents"), items: agentResults });
    }
    if (tab === "all" || tab === "offices") {
      result.push({ key: "offices", label: t("palette.tabOffices"), items: officeResults });
    }
    if (tab === "all" || tab === "messages") {
      result.push({ key: "messages", label: t("palette.tabMessages"), items: messageResults });
    }
    if (tab === "settings" || (tab === "all" && query.trim())) {
      result.push({ key: "settings", label: t("palette.tabSettings"), items: settingsResults });
    }
    if (tab === "all" || tab === "settings") {
      result.push({ key: "actions", label: t("palette.sectionActions"), items: actionResults });
    }
    return result.filter((section) => section.items.length > 0);
  });

  const rows = $derived.by(() => {
    const result: PaletteRow[] = [];
    let index = 0;
    for (const section of sections) {
      result.push({
        type: "header",
        key: `header:${section.key}`,
        label: section.label,
        count: section.items.length,
      });
      for (const item of section.items) {
        result.push({ type: "item", key: `item:${item.key}`, item, index });
        index += 1;
      }
    }
    return result;
  });

  const items = $derived(rows.filter((row): row is Extract<PaletteRow, { type: "item" }> => row.type === "item"));
  const itemCount = $derived(items.length);
  const selectedResult = $derived(items[selectedIndex]?.item);

  const tabCounts = $derived<Record<Tab, number>>({
    all: 0,
    agents: bots.length,
    offices: chatrooms.length,
    messages: messageHits.length,
    settings: settingsLinks.length + quickActions.length,
  });

  $effect(() => {
    if (open) {
      query = "";
      tab = "all";
      selectedIndex = 0;
      messageHits = [];
      messageError = null;
      messageLoading = false;
      setTimeout(() => inputRef?.focus(), 40);
    } else {
      messageHits = [];
      messageError = null;
      messageLoading = false;
    }
  });

  $effect(() => {
    if (!open) return;
    // Read the retry counter so the search runs again when the user asks.
    void messageRetry;
    const needle = query.trim();
    const wantsMessages = tab === "all" || tab === "messages";
    if (!wantsMessages || needle.length < 2) {
      messageSeq += 1;
      messageHits = [];
      messageError = null;
      messageLoading = false;
      return;
    }
    const seq = ++messageSeq;
    if (messageTimer) clearTimeout(messageTimer);
    messageTimer = setTimeout(async () => {
      messageLoading = true;
      messageError = null;
      try {
        const hits = await invoke<MessageHit[]>("search_messages", {
          query: needle,
          limit: 30,
        });
        if (seq === messageSeq) messageHits = hits;
      } catch (error) {
        if (seq === messageSeq) {
          messageHits = [];
          messageError = String(error);
        }
      } finally {
        if (seq === messageSeq) messageLoading = false;
      }
    }, 220);
    return () => {
      if (messageTimer) {
        clearTimeout(messageTimer);
        messageTimer = null;
      }
    };
  });

  $effect(() => {
    // A new query or tab starts at the top of its result set.
    void query;
    void tab;
    selectedIndex = 0;
  });

  $effect(() => {
    if (selectedIndex >= itemCount) selectedIndex = Math.max(0, itemCount - 1);
  });

  function choose(item: SearchResult) {
    closeWith(item.run);
  }

  function scrollSelected() {
    setTimeout(() => {
      const item = items[selectedIndex]?.item;
      if (!item) return;
      document
        .getElementById(`palette-item-${item.key}`)
        ?.scrollIntoView?.({ block: "nearest" });
    }, 0);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    // Arrow/Enter navigation belongs to the search field. Let a focused tab
    // or result button keep its native keyboard activation.
    if (event.target !== inputRef) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      selectedIndex = itemCount ? Math.min(selectedIndex + 1, itemCount - 1) : 0;
      scrollSelected();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
      scrollSelected();
    } else if (event.key === "Enter") {
      event.preventDefault();
      const item = items[selectedIndex]?.item;
      if (item) choose(item);
    }
  }

  function retryMessages() {
    messageRetry += 1;
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-start justify-center bg-[var(--scrim)] p-4 pt-[10vh]"
    onclick={onClose}
    onkeydown={handleKeydown}
    role="dialog"
    aria-modal="true"
    aria-label={t("palette.searchAll")}
    tabindex="-1"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      role="document"
      class="flex max-h-[min(680px,82vh)] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] shadow-2xl"
      onclick={(event) => event.stopPropagation()}
    >
      <div class="flex items-center gap-3 border-b border-[var(--hairline)] bg-[var(--surface-1)] px-4 py-3">
        <Search class="size-4 shrink-0 text-[var(--text-tertiary)]" />
        <input
          bind:this={inputRef}
          bind:value={query}
          type="text"
          placeholder={t("palette.searchAll")}
          aria-label={t("palette.searchAll")}
          aria-controls="palette-results"
          aria-activedescendant={selectedResult ? `palette-item-${selectedResult.key}` : undefined}
          class="min-w-0 flex-1 border-none bg-transparent text-sm text-[var(--text-primary)] outline-none ring-0 placeholder:text-[var(--text-muted)]"
        />
        {#if query}
          <button
            type="button"
            class="flex h-6 items-center gap-1 rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 text-[11px] text-[var(--text-tertiary)] transition-colors hover:text-[var(--text-primary)]"
            onclick={() => {
              query = "";
              inputRef?.focus();
            }}
            aria-label={t("palette.clear")}
          >
            <X class="size-3" />
            {t("palette.clear")}
          </button>
        {/if}
        <span class="flex items-center gap-1 rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 py-0.5 font-mono text-[11px] text-[var(--text-tertiary)]">
          <Command class="size-3" />
          K
        </span>
      </div>

      <div class="flex items-center gap-1 overflow-x-auto border-b border-[var(--hairline)] px-3 py-2" role="tablist" aria-label={t("palette.searchAll")}>
        {#each tabs as entry (entry.id)}
          <button
            type="button"
            role="tab"
            aria-selected={tab === entry.id}
            class={cn(
              "flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-[11px] font-medium transition-colors",
              tab === entry.id
                ? "border border-[var(--hairline-strong)] bg-[var(--surface-3)] text-[var(--text-primary)]"
                : "border border-transparent text-[var(--text-tertiary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)]",
            )}
            onclick={() => {
              tab = entry.id;
              inputRef?.focus();
            }}
          >
            {t(entry.labelKey as TranslationKey)}
            {#if tabCounts[entry.id] > 0 && entry.id !== "all"}
              <span class="font-mono text-[11px] text-[var(--text-muted)]">{tabCounts[entry.id]}</span>
            {/if}
          </button>
        {/each}
      </div>

      {#if (tab === "all" || tab === "messages") && query.trim().length >= 2 && (messageLoading || messageError)}
        <div class="flex items-center gap-2 border-b border-[var(--hairline)] bg-[var(--surface-0)] px-4 py-1.5 text-[11px] text-[var(--text-muted)]" role="status">
          {#if messageLoading}
            <Loader2 class="size-3 animate-spin" />
            {t("palette.searchingMessages")}
          {:else}
            <span class="text-[var(--danger-text)]">{t("palette.messageSearchFailed")}</span>
            <button
              type="button"
              class="ml-auto flex items-center gap-1 rounded-md border border-[var(--hairline)] px-1.5 py-0.5 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-2)]"
              onclick={retryMessages}
            >
              <RefreshCw class="size-3" />
              {t("palette.retry")}
            </button>
          {/if}
        </div>
      {/if}

      <div id="palette-results" role="listbox" class="min-h-0 flex-1 overflow-y-auto p-2">
        {#if rows.length === 0}
          <div class="flex flex-col items-center justify-center gap-1 px-4 py-16 text-center">
            <span class="flex size-10 items-center justify-center rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-tertiary)]">
              <Search class="size-4" />
            </span>
            {#if tab === "messages" && query.trim().length < 2}
              <p class="mt-1 text-xs font-semibold text-[var(--text-secondary)]">{t("palette.tabMessages")}</p>
              <p class="text-[11px] text-[var(--text-muted)]">{t("palette.messagesHint")}</p>
            {:else if query.trim()}
              <p class="mt-1 text-xs font-semibold text-[var(--text-secondary)]">
                {t("palette.noResults", { query: query.trim() })}
              </p>
              <p class="text-[11px] text-[var(--text-muted)]">{t("palette.noResultsHint")}</p>
            {:else}
              <p class="mt-1 text-xs font-semibold text-[var(--text-secondary)]">{t("palette.noMatches")}</p>
              <p class="text-[11px] text-[var(--text-muted)]">{t("palette.noMatchesHint")}</p>
            {/if}
          </div>
        {:else}
          {#each rows as row (row.key)}
            {#if row.type === "header"}
              <div class="flex items-center justify-between px-2 pb-1 pt-3 font-mono text-[11px] uppercase tracking-[0.12em] text-[var(--text-muted)]" role="presentation">
                <span>{row.label}</span>
                <span class="text-[var(--text-faint)]">{row.count}</span>
              </div>
            {:else}
              {@const item = row.item}
              {@const Icon = item.icon}
              {@const selected = selectedIndex === row.index}
              <button
                type="button"
                id={`palette-item-${item.key}`}
                role="option"
                aria-selected={selected}
                class={cn(
                  "flex w-full items-center gap-2.5 rounded-xl border px-2.5 py-2 text-left transition-colors",
                  selected
                    ? "border-[var(--hairline-strong)] bg-[var(--surface-3)] text-[var(--text-primary)] shadow-sm"
                    : "border-transparent text-[var(--text-secondary)] hover:bg-[var(--surface-2)]",
                )}
                onclick={() => choose(item)}
                onmouseenter={() => (selectedIndex = row.index)}
              >
                <span
                  class={cn(
                    "flex size-7 shrink-0 items-center justify-center rounded-lg border",
                    selected
                      ? "border-[var(--hairline-strong)] bg-[var(--surface-2)] text-[var(--text-primary)]"
                      : "border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-tertiary)]",
                  )}
                >
                  <Icon class="size-3.5" strokeWidth={1.8} />
                </span>
                <span class="flex min-w-0 flex-1 flex-col">
                  <span class="flex min-w-0 items-center gap-2">
                    <span class="truncate text-xs font-semibold text-[var(--text-primary)]">
                      {#each highlightParts(item.title, query) as part}
                        <span class={part.match ? "rounded-sm bg-[var(--brand-soft)] px-0.5 text-[var(--brand-text)]" : ""}>{part.text}</span>
                      {/each}
                    </span>
                    {#if item.badge}
                      <span class="shrink-0 rounded-full border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 py-0.5 font-mono text-[11px] uppercase text-[var(--text-muted)]">
                        {item.badge}
                      </span>
                    {/if}
                  </span>
                  <span class="truncate text-[11px] text-[var(--text-muted)]">
                    {#each highlightParts(item.subtitle, query) as part}
                      <span class={part.match ? "text-[var(--text-secondary)]" : ""}>{part.text}</span>
                    {/each}
                  </span>
                </span>
                <CornerDownLeft class={cn("size-3.5 shrink-0", selected ? "text-[var(--brand-text)]" : "text-transparent")} />
              </button>
            {/if}
          {/each}
        {/if}
      </div>

      <div class="flex items-center justify-between gap-3 border-t border-[var(--hairline)] bg-[var(--surface-0)] px-4 py-2 text-[11px] text-[var(--text-tertiary)]">
        <div class="flex items-center gap-3">
          <span class="flex items-center gap-1">
            <kbd class="rounded border border-[var(--hairline)] bg-[var(--surface-2)] px-1 py-0.5 font-mono text-[11px]">↑</kbd>
            <kbd class="rounded border border-[var(--hairline)] bg-[var(--surface-2)] px-1 py-0.5 font-mono text-[11px]">↓</kbd>
            {t("palette.navigate")}
          </span>
          <span class="flex items-center gap-1">
            <kbd class="rounded border border-[var(--hairline)] bg-[var(--surface-2)] px-1 py-0.5 font-mono text-[11px]">↵</kbd>
            {t("palette.select")}
          </span>
          <span class="flex items-center gap-1">
            <kbd class="rounded border border-[var(--hairline)] bg-[var(--surface-2)] px-1 py-0.5 font-mono text-[11px]">esc</kbd>
            {t("palette.dismiss")}
          </span>
        </div>
        <span class="font-mono text-[11px] uppercase tracking-[0.12em] text-[var(--text-muted)]">RAVENBOT Search</span>
      </div>
    </div>
  </div>
{/if}
