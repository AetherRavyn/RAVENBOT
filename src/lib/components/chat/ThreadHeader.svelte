<script lang="ts">
  /**
   * The conversation header bar.
   *
   * The owning view keeps every piece of state: open popovers and panels, the
   * model switcher catalog, the thread list and worktree flag, run status, and
   * all handlers. This component renders the header and forwards actions.
   */
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import type { AvatarMood } from "$lib/avatar";
  import type { OpenBotActivity } from "$lib/openBotAvatar";
  import ModelPicker from "$lib/components/ModelPicker.svelte";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import ThreadSearch from "./ThreadSearch.svelte";
  import { Button } from "$lib/components/ui/button";
  import { t } from "$lib/i18n";
  import type { CatalogModel, CatalogProvider } from "$lib/model-catalog";
  import {
    Boxes,
    Brain,
    Check,
    ChevronDown,
    Clock,
    Cpu,
    DollarSign,
    Ghost,
    GitBranch,
    Hash,
    History,
    Image,
    MessageSquare,
    Monitor,
    MoreHorizontal,
    Pause,
    Pencil,
    Play,
    Plus,
    Search,
    Settings,
    Share2,
    Trash2,
    Users,
    XCircle,
  } from "@lucide/svelte";

  interface ThreadSummary {
    id: string;
    title?: string | null;
  }

  interface ChannelOption {
    id: string;
    name: string;
  }

  interface Props {
    botName: string;
    avatarUrl?: string | null;
    avatarStyle?: string | null;
    /** The live fleet mood, so the header face matches the roster. */
    mood?: AvatarMood;
    /** The current tool activity, when the renderer can show it. */
    activity?: OpenBotActivity | null;
    /** Reduced motion (or an explicit preference) forces a still pose. */
    animated?: boolean;
    statusDot: string;
    engine?: string | null;
    paused?: boolean;
    modelLabel: string;
    modelFacts: string;

    threadTitle: string;
    threadEphemeral?: boolean;
    threads?: ThreadSummary[];
    selectedThreadId?: string | null;
    deleteArmed?: string | null;

    showThreadDropdown?: boolean;
    showModelSwitcher?: boolean;
    threadSearchOpen?: boolean;

    switcherProviders?: CatalogProvider[];
    switcherProviderId?: string;
    modelId?: string;
    modelProvider?: string;
    switcherModels?: CatalogModel[];
    switcherConfigured?: string[];
    switcherLoading?: boolean;
    switcherErrorText?: string | null;
    switcherFooterNote?: string;
    switcherReasonFor: (provider: CatalogProvider) => string | null;
    switcherModel?: string;

    threadSearchQuery?: string;
    threadSearchMatchCount?: number;
    threadSearchPosition?: number;
    threadSearchInput?: HTMLInputElement | null;

    sessionCost: number;
    showCostInfo?: boolean;
    showThreadDrawer?: boolean;
    showComputer?: boolean;
    computerPaused?: boolean;
    channelOptions?: ChannelOption[];
    activeChannelId?: string | null;
    showChannels?: boolean;
    showTeamImport?: boolean;
    canShare?: boolean;
    shareOpen?: boolean;
    showRoutines?: boolean;
    showSync?: boolean;
    showMoreMenu?: boolean;
    hasActiveRun?: boolean;

    worktreeEnabled?: boolean;
    worktreeBusy?: boolean;

    onToggleModelSwitcher: () => void;
    onSelectSwitcherProvider: (id: string) => void;
    onSelectSwitcherModel: (id: string) => void;
    onApplySwitcherModel: () => void;

    onToggleThreadDropdown: () => void;
    onSelectThread: (id: string) => void;
    onCreateThread: () => void;
    onBeginRenameThread: (thread: ThreadSummary) => void;
    onDeleteThread: (id: string) => void;
    onToggleWorktree: () => void;

    onOpenSearch: () => void;
    onCloseSearch: () => void;
    onQueryChange: (value: string) => void;
    onStepSearch: (delta: number) => void;

    onToggleTelemetry: () => void;
    onOpenIntelligence: () => void;
    onToggleThreadDrawer: () => void;
    onOpenComputer: () => void;
    onOpenBackdrop: () => void;
    onChannelChange: (id: string | null) => void;
    onOpenChannels: () => void;
    onOpenTeamImport: () => void;
    onOpenShare: () => void;
    onOpenRoutines: () => void;
    onOpenSync: () => void;
    onOpenSettings: () => void;
    onToggleMoreMenu: () => void;
    onCloseMoreMenu: () => void;
    onStopRun: () => void;
    onTogglePause: () => void;
  }

  let {
    botName,
    avatarUrl = null,
    avatarStyle = null,
    mood = "idle",
    activity = null,
    animated = true,
    statusDot,
    engine = null,
    paused = false,
    modelLabel,
    modelFacts,
    threadTitle,
    threadEphemeral = false,
    threads = [],
    selectedThreadId = null,
    deleteArmed = null,
    showThreadDropdown = false,
    showModelSwitcher = false,
    threadSearchOpen = false,
    switcherProviders = [],
    switcherProviderId = "",
    modelId = "",
    modelProvider = "",
    switcherModels = [],
    switcherConfigured = [],
    switcherLoading = false,
    switcherErrorText = null,
    switcherFooterNote,
    switcherReasonFor,
    switcherModel = $bindable(""),
    threadSearchQuery = "",
    threadSearchMatchCount = 0,
    threadSearchPosition = 0,
    threadSearchInput = $bindable(null),
    sessionCost,
    showCostInfo = false,
    showThreadDrawer = false,
    showComputer = false,
    computerPaused = false,
    channelOptions = [],
    activeChannelId = null,
    showChannels = false,
    showTeamImport = false,
    canShare = false,
    shareOpen = false,
    showRoutines = false,
    showSync = false,
    showMoreMenu = false,
    hasActiveRun = false,
    worktreeEnabled = false,
    worktreeBusy = false,
    onToggleModelSwitcher,
    onSelectSwitcherProvider,
    onSelectSwitcherModel,
    onApplySwitcherModel,
    onToggleThreadDropdown,
    onSelectThread,
    onCreateThread,
    onBeginRenameThread,
    onDeleteThread,
    onToggleWorktree,
    onOpenSearch,
    onCloseSearch,
    onQueryChange,
    onStepSearch,
    onToggleTelemetry,
    onOpenIntelligence,
    onToggleThreadDrawer,
    onOpenComputer,
    onOpenBackdrop,
    onChannelChange,
    onOpenChannels,
    onOpenTeamImport,
    onOpenShare,
    onOpenRoutines,
    onOpenSync,
    onOpenSettings,
    onToggleMoreMenu,
    onCloseMoreMenu,
    onStopRun,
    onTogglePause,
  }: Props = $props();
