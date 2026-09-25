<script lang="ts">
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ChatRoomList from "$lib/components/ChatRoomList.svelte";
  import { t } from "$lib/i18n";
  import { workspace } from "$lib/workspace.svelte";
  import type { ThemeDefinition } from "$lib/theme";

  let { theme }: { theme: ThemeDefinition } = $props();
</script>

<aside
  class="min-w-0 overflow-hidden flex flex-col border-r border-[var(--hairline-faint)] bg-[var(--surface-0)]"
  aria-label={t("sidebar.fleetNavigator")}
>
  <div class="h-[38px] shrink-0 px-3 flex items-center border-b border-[var(--hairline-faint)]">
    <span class="text-xs font-semibold tracking-tight text-[var(--text-primary)] truncate">
      {theme.brand.brandTitle}<span class="text-[var(--brand-text)]">{theme.brand.brandAccent}</span>
    </span>
  </div>

  <div class="flex-1 overflow-hidden">
    {#if workspace.dest === "offices"}
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
      />
    {/if}
  </div>
</aside>
