<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { getDiceBearUrl, OFFICE_TEMPLATES } from "$lib/utils";
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
      <div class="size-5 rounded-md bg-purple-950/70 border border-purple-800/50 flex items-center justify-center text-purple-400">
        <Building2 class="size-3.5" />
      </div>
      <span class="font-bold text-[11px] tracking-wider uppercase text-zinc-100">Offices & Pods</span>
      <span class="bg-[#181824] text-zinc-400 text-[10px] font-mono font-medium px-2 py-0.5 rounded-full border border-[#232333]">
        {rooms.length}
      </span>
    </div>

    <button
      type="button"
      class="size-7 rounded-lg border border-[#232334] bg-[#12121d] flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors"
      onclick={() => (showCreate = true)}
      title="Create office"
    >
      <Plus class="size-3.5" />
    </button>
  </div>

  <!-- Office List -->
  <div class="flex-1 overflow-y-auto px-3 py-2 space-y-2">
    {#if loading}
      <div class="space-y-2">
        {#each [1, 2, 3] as _}
          <div class="p-3 rounded-2xl border border-[#1e1e2d] bg-[#0d0d16] space-y-2">
            <div class="flex items-center gap-3">
              <Skeleton class="size-11 rounded-full bg-[#181826]" />
              <div class="space-y-1.5 flex-1">
                <Skeleton class="h-3.5 w-3/4 bg-[#181826] rounded-md" />
                <Skeleton class="h-2.5 w-1/2 bg-[#181826] rounded-md" />
              </div>
            </div>
          </div>
        {/each}
      </div>
    {:else if rooms.length === 0}
      <div class="p-6 text-center border border-dashed border-[#222234] rounded-2xl my-4 bg-[#0d0d16]/50">
        <div class="size-10 rounded-2xl bg-purple-950/40 border border-purple-800/40 text-purple-400 flex items-center justify-center mx-auto mb-3">
          <Building2 class="size-5" />
        </div>
        <h4 class="font-bold text-xs text-white">No offices established</h4>
        <p class="text-[11px] text-zinc-500 mt-1 leading-relaxed">
          Create an IT Office, Growth Pod, or Design Studio.
        </p>
        <Button class="mt-3.5 h-7 text-xs gap-1.5 bg-purple-600 hover:bg-purple-500 text-white font-medium" size="sm" onclick={() => (showCreate = true)}>
          <Plus class="size-3" />
          Create First Office
        </Button>
      </div>
    {:else}
      {#each rooms as room (room.id)}
        {@const tmpl = OFFICE_TEMPLATES[room.office_template as keyof typeof OFFICE_TEMPLATES] || OFFICE_TEMPLATES.custom}
        {@const IconComponent = templateIcons[room.office_template] || Building2}
        {@const isSelected = selectedRoomId === room.id}
        <button
          type="button"
          class="w-full text-left focus:outline-none rounded-2xl transition-all cursor-pointer block"
          onclick={() => onSelectRoom(room.id)}
        >
          <div
            class="p-3 rounded-2xl border transition-all {isSelected
              ? 'border-purple-500/80 bg-[#120f20]/90 shadow-[0_0_25px_rgba(147,51,234,0.18)]'
              : 'border-[#1e1e2c] bg-[#0d0d15]/80 hover:border-purple-500/40 hover:bg-[#12121d]'}"
          >
            <div class="flex items-center gap-3">
              <div class="size-11 rounded-full overflow-hidden bg-[#181826] border border-[#2b2b3d] shrink-0">
                <img
                  src={room.avatar_url || getDiceBearUrl(room.name, room.avatar_style || "bottts")}
                  alt={room.name}
                  class="size-full object-cover"
                />
              </div>

              <div class="flex-1 min-w-0">
                <div class="flex items-center justify-between">
                  <span class="font-bold text-sm text-white truncate">{room.name}</span>
                  <div class="size-5 rounded-md bg-[#161624] border border-[#252538] flex items-center justify-center text-purple-400 shrink-0">
                    <IconComponent class="size-3" />
                  </div>
                </div>
                <p class="text-[11px] text-zinc-400 truncate mt-0.5">
                  {room.description || tmpl.description}
                </p>
                <div class="flex items-center gap-2 mt-1.5">
                  <span class="text-[9px] px-1.5 py-0.2 rounded bg-[#161624] border border-[#262638] text-zinc-300 font-mono capitalize">
                    {room.office_template.replace("-", " ")}
                  </span>
                  <span class="text-[10px] text-emerald-400 flex items-center gap-1 font-mono">
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
