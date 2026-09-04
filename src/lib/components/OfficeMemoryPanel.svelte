<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Card from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import { Brain, Users, Sparkles, Search, Plus, Filter, Tag, Check, Clock, Trash2 } from "@lucide/svelte";
  import { cn } from "$lib/utils.js";

  interface Props {
    chatroomId: string;
  }
  let { chatroomId }: Props = $props();

  let memories = $state<any[]>([]);
  let query = $state("");
  let newContent = $state("");
  let newCategory = $state("general");
  let loading = $state(false);

  const CATEGORIES = [
    { id: "general", label: "General Knowledge", icon: "🧠" },
    { id: "preference", label: "Client & User Preference", icon: "⭐" },
    { id: "fact", label: "Technical Fact / Specs", icon: "📌" },
    { id: "process", label: "Workflow / SOP Process", icon: "⚡" },
    { id: "rule", label: "Governance / Security Rule", icon: "🛡️" },
    { id: "architecture", label: "Architecture Decision", icon: "🏛️" },
  ];

  async function load() {
    try {
      memories = await invoke("list_office_memories", { chatroomId });
    } catch (e) {
      console.error("Failed to load office memories:", e);
    }
  }

  async function search() {
    if (!query.trim()) return load();
    try {
      const res = await invoke("search_office_memories", { chatroomId, query: query.trim() });
      memories = (res as any[]).map(([m]: any) => m);
    } catch (e) {
      console.error("Failed to search office memories:", e);
    }
  }

  async function add() {
    if (!newContent.trim()) return;
    loading = true;
    try {
      await invoke("add_office_memory", {
        chatroomId,
        content: newContent.trim(),
        category: newCategory,
        createdBy: null,
      });
      newContent = "";
      await load();
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (chatroomId) load();
  });
</script>

<div class="space-y-4">
  <!-- Knowledge Capture Card -->
  <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3.5 backdrop-blur-md">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="size-7 rounded-lg bg-purple-950/60 border border-purple-500/30 flex items-center justify-center text-purple-300">
          <Brain class="size-4" />
        </div>
        <div>
          <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono">
            Shared Office Brain (Blackboard Memory)
          </h4>
          <p class="text-[11px] text-zinc-400">
            Persistent long-term knowledge shared across all agents in this office.
          </p>
        </div>
      </div>
      <Badge variant="outline" class="bg-purple-950/40 border-purple-500/30 text-purple-300 font-mono text-[10px]">
        {memories.length} Memories
      </Badge>
    </div>

    <!-- Search Bar -->
    <div class="flex gap-2">
      <div class="relative flex-1">
        <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
        <Input
          bind:value={query}
          placeholder="Semantic search team knowledge base..."
          class="pl-8 h-8.5 text-xs bg-[#141424] border-[#25253c] text-white focus-visible:border-purple-500"
          onkeydown={(e) => e.key === "Enter" && search()}
        />
      </div>
      <Button
        size="sm"
        variant="outline"
        onclick={search}
        class="gap-1.5 h-8.5 text-xs bg-[#161628] border-[#29293e] text-zinc-200 hover:bg-[#1f1f34] cursor-pointer"
      >
        <Search class="size-3.5" />
        <span>Search</span>
      </Button>
      {#if query}
        <Button
          size="sm"
          variant="outline"
          onclick={() => { query = ""; load(); }}
          class="h-8.5 text-xs bg-[#161628] border-[#29293e] text-zinc-400 hover:text-white cursor-pointer"
        >
          Clear
        </Button>
      {/if}
    </div>

    <!-- Add New Knowledge Item -->
    <div class="space-y-2.5 pt-2 border-t border-white/5">
      <Textarea
        bind:value={newContent}
        placeholder="Add sovereign team knowledge (e.g. 'Always use strict JSON schema validation for all incoming API payloads. Client database host is staging-db.internal.')"
        rows={2}
        class="text-xs bg-[#141424] border-[#25253c] text-white focus-visible:border-purple-500 min-h-[56px] leading-relaxed"
      />

      <div class="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
        <!-- Styled Dropdown Menu -->
        <div class="relative flex-1">
          <select
            bind:value={newCategory}
            class="w-full h-8.5 rounded-xl border border-[#28283e] bg-[#141424] px-3 pr-8 text-xs text-zinc-200 focus:border-purple-500 focus:ring-1 focus:ring-purple-500/30 outline-none cursor-pointer transition-all hover:bg-[#18182a]"
          >
            {#each CATEGORIES as c}
              <option value={c.id} class="bg-[#0e0e1a] text-zinc-100 py-1.5">
                {c.icon} {c.label}
              </option>
            {/each}
          </select>
        </div>

        <Button
          size="sm"
          onclick={add}
          disabled={loading || !newContent.trim()}
          class="h-8.5 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium gap-1.5 cursor-pointer shadow-sm shrink-0"
        >
          <Sparkles class="size-3.5" />
          <span>Add to Team Brain</span>
        </Button>
      </div>
    </div>
  </div>

  <!-- Memory Items List -->
  <div class="space-y-2 max-h-[300px] min-h-0 overflow-y-auto pr-1 overscroll-contain">
    {#each memories as m (m.id || m.content)}
      <div class="p-3.5 rounded-xl border border-[#202034] bg-[#10101e] hover:border-purple-500/30 transition-all space-y-2">
        <div class="flex items-start justify-between gap-3">
          <p class="text-xs text-zinc-200 flex-1 leading-relaxed font-sans select-text">
            {m.content}
          </p>
          <Badge
            variant="outline"
            class="text-[10px] font-mono shrink-0 bg-purple-950/40 border-purple-500/30 text-purple-300 px-2 py-0.5"
          >
            {m.category || "general"}
          </Badge>
        </div>

        <div class="flex items-center gap-3 text-[10px] text-zinc-400 font-mono pt-1 border-t border-white/5">
          <span class="flex items-center gap-1 text-pink-300">
            <Brain class="size-3" />
            <span>{Math.round((m.importance || 0.8) * 100)}% relevance</span>
          </span>
          <span>•</span>
          <span class="text-zinc-400">{m.access_count || 0} recalls</span>
          <span class="ml-auto text-zinc-400">
            {m.created_at ? new Date(m.created_at).toLocaleDateString() : "Active"}
          </span>
        </div>
      </div>
    {:else}
      <div class="py-10 text-center text-zinc-500 border border-dashed border-[#202034] rounded-xl space-y-1.5">
        <Brain class="size-6 mx-auto opacity-30 text-purple-400" />
        <p class="text-xs">No team memories recorded yet.</p>
        <p class="text-[11px] text-zinc-400">Add operational facts or SOP rules above for your agents to recall.</p>
      </div>
    {/each}
  </div>
</div>
