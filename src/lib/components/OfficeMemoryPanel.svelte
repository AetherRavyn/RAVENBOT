<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
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
  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3.5 ">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="size-7 rounded-lg bg-[var(--brand-soft)] border border-[var(--brand)]/30 flex items-center justify-center text-[var(--brand-text)]">
          <Brain class="size-4" />
        </div>
        <div>
          <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono">
            Shared Office Brain (Blackboard Memory)
          </h4>
          <p class="text-[11px] text-[var(--text-tertiary)]">
            Persistent long-term knowledge shared across all agents in this office.
          </p>
        </div>
      </div>
      <Badge variant="outline" class="bg-[var(--brand-soft)] border-[var(--brand)]/30 text-[var(--brand-text)] font-mono text-[10px]">
        {memories.length} Memories
      </Badge>
    </div>

    <!-- Search Bar -->
    <div class="flex gap-2">
      <div class="relative flex-1">
        <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
        <Input
          bind:value={query}
          placeholder="Semantic search team knowledge base..."
          class="pl-8 h-8.5 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-white focus-visible:border-[var(--brand)]"
          onkeydown={(e) => e.key === "Enter" && search()}
        />
      </div>
      <Button
        size="sm"
        variant="outline"
        onclick={search}
        class="gap-1.5 h-8.5 text-xs bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[#1f1f34] cursor-pointer"
      >
        <Search class="size-3.5" />
        <span>Search</span>
      </Button>
      {#if query}
        <Button
          size="sm"
          variant="outline"
          onclick={() => { query = ""; load(); }}
          class="h-8.5 text-xs bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
        >
          Clear
        </Button>
      {/if}
    </div>

    <!-- Add New Knowledge Item -->
    <div class="space-y-2.5 pt-2 border-t border-[var(--hairline)]">
      <Textarea
        bind:value={newContent}
        placeholder="Add sovereign team knowledge (e.g. 'Always use strict JSON schema validation for all incoming API payloads. Client database host is staging-db.internal.')"
        rows={2}
        class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-white focus-visible:border-[var(--brand)] min-h-[56px] leading-relaxed"
      />

      <div class="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
        <!-- Styled Dropdown Menu -->
        <div class="relative flex-1">
          <SimpleSelect
            value={newCategory}
            options={CATEGORIES.map((c) => ({ value: c.id, label: c.label, icon: c.icon }))}
            onValueChange={(v) => (newCategory = v)}
            class="h-8.5 rounded-xl"
          />
        </div>

        <Button
          size="sm"
          onclick={add}
          disabled={loading || !newContent.trim()}
          class="h-8.5 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-white font-medium gap-1.5 cursor-pointer shadow-sm shrink-0"
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
      <div class="p-3.5 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--hairline-strong)] transition-all space-y-2">
        <div class="flex items-start justify-between gap-3">
          <p class="text-xs text-[var(--text-secondary)] flex-1 leading-relaxed font-sans select-text">
            {m.content}
          </p>
          <Badge
            variant="outline"
            class="text-[10px] font-mono shrink-0 bg-[var(--brand-soft)] border-[var(--brand)]/30 text-[var(--brand-text)] px-2 py-0.5"
          >
            {m.category || "general"}
          </Badge>
        </div>

        <div class="flex items-center gap-3 text-[10px] text-[var(--text-tertiary)] font-mono pt-1 border-t border-[var(--hairline)]">
          <span class="flex items-center gap-1 text-pink-300">
            <Brain class="size-3" />
            <span>{Math.round((m.importance || 0.8) * 100)}% relevance</span>
          </span>
          <span>•</span>
          <span class="text-[var(--text-tertiary)]">{m.access_count || 0} recalls</span>
          <span class="ml-auto text-[var(--text-tertiary)]">
            {m.created_at ? new Date(m.created_at).toLocaleDateString() : "Active"}
          </span>
        </div>
      </div>
    {:else}
      <div class="py-10 text-center text-[var(--text-muted)] border border-dashed border-[var(--hairline)] rounded-xl space-y-1.5">
        <Brain class="size-6 mx-auto opacity-30 text-[var(--brand-text)]" />
        <p class="text-xs">No team memories recorded yet.</p>
        <p class="text-[11px] text-[var(--text-tertiary)]">Add operational facts or SOP rules above for your agents to recall.</p>
      </div>
    {/each}
  </div>
</div>
