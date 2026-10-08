<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import { Brain, Sparkles, Search } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  interface Props {
    chatroomId: string;
  }
  let { chatroomId }: Props = $props();

  let memories = $state<any[]>([]);
  let query = $state("");
  let newContent = $state("");
  let newCategory = $state("general");
  let loading = $state(false);

  const CATEGORIES = $derived([
    { id: "general", label: t("memory.cGeneral"), icon: "🧠" },
    { id: "preference", label: t("memory.cPreference"), icon: "⭐" },
    { id: "fact", label: t("memory.cFact"), icon: "📌" },
    { id: "process", label: t("memory.cProcess"), icon: "⚡" },
    { id: "rule", label: t("memory.cRule"), icon: "🛡️" },
    { id: "architecture", label: t("memory.cArchitecture"), icon: "🏛️" },
  ]);

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
  <!-- Compact pane header (OpenBot ConnectorCenter/Routines pattern): icon +
       bold title + count chip. The old uppercase mini-title with subtitle line
       was chrome the drawers already label externally. -->
  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3.5">
    <div class="flex items-center justify-between gap-2">
      <div class="flex items-center gap-2 min-w-0">
        <Brain class="size-4 shrink-0 text-[var(--brand-text)]" />
        <span class="text-[13px] font-bold text-[var(--text-primary)] truncate">{t("memory.title")}</span>
        <span class="text-[11px] px-1.5 py-0.5 rounded-md bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-[var(--brand-text)] font-mono shrink-0">
          {memories.length}
        </span>
      </div>
    </div>

    <!-- Search Bar -->
    <div class="flex gap-2">
      <div class="relative flex-1">
        <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
        <Input
          bind:value={query}
          aria-label="Search office memories"
          placeholder={t("memory.searchPlaceholder")}
          class="pl-8 h-8.5 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)] focus-visible:border-[var(--brand)]"
          onkeydown={(e) => e.key === "Enter" && search()}
        />
      </div>
      <Button
        size="sm"
        variant="outline"
        onclick={search}
        class="gap-1.5 h-8.5 text-xs bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:border-[var(--brand)]/40 hover:text-[var(--text-primary)] cursor-pointer"
      >
        <Search class="size-3.5" />
        <span>{t("memory.search")}</span>
      </Button>
      {#if query}
        <Button
          size="sm"
          variant="outline"
          onclick={() => { query = ""; load(); }}
          class="h-8.5 text-xs bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
        >
          {t("memory.clear")}
        </Button>
      {/if}
    </div>

    <!-- Add New Knowledge Item -->
    <div class="space-y-2.5 pt-2 border-t border-[var(--hairline)]">
      <Textarea
        bind:value={newContent}
        aria-label="New memory content"
        placeholder={t("memory.addPlaceholder")}
        rows={2}
        class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)] focus-visible:border-[var(--brand)] min-h-[56px] leading-relaxed"
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
          class="h-8.5 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium gap-1.5 cursor-pointer shadow-sm shrink-0"
        >
          <Sparkles class="size-3.5" />
          <span>{t("memory.addBtn")}</span>
        </Button>
      </div>
    </div>
  </div>

  <!-- Memory Items List -->
  <div class="space-y-2 max-h-[300px] min-h-0 overflow-y-auto pr-1 overscroll-contain no-scrollbar">
    {#each memories as m (m.id || m.content)}
      <div class="p-3.5 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--hairline-strong)] transition-all space-y-2">
        <div class="flex items-start justify-between gap-3">
          <p class="text-xs text-[var(--text-secondary)] flex-1 leading-relaxed font-sans select-text">
            {m.content}
          </p>
          <Badge
            variant="outline"
            class="text-[11px] font-mono shrink-0 bg-[var(--brand-soft)] border-[var(--brand)]/30 text-[var(--brand-text)] px-2 py-0.5"
          >
            {m.category || "general"}
          </Badge>
        </div>

        <div class="flex items-center gap-3 text-[11px] text-[var(--text-tertiary)] font-mono pt-1 border-t border-[var(--hairline)]">
          <span class="flex items-center gap-1 text-[var(--brand-text)]">
            <Brain class="size-3" />
            <span>{t("memory.relevance", { pct: Math.round((m.importance || 0.8) * 100) })}</span>
          </span>
          <span>•</span>
          <span class="text-[var(--text-tertiary)]">{t("memory.recalls", { n: m.access_count || 0 })}</span>
          <span class="ml-auto text-[var(--text-tertiary)]">
            {m.created_at ? new Date(m.created_at).toLocaleDateString() : t("memory.active")}
          </span>
        </div>
      </div>
    {:else}
      <div class="py-10 text-center text-[var(--text-muted)] border border-dashed border-[var(--hairline)] rounded-xl space-y-1.5">
        <Brain class="size-6 mx-auto opacity-30 text-[var(--brand-text)]" />
        <p class="text-xs">{t("memory.emptyTitle")}</p>
        <p class="text-[11px] text-[var(--text-tertiary)]">{t("memory.emptyHint")}</p>
      </div>
    {/each}
  </div>
</div>
