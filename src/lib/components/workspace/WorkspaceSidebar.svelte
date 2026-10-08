<script lang="ts">
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ChatRoomList from "$lib/components/ChatRoomList.svelte";
  import { t } from "$lib/i18n";
  import { workspace, RAIL_WIDTH } from "$lib/workspace.svelte";
  import { fleetActivity } from "$lib/fleetActivity.svelte";
  import { getDiceBearUrl, DEFAULT_AVATAR_STYLE } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import { Settings } from "@lucide/svelte";
  import type { ThemeDefinition } from "$lib/theme";

  let { theme }: { theme: ThemeDefinition } = $props();

  const activityDot = (a: string) =>
    a === "working"
      ? "bg-[var(--brand)] animate-pulse"
      : a === "attention"
        ? "bg-[var(--warning-text)] animate-pulse"
        : a === "responded"
          ? "bg-[var(--success-text)]"
          : "";

  // Compact (88px) strip mirrors the same selection-as-router data the full
  // panel renders — icon rows only, per OpenBot auto-compact.
  const compactItems = $derived(
    workspace.dest === "offices"
      ? workspace.chatrooms.map((r: any) => ({
          id: r.id,
          name: r.name || "",
          avatar: r.avatar_url || getDiceBearUrl(r.name || "room", DEFAULT_AVATAR_STYLE),
          activity: "idle",
          selected: workspace.selectedRoomId === r.id,
          select: () => workspace.selectRoom(r.id),
        }))
      : workspace.bots.map((b: any) => ({
          id: b.id,
          name: b.name || "",
          avatar: b.avatar_url || getDiceBearUrl(b.name || "bot", b.avatar_style || DEFAULT_AVATAR_STYLE),
          activity: fleetActivity.get(b.id),
          selected: workspace.selectedBotId === b.id,
          select: () => workspace.selectBot(b.id),
        })),
  );

  // AccountDock (OpenBot bottom-left chip): who this conversation is with.
  const isRooms = $derived(workspace.dest === "offices");
  const dockTarget = $derived(isRooms ? workspace.selectedRoom : workspace.selectedBot);
  const dockAvatar = $derived(
    dockTarget
      ? dockTarget.avatar_url ||
          getDiceBearUrl(dockTarget.name || "chat", dockTarget.avatar_style || DEFAULT_AVATAR_STYLE)
      : "",
  );
  const dockActivity = $derived(
    dockTarget && !isRooms ? fleetActivity.get(dockTarget.id) : "idle",
  );


</script>

<aside
  class="relative min-w-0 overflow-hidden flex flex-col border-r border-[var(--hairline-faint)] bg-[var(--surface-0)]"
  aria-label={t("sidebar.fleetNavigator")}
>
  <div class="h-[38px] shrink-0 px-3 flex items-center border-b border-[var(--hairline-faint)]">
    <span class="text-xs font-semibold tracking-tight text-[var(--text-primary)] truncate">
      {theme.brand.brandTitle}<span class="text-[var(--brand-text)]">{theme.brand.brandAccent}</span>
    </span>
  </div>

  {#if workspace.sidebarCompact}
    <div class="flex-1 overflow-y-auto no-scrollbar py-2 flex flex-col items-center gap-1.5">
      {#each compactItems as it (it.id)}
        <button
          type="button"
          class={cn(
            "relative size-11 rounded-xl overflow-hidden shrink-0 cursor-pointer transition-opacity",
            it.selected
              ? "ring-2 ring-[var(--brand)]"
              : "opacity-75 hover:opacity-100"
          )}
          onclick={it.select}
          title={it.name}
          aria-label={it.name}
          aria-current={it.selected ? "true" : undefined}
        >
          <img src={it.avatar} alt="" class="size-full object-cover" loading="lazy" />
          {#if it.activity !== "idle"}
            <span class={cn("absolute bottom-0.5 right-0.5 size-2 rounded-full ring-2 ring-[var(--surface-0)]", activityDot(it.activity))}></span>
          {/if}
        </button>
      {/each}
    </div>
  {:else}
    <div class="flex-1 overflow-hidden">
      {#if isRooms}
        <ChatRoomList
          bots={workspace.bots}
          selectedRoomId={workspace.selectedRoomId}
          onSelectRoom={(id) => workspace.selectRoom(id)}
          onRoomCreated={(room) => workspace.handleRoomCreated(room)}
        />
      {:else}
        <Sidebar
          bots={workspace.bots}
          selectedBotId={workspace.selectedBotId}
          onSelectBot={(id) => workspace.selectBot(id)}
          onBotCreated={(bot) => workspace.handleBotCreated(bot)}
          onBotUpdated={(bot) => workspace.handleBotUpdated(bot)}
          onBotDeleted={(id) => workspace.handleBotDeleted(id)}
          openSettings={() => workspace.openSettings()}
          onNewChat={() => workspace.newChat()}
          onReorder={(ids) => workspace.reorderBots(ids)}
        />
      {/if}
    </div>
  {/if}

  <!-- AccountDock: current conversation chip + settings shortcut -->
  <div class="shrink-0 h-10 px-2 border-t border-[var(--hairline-faint)] flex items-center gap-2">
    {#if dockTarget}
      <span class="relative size-6 shrink-0">
        <img src={dockAvatar} alt="" class="size-6 rounded-lg object-cover border border-[var(--hairline)]" />
        {#if dockActivity !== "idle"}
          <span class={cn("absolute -bottom-0.5 -right-0.5 size-1.5 rounded-full ring-2 ring-[var(--surface-0)]", activityDot(dockActivity))}></span>
        {/if}
      </span>
      {#if !workspace.sidebarCompact}
        <span class="flex-1 min-w-0 text-[11px] font-semibold text-[var(--text-primary)] truncate">{dockTarget.name}</span>
      {/if}
    {:else if !workspace.sidebarCompact}
      <span class="flex-1 min-w-0 text-[11px] text-[var(--text-muted)] truncate">{t("rail.home")}</span>
    {/if}
    <button
      type="button"
      class="size-7 shrink-0 {workspace.sidebarCompact ? 'mx-auto' : 'ml-auto'} rounded-lg flex items-center justify-center text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)] cursor-pointer"
      style="transition-duration: var(--duration-hover)"
      onclick={() => workspace.openSettings()}
      title={t("rail.settings")}
      aria-label={t("rail.settings")}
    >
      <Settings class="size-3.5" strokeWidth={1.5} />
    </button>
  </div>
</aside>
