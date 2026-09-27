<script lang="ts">
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { getDiceBearUrl } from "$lib/utils";
  import { officeTemplateDesc } from "$lib/catalogI18n";
  import { t } from "$lib/i18n";
  import CreateChatRoom from "$lib/components/CreateChatRoom.svelte";
  import {
    Building2,
    Plus,
    Laptop,
    TrendingUp,
    Briefcase,
    Palette,
    Radio,
  } from "@lucide/svelte";

  interface Props {
    bots: any[];
    selectedRoomId: string | null;
    onSelectRoom: (id: string) => void;
    onRoomCreated: (room: any) => void;
  }

  let { bots = [], selectedRoomId, onSelectRoom, onRoomCreated }: Props = $props();

  let rooms = $state<any[]>([]);
  let showCreate = $state(false);
  let loading = $state(true);

  const templateIcons: Record<string, any> = {
 "it-office": Laptop,
 "marketing": TrendingUp,
 "sales": Briefcase,
 "design": Palette,
 "custom": Building2,
  };

  async function load() {
    try {
      rooms = await invoke("list_chatrooms");
    } catch (e) {
      console.error("Failed to load chatrooms:", e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    load();
  });

  $effect(() => {
    void bots.length;
    load();
  });
</script>

<div class="flex flex-col h-full overflow-hidden select-none">
  <!-- Section Header -->
  <div class="px-3 pt-3 pb-2 flex items-center justify-between">
    <div class="flex items-center gap-2">
      <div class="size-5 rounded-md bg-[var(--brand-soft)] border border-[var(--brand)]/50 flex items-center justify-center text-[var(--brand-text)]">
        <Building2 class="size-3.5" />
      </div>
      <span class="font-bold text-[11px] tracking-wider uppercase text-[var(--text-primary)]">{t("office.title")}</span>
      <span class="bg-[var(--surface-2)] text-[var(--text-tertiary)] text-[10px] font-mono font-medium px-2 py-0.5 rounded-full border border-[var(--hairline)]">
        {rooms.length}
      </span>
    </div>

    <button
      type="button"
      class="size-7 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] transition-colors"
      aria-label={t("office.createTip")}
      onclick={() => (showCreate = true)}
      title={t("office.createTip")}
    >
      <Plus class="size-3.5" />
    </button>
  </div>

  <!-- Office List -->
  <div class="flex-1 overflow-y-auto no-scrollbar px-3 py-2 space-y-2">
    {#if loading}
      <div class="space-y-2">
        {#each [1, 2, 3] as _}
          <div class="p-3 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] space-y-2">
            <div class="flex items-center gap-3">
              <Skeleton class="size-11 rounded-full bg-[var(--surface-3)]" />
              <div class="space-y-1.5 flex-1">
                <Skeleton class="h-3.5 w-3/4 bg-[var(--surface-3)] rounded-md" />
                <Skeleton class="h-2.5 w-1/2 bg-[var(--surface-3)] rounded-md" />
              </div>
            </div>
          </div>
        {/each}
      </div>
    {:else if rooms.length === 0}
      <div class="p-6 text-center border border-dashed border-[var(--hairline)] rounded-2xl my-4 bg-[var(--surface-1)]/50">
        <div class="size-10 rounded-2xl bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] flex items-center justify-center mx-auto mb-3">
          <Building2 class="size-5" />
        </div>
        <h4 class="font-bold text-xs text-[var(--text-primary)]">{t("office.emptyTitle")}</h4>
        <p class="text-[11px] text-[var(--text-muted)] mt-1 leading-relaxed">
          {t("office.emptyDesc")}
        </p>
        <Button class="mt-3.5 h-7 text-xs gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium" size="sm" onclick={() => (showCreate = true)}>
          <Plus class="size-3" />
          {t("office.createFirst")}
        </Button>
      </div>
    {:else}
      {#each rooms as room (room.id)}
        {@const IconComponent = templateIcons[room.office_template] || Building2}
        {@const isSelected = selectedRoomId === room.id}
        <button
          type="button"
          class="w-full text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 rounded-2xl transition-all cursor-pointer block"
          aria-current={isSelected ? "true" : undefined}
          onclick={() => onSelectRoom(room.id)}
        >
          <div
            class="p-3 rounded-2xl border transition-all {isSelected
              ? 'border-[var(--brand)]/80 bg-[var(--brand-soft)]'
              : 'border-[var(--hairline)] bg-[var(--surface-2)]/80 hover:border-[var(--brand)]/40 hover:bg-[var(--surface-2)]'}"
          >
            <div class="flex items-center gap-3">
              <div class="size-11 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--hairline)] shrink-0">
                <RavenAvatar name={room.name} imageUrl={room.avatar_url} />
              </div>

              <div class="flex-1 min-w-0">
                <div class="flex items-center justify-between">
                  <span class="font-bold text-sm text-[var(--text-primary)] truncate">{room.name}</span>
                  <div class="size-5 rounded-md bg-[var(--surface-3)] border border-[var(--hairline)] flex items-center justify-center text-[var(--brand-text)] shrink-0">
                    <IconComponent class="size-3" />
                  </div>
                </div>
                <p class="text-[11px] text-[var(--text-tertiary)] truncate mt-0.5">
                  {room.description || officeTemplateDesc(room.office_template)}
                </p>
                <div class="flex items-center gap-2 mt-1.5">
                  <span class="text-[9px] px-1.5 py-[2px] rounded bg-[var(--surface-3)] border border-[var(--hairline)] text-[var(--text-secondary)] font-mono capitalize">
                    {room.office_template.replace("-", " ")}
                  </span>
                  <span class="text-[10px] text-success flex items-center gap-1 font-mono">
                    <Radio class="size-2.5" />
                    Parallel
                  </span>
                </div>
              </div>
            </div>
          </div>
        </button>
      {/each}
    {/if}
  </div>
</div>

<CreateChatRoom
  open={showCreate}
  onClose={() => (showCreate = false)}
  onCreated={(room) => {
    rooms = [room, ...rooms];
    onRoomCreated(room);
    onSelectRoom(room.id);
  }}
  {bots}
/>