</script>

  <header class="h-[46px] px-4 border-b border-[var(--hairline)] bg-[var(--surface-0)] flex items-center justify-between gap-3 z-20 shrink-0">
    <!-- Left: Bot Avatar & Info + Thread Switcher Dropdown -->
    <div class="flex items-center gap-3 min-w-0 flex-1">
      <div class="relative size-8 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline)] p-0.5 shrink-0 shadow-sm">
        <RavenAvatar
          name={botName}
          mood={mood}
          activity={activity}
          animated={animated}
          imageUrl={avatarUrl}
          style={avatarStyle}
        />
        <span class="absolute bottom-0 right-0 size-2 rounded-full ring-1 ring-[var(--surface-0)] {statusDot}"></span>
      </div>

      <div class="flex items-center gap-2 min-w-0">
        <span class="font-bold text-sm text-[var(--text-primary)] truncate">{botName}</span>

        {#if engine && engine !== "native"}
          <span
            class="thread-header-engine text-[11px] font-mono py-0.5 px-2 rounded-md bg-[var(--brand-soft)] border border-[var(--brand)]/30 text-[var(--brand-text)] hidden sm:inline-flex items-center gap-1"
            title={t("thread.cliBot")}
          >
            <Cpu class="size-2.5" /> {engine}
          </span>
        {/if}

        <!-- Model Quick Switcher (clickable pill, Grok-style) -->
        <div class="relative thread-header-model">
          <button
            type="button"
            class="text-[11px] font-mono py-0.5 px-2 rounded-md bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] truncate cursor-pointer transition-colors hidden sm:inline-flex items-center gap-1 max-w-[220px] {engine && engine !== 'native' ? 'opacity-40' : ''}"
            onclick={(e) => {
              e.stopPropagation();
              onToggleModelSwitcher();
            }}
            aria-expanded={showModelSwitcher}
            aria-haspopup="dialog"
            title={`${t("thread.switchModel")}${modelFacts ? " · " + modelFacts : ""}`}
          >
            <span class="truncate">{modelLabel}</span>
            <ChevronDown class="size-2.5 shrink-0" />
          </button>

          {#if showModelSwitcher}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="absolute left-0 top-7 z-50 w-[26rem] max-w-[calc(100vw-2rem)] bg-[var(--surface-1)] border border-[var(--hairline)] rounded-2xl shadow-2xl animate-in fade-in zoom-in-95  overflow-hidden"
              onclick={(e) => e.stopPropagation()}
            >
              <span class="block px-3 py-2 text-[11px] font-bold text-[var(--text-tertiary)] uppercase tracking-wider font-mono border-b border-[var(--hairline)]">
                Switch Model · {modelProvider}/{modelId.split("/").pop()}
              </span>

              <div class="px-2.5 pt-2 pb-2">
                <ModelPicker
                  providers={switcherProviders}
                  provider={switcherProviderId}
                  model={modelId}
                  models={switcherModels}
                  configured={switcherConfigured}
                  loading={switcherLoading}
                  error={switcherErrorText}
                  reasonFor={switcherReasonFor}
                  onSelectProvider={onSelectSwitcherProvider}
                  onSelectModel={onSelectSwitcherModel}
                  heightClass="max-h-[18rem]"
                  footerNote={switcherFooterNote}
                />
              </div>

              <!-- Custom model id -->
              <div class="flex items-center gap-1.5 pt-1 border-t border-[var(--hairline)] mt-1">
                <input
                  bind:value={switcherModel}
                  aria-label={t("a11y.customModelId")}
                  placeholder="model id…"
                  class="flex-1 min-w-0 h-6 px-2 rounded-lg bg-[var(--surface-0)] border border-[var(--hairline)] text-[11px] font-mono text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 focus:border-[var(--brand)]/50"
                  onkeydown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      e.stopPropagation();
                      onApplySwitcherModel();
                    }
                  }}
                />
                <Button
                  size="sm"
                  class="h-6 px-2 text-[11px] bg-[var(--surface-light)] text-[var(--text-on-light)] hover:bg-[var(--surface-light-hover)] cursor-pointer shrink-0"
                  onclick={() => onApplySwitcherModel()}
                >
                  Apply
                </Button>
              </div>
            </div>
          {/if}
        </div>

        {#if threadEphemeral}
          <span class="text-[11px] font-mono py-0.5 px-2 rounded-md bg-warning/15 border border-warning/30 text-warning flex items-center gap-1 shrink-0" title={t("thread.tempTip")}>
            <Ghost class="size-3" />
            <span>{t("thread.temp")}</span>
          </span>
        {/if}
      </div>

      <!-- Thread Switcher Dropdown -->
      <div class="relative ml-2 min-w-0 shrink">
        <button
          type="button"
          class="h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] text-xs text-[var(--text-secondary)] flex items-center gap-1.5 cursor-pointer font-medium transition-colors min-w-0"
          onclick={(e) => {
            e.stopPropagation();
            onToggleThreadDropdown();
          }}
          aria-expanded={showThreadDropdown}
          title={t("thread.switchThread")}
        >
          <MessageSquare class="size-3 text-[var(--brand-text)] shrink-0" />
          <span class="min-w-0 max-w-[130px] truncate text-[11px] font-mono">
            {threadTitle}
          </span>
          <ChevronDown class="size-3 text-[var(--text-tertiary)] shrink-0" />
        </button>

        {#if showThreadDropdown}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute left-0 top-9 z-50 w-64 bg-[var(--surface-1)] border border-[var(--hairline)] rounded-2xl shadow-2xl p-2 space-y-1 animate-in fade-in zoom-in-95 "
            onclick={(e) => e.stopPropagation()}
          >
            <div class="flex items-center justify-between px-2 py-1 border-b border-[var(--hairline)]">
              <span class="text-[11px] font-bold text-[var(--text-tertiary)] uppercase tracking-wider font-mono">{t("thread.chatHistory")}</span>
              <button
                type="button"
                class="text-[11px] text-[var(--brand-text)] hover:text-[var(--brand-hover)] flex items-center gap-1 cursor-pointer"
                onclick={() => onCreateThread()}
              >
                <Plus class="size-3" /> New
              </button>
            </div>

            <div class="max-h-60 overflow-y-auto space-y-0.5 py-1">
              {#each threads as thread (thread.id)}
                {@const isSelected = selectedThreadId === thread.id}
                <div
                  class="group/thread flex items-center gap-0.5 rounded-xl border transition-colors {isSelected
                    ? 'bg-[var(--surface-3)] text-[var(--text-primary)] border-[var(--hairline-strong)]'
                    : 'border-transparent text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                >
                  <button
                    type="button"
                    class="min-w-0 flex-1 truncate px-2.5 py-1.5 text-left text-xs cursor-pointer"
                    onclick={() => onSelectThread(thread.id)}
                    aria-pressed={isSelected}
                  >
                    <span class="truncate">{thread.title || t("thread.untitled")}</span>
                  </button>
                  <div class="flex items-center gap-0.5 pr-1">
                    <button
                      type="button"
                      class="flex size-6 items-center justify-center rounded-md text-[var(--text-muted)] opacity-70 hover:bg-[var(--surface-1)] hover:text-[var(--text-primary)] hover:opacity-100 focus:opacity-100 cursor-pointer"
                      onclick={() => {
                        onBeginRenameThread(thread);
                      }}
                      aria-label={t("thread.rename")}
                      title={t("thread.rename")}
                    >
                      <Pencil class="size-3" />
                    </button>
                    <button
                      type="button"
                      class="flex size-6 items-center justify-center rounded-md opacity-70 hover:opacity-100 cursor-pointer {deleteArmed === thread.id
                        ? 'bg-[var(--danger-soft)] text-[var(--danger-text)]'
                        : 'text-[var(--text-muted)] hover:bg-[var(--danger-soft)] hover:text-[var(--danger-text)]'}"
                      onclick={() => onDeleteThread(thread.id)}
                      aria-label={deleteArmed === thread.id
                        ? t("thread.confirmDelete")
                        : t("thread.delete")}
                      title={deleteArmed === thread.id
                        ? t("thread.confirmDelete")
                        : t("thread.delete")}
                    >
                      {#if deleteArmed === thread.id}
                        <Check class="size-3" />
                      {:else}
                        <Trash2 class="size-3" />
                      {/if}
                    </button>
                  </div>
                </div>
              {:else}
                <div class="p-3 text-center text-xs text-[var(--text-muted)]">{t("thread.noPrev")}</div>
              {/each}
            </div>

            <div class="border-t border-[var(--hairline)] px-2 pt-2 pb-1 space-y-1">
              <button
                type="button"
                class="w-full flex items-center gap-2 rounded-xl px-2 py-1.5 text-xs transition-colors cursor-pointer disabled:opacity-50 {worktreeEnabled
                  ? 'text-[var(--brand-text)]'
                  : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
                onclick={() => onToggleWorktree()}
                disabled={!selectedThreadId || worktreeBusy}
                aria-pressed={Boolean(worktreeEnabled)}
                title="Requires a clean Git repository with one project folder"
              >
                <GitBranch class="size-3.5 shrink-0" />
                <span class="min-w-0 flex-1 text-left">Isolated Git worktree</span>
                <span class="font-mono text-[11px]">{worktreeEnabled ? "On" : "Off"}</span>
              </button>
              <p class="px-2 text-[11px] leading-snug text-[var(--text-muted)]">
                Runs write to a separate checkout until you review and apply them. Requires a clean Git repository; ignored local files are copied for reading, not applied.
              </p>
            </div>
          </div>
        {/if}
      </div>

      <!-- Quick New Thread Button -->
      <button
        type="button"
        class="h-7 px-2 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] text-xs text-[var(--text-secondary)] flex items-center gap-1 cursor-pointer transition-colors shrink-0"
        onclick={() => onCreateThread()}
        title={t("thread.newThread")}
      >
        <Plus class="size-3.5" />
        <span class="hidden md:inline text-[11px]">{t("thread.new")}</span>
      </button>
    </div>

    <!-- Right Header Controls -->
    <div class="flex items-center gap-2 shrink-0">
      <!-- In-thread search -->
      <div class="relative">
        <button
          type="button"
          class="h-7 px-2 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-xs flex items-center gap-1.5 transition-colors cursor-pointer {threadSearchOpen
            ? 'text-[var(--brand-text)] border-[var(--brand)]/40 bg-[var(--brand-soft)]'
            : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
          onclick={() => (threadSearchOpen ? onCloseSearch() : onOpenSearch())}
          aria-expanded={threadSearchOpen}
          aria-label={t("thread.searchInThread")}
          title={t("thread.searchInThread")}
        >
          <Search class="size-3.5" />
          <span class="hidden lg:inline text-[11px]">{t("thread.searchShort")}</span>
        </button>

        {#if threadSearchOpen}
          <ThreadSearch
            query={threadSearchQuery}
            matchCount={threadSearchMatchCount}
            position={threadSearchPosition}
            bind:input={threadSearchInput}
            onQueryChange={onQueryChange}
            onStep={onStepSearch}
            onClose={onCloseSearch}
          />
        {/if}
      </div>

      <!-- Secondary actions collapse into the More menu in a narrow
           conversation column. -->
      <div class="thread-header-secondary flex items-center gap-2">
      <!-- Session Telemetry Pill -->
      <button
        type="button"
        class="h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-xs font-mono flex items-center gap-1.5 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={() => onToggleTelemetry()}
        aria-pressed={showCostInfo}
        title={t("thread.telemetry")}
      >
        <DollarSign class="size-3 text-success" />
        <span>${sessionCost.toFixed(4)}</span>
      </button>

      <!-- Agent Intelligence / Skills Button -->
      <button
        type="button"
        class="h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-xs flex items-center gap-1.5 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={() => onOpenIntelligence()}
        title={t("thread.intelligence")}
      >
        <Brain class="size-3 text-[var(--brand-text)] shrink-0" />
        <span class="hidden xl:inline text-[11px]">{botName.split(" ")[0]} Intelligence</span>
      </button>

      <!-- Thread Drawer Toggle Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showThreadDrawer ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => onToggleThreadDrawer()}
        aria-pressed={showThreadDrawer}
        aria-label={t("thread.historyToggle")}
        title={t("thread.historyToggle")}
      >
        <History class="size-3.5" />
      </button>

      <!-- Computer / Desktop Panel Button -->
      <button
        type="button"
        class="relative size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showComputer ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => onOpenComputer()}
        aria-pressed={showComputer}
        aria-label={t("thread.computer")}
        title={computerPaused ? t("computer.takenOver") : t("thread.computer")}
      >
        <Monitor class="size-3.5" />
        {#if computerPaused}
          <span class="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-[var(--warning-text)] ring-2 ring-[var(--surface-2)]" aria-hidden="true"></span>
        {/if}
      </button>

      <!-- Channel (context) picker for new threads -->
      {#if channelOptions.length > 0}
        <SimpleSelect
          value={activeChannelId ?? ""}
          options={[
            { value: "", label: t("thread.noChannel") },
            ...channelOptions.map((c) => ({ value: c.id, label: c.name })),
          ]}
          onValueChange={(v) => onChannelChange(v || null)}
          placeholder={t("thread.noChannel")}
          class="h-7 w-32 rounded-lg text-[11px] font-mono"
        />
      {/if}

      <!-- Channels manager -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showChannels ? 'bg-success/20 text-success border-success/40' : ''}"
        onclick={() => onOpenChannels()}
        aria-pressed={showChannels}
        aria-label={t("thread.channels")}
        title={t("thread.channels")}
      >
        <Hash class="size-3.5" />
      </button>

      <!-- Import a team -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showTeamImport ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => onOpenTeamImport()}
        aria-pressed={showTeamImport}
        aria-label={t("thread.importTeam")}
        title={t("thread.importTeam")}
      >
        <Users class="size-3.5" />
      </button>

      {#if canShare}
        <!-- Read-only public conversation link -->
        <button
          type="button"
          class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {shareOpen ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
          onclick={() => onOpenShare()}
          aria-pressed={shareOpen}
          aria-label={t("thread.shareConversation")}
          title={t("thread.shareConversation")}
        >
          <Share2 class="size-3.5" />
        </button>
      {/if}

      <!-- Routines / Scheduler Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showRoutines ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => onOpenRoutines()}
        aria-pressed={showRoutines}
        aria-label={t("routines.title")}
        title={t("routines.title")}
      >
        <Clock class="size-3.5" />
      </button>

      <!-- Fleet Sync / Backup Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer {showSync ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]/40' : ''}"
        onclick={() => onOpenSync()}
        aria-pressed={showSync}
        aria-label={t("thread.fleetSync")}
        title={t("thread.fleetSync")}
      >
        <Boxes class="size-3.5" />
      </button>

      <!-- Settings Shortcut -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={() => onOpenSettings()}
        aria-label={t("thread.settingsKeys")}
        title={t("thread.settingsKeys")}
      >
        <Settings class="size-3.5" />
      </button>
      </div>

      <!-- Narrow-column overflow: the same actions, one menu. -->
      <div class="relative thread-header-more">
        <button
          type="button"
          class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
          onclick={(event) => {
            event.stopPropagation();
            onToggleMoreMenu();
          }}
          aria-haspopup="menu"
          aria-expanded={showMoreMenu}
          aria-label={t("thread.more")}
          title={t("thread.more")}
        >
          <MoreHorizontal class="size-4" />
        </button>

        {#if showMoreMenu}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute right-0 top-9 z-50 w-64 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-1.5 shadow-2xl animate-in fade-in zoom-in-95"
            role="menu"
            tabindex="-1"
            onclick={(event) => event.stopPropagation()}
            onkeydown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                onCloseMoreMenu();
              }
            }}
          >
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onToggleThreadDrawer();
              }}
            >
              <History class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.historyToggle")}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenComputer();
              }}
            >
              <Monitor class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.computer")}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenBackdrop();
              }}
            >
              <Image class="size-3.5 text-[var(--brand-text)]" />
              {t("backdrop.title")}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenChannels();
              }}
            >
              <Hash class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.channels")}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenTeamImport();
              }}
            >
              <Users class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.importTeam")}
            </button>
            {#if canShare}
              <button
                type="button"
                role="menuitem"
                class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
                onclick={() => {
                  onCloseMoreMenu();
                  onOpenShare();
                }}
              >
                <Share2 class="size-3.5 text-[var(--brand-text)]" />
                {t("thread.shareConversation")}
              </button>
            {/if}
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenRoutines();
              }}
            >
              <Clock class="size-3.5 text-[var(--brand-text)]" />
              {t("routines.title")}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenSync();
              }}
            >
              <Boxes class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.fleetSync")}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenIntelligence();
              }}
            >
              <Brain class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.intelligence")}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onToggleTelemetry();
              }}
            >
              <DollarSign class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.telemetry")}
            </button>
            <div class="my-1 border-t border-[var(--hairline)]"></div>
            <button
              type="button"
              role="menuitem"
              class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => {
                onCloseMoreMenu();
                onOpenSettings();
              }}
            >
              <Settings class="size-3.5 text-[var(--brand-text)]" />
              {t("thread.settingsKeys")}
            </button>

            {#if channelOptions.length > 0}
              <div class="mt-1 border-t border-[var(--hairline)] px-2 pt-2 pb-1">
                <span class="mb-1 block font-mono text-[11px] uppercase tracking-wider text-[var(--text-muted)]">
                  {t("thread.channels")}
                </span>
                <SimpleSelect
                  value={activeChannelId ?? ""}
                  options={[
                    { value: "", label: t("thread.noChannel") },
                    ...channelOptions.map((c) => ({ value: c.id, label: c.name })),
                  ]}
                  onValueChange={(value) => onChannelChange(value || null)}
                  placeholder={t("thread.noChannel")}
                  class="h-7 w-full rounded-lg text-[11px] font-mono"
                />
              </div>
            {/if}
          </div>
        {/if}
      </div>

      {#if hasActiveRun}
        <button
          type="button"
          class="size-7 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] flex items-center justify-center text-[var(--danger-text)] hover:bg-[var(--danger-soft-hover)] transition-colors cursor-pointer"
          onclick={() => onStopRun()}
          aria-label={t("a11y.stopRun")}
          title="Stop the active run"
        >
          <XCircle class="size-3.5" />
        </button>
      {/if}

      <!-- Pause / Play Agent -->
      <button
        type="button"
        class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors cursor-pointer"
        onclick={() => onTogglePause()}
        aria-label={paused ? "Resume agent" : "Pause all agents"}
        aria-pressed={paused}
        title={paused ? "Resume agent" : "Pause all agents (kill switch)"}
      >
        {#if paused}
          <Play class="size-3.5 fill-current text-[var(--brand-text)]" />
        {:else}
          <Pause class="size-3.5" />
        {/if}
      </button>
    </div>
  </header>
