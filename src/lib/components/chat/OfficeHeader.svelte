<script lang="ts">
  /**
   * The office header band: who this office is, what it is doing, and who is
   * in it.
   *
   * The room owns the autonomy toggle, room search, manage sheet and every
   * panel these buttons open. This component renders the identity, the live
   * status meta and the roster, and forwards each action with the member it
   * belongs to. The search input ref is bindable because opening the search
   * from the room focuses it on the next tick.
   */
  import {
    Brain,
    ChevronDown,
    ChevronUp,
    Pause,
    Search,
    Settings,
    Users,
    Workflow,
    X,
    XCircle,
  } from "@lucide/svelte";
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { t } from "$lib/i18n";
  import { cn } from "$lib/utils.js";
  import type { AvatarMood } from "$lib/avatar";
  import type { Activity } from "$lib/fleetActivity.svelte";

  interface OfficeMember {
    rank: string;
    specialty?: string;
    bot?: {
      id?: string;
      name?: string;
      avatar_url?: string | null;
      avatar_style?: string | null;
    } | null;
  }

  interface Props {
    roomName: string;
    roomAvatarUrl?: string | null;
    roomAvatarStyle?: string | null;
    officeTemplate?: string | null;
    members: OfficeMember[];
    autonomyOn: boolean;
    autonomyBusy: boolean;
    officeActive: boolean;
    sending: boolean;
    teamActive: boolean;
    officeTokens: number;
    officeCost: number;
    roomSearchOpen: boolean;
    roomSearchQuery: string;
    roomSearchMatchCount: number;
    roomSearchPosition: number;
    roomSearchInput?: HTMLInputElement | null;
    activityFor: (botId: string) => Activity;
    moodFor: (botId: string) => AvatarMood;
    onToggleAutonomy: () => void;
    onStopOffice: () => void;
    onManageAgent: (member: OfficeMember) => void;
    onOpenSearch: () => void;
    onCloseSearch: () => void;
    onSearchInput: (event: Event) => void;
    onStepSearch: (delta: number) => void;
    onHire: () => void;
    onMemory: () => void;
    onPlan: () => void;
    onSettings: () => void;
  }

  let {
    roomName,
    roomAvatarUrl,
    roomAvatarStyle,
    officeTemplate,
    members,
    autonomyOn,
    autonomyBusy,
    officeActive,
    sending,
    teamActive,
    officeTokens,
    officeCost,
    roomSearchOpen,
    roomSearchQuery,
    roomSearchMatchCount,
    roomSearchPosition,
    roomSearchInput = $bindable(null),
    activityFor,
    moodFor,
    onToggleAutonomy,
    onStopOffice,
    onManageAgent,
    onOpenSearch,
    onCloseSearch,
    onSearchInput,
    onStepSearch,
    onHire,
    onMemory,
    onPlan,
    onSettings,
  }: Props = $props();

  function rosterRing(act: Activity): string {
    switch (act) {
      case "working": return "ring-2 ring-[var(--brand)]";
      case "attention": return "ring-2 ring-warning";
      case "responded": return "ring-2 ring-success";
      default: return "ring-1 ring-[var(--brand)]/40";
    }
  }
  function rosterDot(act: Activity): string {
    switch (act) {
      case "working": return "bg-[var(--brand)] animate-pulse";
      case "attention": return "bg-warning animate-pulse";
      case "responded": return "bg-success";
      default: return "";
    }
  }
</script>

<header class="px-4 py-2.5 border-b border-[var(--hairline)] bg-[var(--surface-0)] flex items-center gap-3 z-10 shrink-0">
  <div class="size-9 rounded-xl overflow-hidden bg-[var(--surface-3)] shrink-0">
    <RavenAvatar name={roomName} imageUrl={roomAvatarUrl} style={roomAvatarStyle} />
  </div>

  <div class="min-w-0">
    <div class="flex items-center gap-2 min-w-0">
      <span class="font-semibold text-sm text-[var(--text-primary)] truncate">{roomName}</span>
      <span class="text-[11px] px-1.5 py-0.5 rounded bg-[var(--surface-3)] text-[var(--text-tertiary)] capitalize shrink-0">
        {(officeTemplate || "custom").replace("-", " ")}
      </span>
      <button
        type="button"
        class="text-[11px] px-1.5 py-0.5 rounded border shrink-0 inline-flex items-center gap-1 cursor-pointer transition-colors disabled:opacity-60 {autonomyOn
          ? 'bg-success/10 text-success border-success/20 hover:bg-success/20'
          : 'bg-[var(--surface-3)] text-[var(--text-muted)] border-[var(--hairline)] hover:text-[var(--text-secondary)]'}"
        title={autonomyOn ? t("room.autonomyOnTip") : t("room.autonomyOffTip")}
        aria-pressed={autonomyOn}
        disabled={autonomyBusy}
        onclick={() => void onToggleAutonomy()}
      >
        {#if autonomyOn}
          <span class="size-1.5 rounded-full bg-success" aria-hidden="true"></span>
        {:else}
          <Pause class="size-2.5" strokeWidth={2} />
        {/if}
        {autonomyOn ? t("room.autonomous") : t("room.autonomyPaused")}
      </button>
      {#if officeActive || sending}
        <button
          type="button"
          class="text-[11px] px-1.5 py-0.5 rounded border shrink-0 inline-flex items-center gap-1 cursor-pointer bg-[var(--danger-soft)] text-[var(--danger-text)] border-[var(--danger-border)] hover:bg-[var(--danger-soft-hover)]"
          title="Stop the active office run"
          aria-label="Stop the active office run"
          onclick={() => void onStopOffice()}
        >
          <XCircle class="size-2.5" strokeWidth={2} />
          Stop
        </button>
      {/if}
    </div>
    <!-- One meta line. The lane is a fact about the office, not a status
         claim — the dot carries the status, so an idle office no longer
         wears a green "running" badge. Cost appears only once there is
         some; "$0.0000 · 0 tok" is not information. -->
    <div class="flex items-center gap-1.5 text-[11px] text-[var(--text-muted)] mt-0.5 min-w-0">
      <span class={cn("size-1.5 rounded-full shrink-0", teamActive || officeActive ? "bg-success" : "bg-[var(--text-faint)]")} aria-hidden="true"></span>
      <span class="shrink-0">{t("room.parallelLane")}</span>
      <span class="text-[var(--text-faint)]">·</span>
      <span class="truncate">{members.length === 1 ? t("room.specialistAssigned", { n: members.length }) : t("room.specialistsAssigned", { n: members.length })}</span>
      {#if officeTokens > 0}
        <span class="text-[var(--text-faint)]">·</span>
        <span class="font-mono tabular-nums shrink-0" title={t("room.telemetry")}>
          ${officeCost.toFixed(4)} · {officeTokens.toLocaleString()} tok
        </span>
      {/if}
    </div>
  </div>

  <div class="flex items-center gap-1 ml-auto shrink-0">
    <!-- The roster. The face follows the work, so a glance shows which agent
         is thinking, which needs an answer, and which failed — without
         reading a label. Clicking one opens its manage sheet. -->
    <div class="flex -space-x-1.5 mr-1">
      {#each members.slice(0, 6) as m}
        {@const botId = m.bot?.id ?? ""}
        {@const act = activityFor(botId)}
        <button
          type="button"
          class="relative size-7 shrink-0 rounded-full transition-transform hover:scale-110 hover:z-10 cursor-pointer"
          onclick={() => onManageAgent(m)}
          title={`${m.bot?.name || m.rank} — ${m.specialty} (click to manage)`}
        >
          <RavenAvatar
            name={m.bot?.name || m.rank}
            mood={moodFor(botId)}
            imageUrl={m.bot?.avatar_url}
            style={m.bot?.avatar_style}
            decorative
            class={cn("size-full rounded-full", rosterRing(act))}
          />
          {#if act !== "idle"}
            <span class={cn("absolute -bottom-0.5 -right-0.5 size-2 rounded-full ring-2 ring-[var(--surface-0)]", rosterDot(act))}></span>
          {/if}
        </button>
      {/each}
      {#if members.length > 6}
        <div class="size-7 rounded-full bg-[var(--surface-3)] text-[11px] font-bold text-[var(--text-tertiary)] flex items-center justify-center">
          +{members.length - 6}
        </div>
      {/if}
    </div>

    <div class="relative">
      <button
        type="button"
        onclick={() => (roomSearchOpen ? onCloseSearch() : onOpenSearch())}
        class="icon-btn size-8 {roomSearchOpen ? 'text-[var(--brand-text)] bg-[var(--brand-soft)]' : ''}"
        title={t("thread.searchInThread")}
        aria-label={t("thread.searchInThread")}
        aria-expanded={roomSearchOpen}
      >
        <Search class="size-4" />
      </button>

      {#if roomSearchOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
          class="thread-search"
          role="search"
          onkeydown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              onCloseSearch();
            }
          }}
        >
          <Search class="thread-search__icon size-3.5" aria-hidden="true" />
          <input
            bind:this={roomSearchInput}
            class="thread-search__input"
            type="search"
            value={roomSearchQuery}
            placeholder={t("thread.searchMessages")}
            aria-label={t("thread.searchMessages")}
            oninput={onSearchInput}
            onkeydown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                onStepSearch(event.shiftKey ? -1 : 1);
              }
            }}
          />
          <span class="thread-search__count" aria-live="polite">
            {#if roomSearchMatchCount}
              {t("thread.searchCount", {
                current: roomSearchPosition + 1,
                total: roomSearchMatchCount,
              })}
            {:else if roomSearchQuery.trim()}
              {t("thread.searchNoResults")}
            {/if}
          </span>
          <button
            type="button"
            class="thread-search__button"
            disabled={!roomSearchMatchCount}
            onclick={() => onStepSearch(-1)}
            aria-label={t("thread.searchPrevious")}
            title={t("thread.searchPrevious")}
          >
            <ChevronUp class="size-3.5" />
          </button>
          <button
            type="button"
            class="thread-search__button"
            disabled={!roomSearchMatchCount}
            onclick={() => onStepSearch(1)}
            aria-label={t("thread.searchNext")}
            title={t("thread.searchNext")}
          >
            <ChevronDown class="size-3.5" />
          </button>
          <button
            type="button"
            class="thread-search__button"
            onclick={onCloseSearch}
            aria-label={t("thread.searchClose")}
            title={t("thread.searchClose")}
          >
            <X class="size-3.5" />
          </button>
        </div>
      {/if}
    </div>

    <button type="button" onclick={() => onHire()} class="icon-btn size-8" title={t("room.hire")} aria-label={t("room.hire")}>
      <Users class="size-4" />
    </button>
    <button type="button" onclick={() => onMemory()} class="icon-btn size-8" title={t("memory.title")} aria-label={t("memory.title")}>
      <Brain class="size-4" />
    </button>
    <button type="button" onclick={() => onPlan()} class="icon-btn size-8" title={t("room.plan")} aria-label={t("room.plan")}>
      <Workflow class="size-4" />
    </button>
    <button type="button" onclick={() => onSettings()} class="icon-btn size-8" title={t("room.settings")} aria-label={t("room.settings")}>
      <Settings class="size-4" />
    </button>
  </div>
</header>
