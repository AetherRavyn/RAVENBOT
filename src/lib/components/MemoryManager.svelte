<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import {
    Brain,
    Check,
    Copy,
    Loader2,
    Pencil,
    Search,
    Sparkles,
    Trash2,
    X,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  interface CategoryOption {
    id: string;
    label: string;
    icon?: string;
  }

  interface Props {
    /** "agent" edits memory_facts; "office" edits shared office knowledge. */
    scope: "agent" | "office";
    targetId: string;
    categories?: CategoryOption[];
  }

  interface MemoryRow {
    id: string;
    content: string;
    category: string | null;
    importance: number;
    access_count: number;
    last_accessed: string;
    created_at: string;
    source: string;
    source_thread_id: string | null;
    source_run_id: string | null;
    updated_at: string;
  }

  interface DuplicateMember {
    id: string;
    content: string;
    importance: number;
    access_count: number;
    created_at: string;
    similarity: number;
  }

  interface DuplicateGroup {
    keep: DuplicateMember;
    duplicates: DuplicateMember[];
  }

  interface MemorySummaryPreview {
    content: string;
    importance: number;
    source_ids: string[];
  }

  let { scope, targetId, categories = [] }: Props = $props();

  let memories = $state<MemoryRow[]>([]);
  let query = $state("");
  let newContent = $state("");
  let newCategory = $state("general");
  let loading = $state(false);
  let busyId = $state<string | null>(null);
  let error = $state("");
  let notice = $state("");
  let editingId = $state<string | null>(null);
  let editContent = $state("");
  let editCategory = $state("general");
  let confirmId = $state<string | null>(null);
  let duplicateGroups = $state<DuplicateGroup[] | null>(null);
  let duplicateBusy = $state(false);
  let duplicateError = $state("");
  let duplicateNotice = $state("");
  let duplicateConfirm = $state(false);
  let selectedIds = $state<string[]>([]);
  let summaryBusy = $state(false);
  let summaryPreview = $state<MemorySummaryPreview | null>(null);
  let summaryContent = $state("");
  let summaryError = $state("");
  let summaryNotice = $state("");

  const isOffice = $derived(scope === "office");
  const filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return memories;
    return memories.filter((memory) =>
      memory.content.toLowerCase().includes(needle),
    );
  });
  const duplicateCount = $derived(
    (duplicateGroups ?? []).reduce(
      (total, group) => total + group.duplicates.length,
      0,
    ),
  );
  const title = $derived(isOffice ? t("memory.title") : t("memory.agentTitle"));
  const description = $derived(
    isOffice ? t("memory.desc") : t("memory.agentDesc"),
  );

  function sourceLabel(source: string | null | undefined): string {
    switch (source) {
      case "user":
        return t("memory.sourceUser");
      case "agent":
        return t("memory.sourceAgent");
      case "self-review":
        return t("memory.sourceReview");
      case "import":
        return t("memory.sourceImport");
      case "summary":
        return t("memory.sourceSummary");
      default:
        return t("memory.sourceUnknown");
    }
  }

  function listArgs() {
    return isOffice ? { chatroomId: targetId } : { botId: targetId };
  }

  function resetDuplicates() {
    duplicateGroups = null;
    duplicateError = "";
    duplicateNotice = "";
    duplicateConfirm = false;
  }

  function clearSelection() {
    selectedIds = [];
    summaryPreview = null;
    summaryContent = "";
    summaryError = "";
  }

  function toggleSelected(id: string) {
    selectedIds = selectedIds.includes(id)
      ? selectedIds.filter((value) => value !== id)
      : [...selectedIds, id];
    summaryPreview = null;
    summaryError = "";
    summaryNotice = "";
  }

  async function summarizeSelected() {
    if (summaryBusy || selectedIds.length < 2) return;
    summaryBusy = true;
    summaryError = "";
    summaryNotice = "";
    summaryPreview = null;
    try {
      const preview = await invoke<MemorySummaryPreview>(
        "summarize_memories",
        {
          scope: isOffice ? "office" : "agent",
          targetId,
          memoryIds: selectedIds,
        },
      );
      summaryPreview = preview;
      summaryContent = preview.content;
    } catch (cause) {
      summaryError = String(cause);
    } finally {
      summaryBusy = false;
    }
  }

  async function applySummary() {
    if (!summaryPreview || summaryBusy) return;
    const content = summaryContent.trim();
    if (!content) return;
    summaryBusy = true;
    summaryError = "";
    summaryNotice = "";
    try {
      const result = await invoke<{ replaced: number }>(
        "apply_memory_summary",
        {
          scope: isOffice ? "office" : "agent",
          targetId,
          sourceIds: selectedIds,
          content,
          importance: summaryPreview.importance,
        },
      );
      summaryNotice = t("memory.summarized", { count: result.replaced });
      selectedIds = [];
      summaryPreview = null;
      summaryContent = "";
      resetDuplicates();
      await load();
    } catch (cause) {
      summaryError = String(cause);
    } finally {
      summaryBusy = false;
    }
  }

  function duplicateArgs() {
    return {
      scope: isOffice ? "office" : "agent",
      targetId,
      threshold: 0.92,
    };
  }

  async function findDuplicates() {
    if (duplicateBusy || memories.length < 2) return;
    duplicateBusy = true;
    duplicateError = "";
    duplicateNotice = "";
    duplicateGroups = null;
    duplicateConfirm = false;
    try {
      duplicateGroups = await invoke<DuplicateGroup[]>(
        "find_memory_duplicates",
        duplicateArgs(),
      );
    } catch (cause) {
      duplicateError = String(cause);
    } finally {
      duplicateBusy = false;
    }
  }

  async function removeDuplicates() {
    if (!duplicateGroups || duplicateBusy || duplicateCount === 0) return;
    duplicateBusy = true;
    duplicateError = "";
    duplicateNotice = "";
    let removed = 0;
    try {
      for (const group of duplicateGroups) {
        if (group.duplicates.length === 0) continue;
        removed += await invoke<number>("remove_memory_duplicates", {
          ...duplicateArgs(),
          keepId: group.keep.id,
          duplicateIds: group.duplicates.map((duplicate) => duplicate.id),
        });
      }
      duplicateGroups = null;
      duplicateConfirm = false;
      duplicateNotice = t("memory.dedupeRemoved", { count: removed });
    } catch (cause) {
      duplicateError = String(cause);
    } finally {
      duplicateBusy = false;
      await load();
    }
  }

  async function load() {
    if (!targetId) return;
    loading = true;
    try {
      memories = await invoke<MemoryRow[]>(
        isOffice ? "list_office_memories" : "list_bot_memories",
        listArgs(),
      );
      const present = new Set(memories.map((memory) => memory.id));
      selectedIds = selectedIds.filter((id) => present.has(id));
      error = "";
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function add() {
    const content = newContent.trim();
    if (!content || loading) return;
    loading = true;
    error = "";
    notice = "";
    try {
      if (isOffice) {
        await invoke("add_office_memory", {
          chatroomId: targetId,
          content,
          category: newCategory,
          createdBy: null,
        });
      } else {
        await invoke("add_bot_memory", {
          botId: targetId,
          content,
          importance: 0.7,
        });
      }
      newContent = "";
      notice = t("memory.saved");
      resetDuplicates();
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  function startEdit(memory: MemoryRow) {
    editingId = memory.id;
    editContent = memory.content;
    editCategory = memory.category || "general";
    confirmId = null;
    error = "";
    notice = "";
  }

  function cancelEdit() {
    editingId = null;
    editContent = "";
  }

  async function saveEdit(memory: MemoryRow) {
    const content = editContent.trim();
    if (!content || busyId) return;
    busyId = memory.id;
    error = "";
    notice = "";
    try {
      if (isOffice) {
        await invoke("update_office_memory", {
          chatroomId: targetId,
          memoryId: memory.id,
          content,
          category: editCategory,
          importance: memory.importance,
        });
      } else {
        await invoke("update_bot_memory", {
          botId: targetId,
          memoryId: memory.id,
          content,
          importance: memory.importance,
        });
      }
      editingId = null;
      notice = t("memory.updated");
      resetDuplicates();
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      busyId = null;
    }
  }

  async function deleteMemory(memory: MemoryRow) {
    if (busyId) return;
    busyId = memory.id;
    error = "";
    notice = "";
    try {
      await invoke(isOffice ? "delete_office_memory" : "delete_bot_memory", {
        ...listArgs(),
        memoryId: memory.id,
      });
      if (editingId === memory.id) editingId = null;
      confirmId = null;
      notice = t("memory.deleted");
      resetDuplicates();
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      busyId = null;
    }
  }

  $effect(() => {
    const id = targetId;
    if (id) {
      resetDuplicates();
      selectedIds = [];
      summaryPreview = null;
      summaryContent = "";
      summaryError = "";
      summaryNotice = "";
      void load();
    }
  });
</script>

<div class="space-y-4">
  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3.5">
    <div class="flex items-center justify-between gap-2">
      <div class="flex min-w-0 items-center gap-2">
        <Brain class="size-4 shrink-0 text-[var(--brand-text)]" />
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <span class="truncate text-[13px] font-bold text-[var(--text-primary)]">
              {title}
            </span>
            <span class="shrink-0 rounded-md border border-[var(--brand)]/25 bg-[var(--brand-soft)] px-1.5 py-0.5 font-mono text-[11px] text-[var(--brand-text)]">
              {memories.length}
            </span>
          </div>
          <p class="truncate text-[11px] text-[var(--text-tertiary)]">
            {description}
          </p>
        </div>
      </div>
    </div>

    <div class="flex gap-2">
      <div class="relative flex-1">
        <Search class="absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-[var(--text-muted)]" />
        <Input
          bind:value={query}
          aria-label={t("memory.searchLabel")}
          placeholder={t("memory.searchPlaceholder")}
          class="h-8.5 border-[var(--hairline)] bg-[var(--surface-2)] pl-8 text-xs text-[var(--text-primary)] focus-visible:border-[var(--brand)]"
        />
      </div>
      {#if query}
        <Button
          size="sm"
          variant="outline"
          onclick={() => (query = "")}
          class="h-8.5 cursor-pointer border-[var(--hairline)] bg-[var(--surface-3)] text-xs text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
        >
          {t("memory.clear")}
        </Button>
      {/if}
    </div>

    <div class="space-y-2.5 border-t border-[var(--hairline)] pt-2">
      <Textarea
        bind:value={newContent}
        aria-label={t("memory.addLabel")}
        placeholder={t("memory.addPlaceholder")}
        rows={2}
        class="min-h-[56px] border-[var(--hairline)] bg-[var(--surface-2)] text-xs leading-relaxed text-[var(--text-primary)] focus-visible:border-[var(--brand)]"
      />

      <div class="flex flex-col items-stretch gap-2 sm:flex-row sm:items-center">
        {#if isOffice}
          <div class="relative flex-1">
            <SimpleSelect
              value={newCategory}
              options={categories.map((category) => ({
                value: category.id,
                label: category.label,
                icon: category.icon,
              }))}
              onValueChange={(value) => (newCategory = value)}
              class="h-8.5 rounded-xl"
            />
          </div>
        {/if}

        <Button
          size="sm"
          onclick={add}
          disabled={loading || !newContent.trim()}
          class="h-8.5 shrink-0 cursor-pointer gap-1.5 bg-[var(--brand)] text-xs font-medium text-[var(--text-on-light)] shadow-sm hover:bg-[var(--brand-hover)]"
        >
          {#if loading}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <Sparkles class="size-3.5" />
          {/if}
          <span>{isOffice ? t("memory.addBtn") : t("memory.addAgentBtn")}</span>
        </Button>
      </div>
    </div>

    {#if notice}
      <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
    {/if}
    {#if summaryNotice}
      <p class="text-[11px] text-[var(--success-text)]" role="status">
        {summaryNotice}
      </p>
    {/if}
    {#if error}
      <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
        {error}
      </p>
    {/if}
  </div>

  {#if selectedIds.length >= 2 || summaryPreview}
    <div class="rounded-2xl border border-[var(--brand)]/30 bg-[var(--brand-soft)]/40 p-4 space-y-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <p class="flex items-center gap-1.5 text-xs font-bold text-[var(--text-primary)]">
          <Sparkles class="size-3.5 text-[var(--brand-text)]" />
          {t("memory.summaryTitle")}
        </p>
        <span class="font-mono text-[11px] text-[var(--brand-text)]">
          {t("memory.selectedCount", { count: selectedIds.length })}
        </span>
        <Button
          size="sm"
          variant="outline"
          class="h-7 cursor-pointer border-[var(--hairline)] text-[11px]"
          onclick={clearSelection}
          disabled={summaryBusy}
        >
          {t("memory.clearSelection")}
        </Button>
      </div>

      {#if !summaryPreview}
        <p class="text-[11px] leading-4 text-[var(--text-secondary)]">
          {t("memory.summaryHint")}
        </p>
        <Button
          size="sm"
          class="h-8 cursor-pointer gap-1.5 bg-[var(--brand)] text-[11px] text-[var(--text-on-light)] hover:bg-[var(--brand-hover)]"
          onclick={() => void summarizeSelected()}
          disabled={summaryBusy || selectedIds.length < 2}
        >
          {#if summaryBusy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <Sparkles class="size-3.5" />
          {/if}
          {summaryBusy
            ? t("memory.summarizing")
            : t("memory.summarizeSelected")}
        </Button>
      {:else}
        <p class="text-[11px] leading-4 text-[var(--text-secondary)]">
          {t("memory.summaryHint")}
        </p>
        <Textarea
          bind:value={summaryContent}
          aria-label={t("memory.summaryContent")}
          rows={4}
          class="min-h-[96px] border-[var(--hairline)] bg-[var(--surface-1)] text-xs leading-relaxed text-[var(--text-primary)] focus-visible:border-[var(--brand)]"
          disabled={summaryBusy}
        />
        <p class="font-mono text-[11px] text-[var(--text-tertiary)]">
          {t("memory.summaryImportance", {
            pct: Math.round(summaryPreview.importance * 100),
          })}
        </p>
        <p class="text-[11px] leading-4 text-[var(--text-muted)]">
          {t("memory.summaryModelNote")}
        </p>
        <div class="flex flex-wrap items-center justify-end gap-2">
          <Button
            size="sm"
            variant="outline"
            class="h-7 cursor-pointer border-[var(--hairline)] text-[11px]"
            onclick={() => {
              summaryPreview = null;
              summaryError = "";
            }}
            disabled={summaryBusy}
          >
            {t("memory.cancel")}
          </Button>
          <Button
            size="sm"
            class="h-7 cursor-pointer gap-1 bg-[var(--brand)] text-[11px] text-[var(--text-on-light)] hover:bg-[var(--brand-hover)]"
            onclick={() => void applySummary()}
            disabled={summaryBusy || !summaryContent.trim()}
          >
            {#if summaryBusy}
              <Loader2 class="size-3 animate-spin" />
            {:else}
              <Check class="size-3" />
            {/if}
            {t("memory.replaceWithSummary", {
              count: selectedIds.length,
            })}
          </Button>
        </div>
      {/if}

      {#if summaryError}
        <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
          {summaryError}
        </p>
      {/if}
    </div>
  {/if}

  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3">
    <div class="flex flex-wrap items-start justify-between gap-2">
      <div class="min-w-0">
        <p class="flex items-center gap-1.5 text-xs font-bold text-[var(--text-primary)]">
          <Copy class="size-3.5 text-[var(--brand-text)]" />
          {t("memory.duplicatesTitle")}
        </p>
        <p class="mt-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
          {t("memory.duplicatesHint")}
        </p>
      </div>
      <Button
        size="sm"
        variant="outline"
        class="h-8 shrink-0 cursor-pointer gap-1.5 border-[var(--hairline)] text-[11px]"
        onclick={() => void findDuplicates()}
        disabled={duplicateBusy || memories.length < 2}
      >
        {#if duplicateBusy && duplicateGroups === null}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <Copy class="size-3.5" />
        {/if}
        {duplicateBusy && duplicateGroups === null
          ? t("memory.scanningDuplicates")
          : t("memory.findDuplicates")}
      </Button>
    </div>

    {#if duplicateNotice}
      <p class="text-[11px] text-[var(--success-text)]" role="status">
        {duplicateNotice}
      </p>
    {/if}
    {#if duplicateError}
      <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
        {duplicateError}
      </p>
    {/if}

    {#if duplicateGroups}
      {#if duplicateGroups.length === 0}
        <p class="text-[11px] text-[var(--success-text)]" role="status">
          {t("memory.noDuplicates")}
        </p>
      {:else}
        <p class="text-[11px] text-[var(--text-secondary)]" role="status">
          {t("memory.duplicateSummary", {
            groups: duplicateGroups.length,
            count: duplicateCount,
          })}
        </p>
        <div class="max-h-64 space-y-2 overflow-y-auto no-scrollbar pr-1">
          {#each duplicateGroups as group, groupIndex (group.keep.id)}
            <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-2.5 space-y-1.5">
              <p class="text-[11px] font-semibold text-[var(--text-secondary)]">
                {t("memory.keep")}
              </p>
              <p class="text-[11px] leading-4 text-[var(--text-primary)]">
                {group.keep.content}
              </p>
              {#each group.duplicates as duplicate (duplicate.id)}
                <div class="flex items-start justify-between gap-2 border-t border-[var(--hairline)] pt-1.5">
                  <p class="min-w-0 flex-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
                    {duplicate.content}
                  </p>
                  <span class="shrink-0 font-mono text-[11px] text-[var(--warning-text)]">
                    {t("memory.similar", {
                      pct: Math.round(duplicate.similarity * 100),
                    })}
                  </span>
                </div>
              {/each}
            </div>
          {/each}
        </div>

        {#if duplicateConfirm}
          <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-2.5 py-2">
            <p class="text-[11px] text-[var(--danger-text)]">
              {t("memory.dedupeConfirm")}
            </p>
            <div class="flex items-center gap-1.5">
              <Button
                size="sm"
                variant="outline"
                class="h-7 cursor-pointer border-[var(--hairline)] text-[11px]"
                onclick={() => (duplicateConfirm = false)}
                disabled={duplicateBusy}
              >
                {t("memory.cancel")}
              </Button>
              <Button
                size="sm"
                variant="destructive"
                class="h-7 cursor-pointer gap-1 text-[11px]"
                onclick={() => void removeDuplicates()}
                disabled={duplicateBusy}
              >
                {#if duplicateBusy}
                  <Loader2 class="size-3 animate-spin" />
                {:else}
                  <Trash2 class="size-3" />
                {/if}
                {t("memory.confirmRemoval")}
              </Button>
            </div>
          </div>
        {:else}
          <div class="flex items-center justify-end">
            <Button
              size="sm"
              variant="destructive"
              class="h-7 cursor-pointer gap-1 text-[11px]"
              onclick={() => (duplicateConfirm = true)}
              disabled={duplicateBusy || duplicateCount === 0}
            >
              <Trash2 class="size-3" />
              {t("memory.removeDuplicates", { count: duplicateCount })}
            </Button>
          </div>
        {/if}
      {/if}
    {/if}
  </div>

  <div class="max-h-[360px] min-h-0 space-y-2 overflow-y-auto overscroll-contain pr-1 no-scrollbar">
    {#each filtered as memory (memory.id)}
      <div class="space-y-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-3.5 transition-all hover:border-[var(--hairline-strong)]">
        {#if editingId === memory.id}
          <Textarea
            bind:value={editContent}
            aria-label={t("memory.editLabel")}
            rows={3}
            class="min-h-[72px] border-[var(--hairline)] bg-[var(--surface-1)] text-xs leading-relaxed text-[var(--text-primary)] focus-visible:border-[var(--brand)]"
          />
          {#if isOffice}
            <SimpleSelect
              value={editCategory}
              options={categories.map((category) => ({
                value: category.id,
                label: category.label,
                icon: category.icon,
              }))}
              onValueChange={(value) => (editCategory = value)}
              class="h-8.5 rounded-xl"
            />
          {/if}
          <div class="flex items-center justify-end gap-2">
            <Button
              size="sm"
              variant="outline"
              class="h-7 cursor-pointer gap-1 border-[var(--hairline)] text-[11px]"
              onclick={cancelEdit}
              disabled={busyId === memory.id}
            >
              <X class="size-3" />
              {t("memory.cancel")}
            </Button>
            <Button
              size="sm"
              class="h-7 cursor-pointer gap-1 bg-[var(--brand)] text-[11px] text-[var(--text-on-light)] hover:bg-[var(--brand-hover)]"
              onclick={() => void saveEdit(memory)}
              disabled={busyId === memory.id || !editContent.trim()}
            >
              {#if busyId === memory.id}
                <Loader2 class="size-3 animate-spin" />
              {:else}
                <Check class="size-3" />
              {/if}
              {t("memory.save")}
            </Button>
          </div>
        {:else}
          <div class="flex items-start justify-between gap-3">
            <p class="flex-1 select-text font-sans text-xs leading-relaxed text-[var(--text-secondary)]">
              {memory.content}
            </p>
            <div class="flex shrink-0 items-center gap-1">
              <input
                type="checkbox"
                class="size-3.5 cursor-pointer accent-[var(--brand)]"
                checked={selectedIds.includes(memory.id)}
                onchange={() => toggleSelected(memory.id)}
                aria-label={t("memory.selectMemory")}
              />
              <button
                type="button"
                class="inline-flex size-6 cursor-pointer items-center justify-center rounded-md text-[var(--text-muted)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
                onclick={() => startEdit(memory)}
                aria-label={t("memory.edit")}
                title={t("memory.edit")}
              >
                <Pencil class="size-3.5" />
              </button>
              <button
                type="button"
                class="inline-flex size-6 cursor-pointer items-center justify-center rounded-md text-[var(--text-muted)] hover:bg-[var(--danger-soft)] hover:text-[var(--danger-text)]"
                onclick={() => (confirmId = memory.id)}
                aria-label={t("memory.delete")}
                title={t("memory.delete")}
              >
                <Trash2 class="size-3.5" />
              </button>
            </div>
          </div>

          {#if confirmId === memory.id}
            <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-2.5 py-2">
              <p class="text-[11px] text-[var(--danger-text)]">
                {t("memory.deleteConfirm")}
              </p>
              <div class="flex items-center gap-1.5">
                <Button
                  size="sm"
                  variant="outline"
                  class="h-7 cursor-pointer border-[var(--hairline)] text-[11px]"
                  onclick={() => (confirmId = null)}
                  disabled={busyId === memory.id}
                >
                  {t("memory.cancel")}
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  class="h-7 cursor-pointer gap-1 text-[11px]"
                  onclick={() => void deleteMemory(memory)}
                  disabled={busyId === memory.id}
                >
                  {#if busyId === memory.id}
                    <Loader2 class="size-3 animate-spin" />
                  {:else}
                    <Trash2 class="size-3" />
                  {/if}
                  {t("memory.delete")}
                </Button>
              </div>
            </div>
          {/if}

          <div class="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-[var(--hairline)] pt-1 font-mono text-[11px] text-[var(--text-tertiary)]">
            <span class="flex items-center gap-1 text-[var(--brand-text)]">
              <Brain class="size-3" />
              <span>{t("memory.relevance", { pct: Math.round((memory.importance || 0.8) * 100) })}</span>
            </span>
            <span>•</span>
            <span>{t("memory.recalls", { n: memory.access_count || 0 })}</span>
            <span>•</span>
            <span
              class="truncate"
              title={memory.source_thread_id
                ? t("memory.sourceThread", {
                    id: memory.source_thread_id.slice(0, 8),
                  })
                : undefined}
            >
              {sourceLabel(memory.source)}
            </span>
            {#if !isOffice}
              <Badge
                variant="outline"
                class="shrink-0 border-[var(--brand)]/30 bg-[var(--brand-soft)] px-2 py-0.5 text-[11px] font-mono text-[var(--brand-text)]"
              >
                {t("memory.agentBadge")}
              </Badge>
            {:else if memory.category}
              <Badge
                variant="outline"
                class="shrink-0 border-[var(--brand)]/30 bg-[var(--brand-soft)] px-2 py-0.5 text-[11px] font-mono text-[var(--brand-text)]"
              >
                {memory.category}
              </Badge>
            {/if}
            <span class="ml-auto text-[var(--text-tertiary)]">
              {memory.created_at ? new Date(memory.created_at).toLocaleDateString() : t("memory.active")}
            </span>
          </div>
        {/if}
      </div>
    {:else}
      <div class="space-y-1.5 rounded-xl border border-dashed border-[var(--hairline)] py-10 text-center text-[var(--text-muted)]">
        <Brain class="mx-auto size-6 text-[var(--brand-text)] opacity-30" />
        <p class="text-xs">
          {query ? t("memory.noMatches") : isOffice ? t("memory.emptyTitle") : t("memory.emptyAgentTitle")}
        </p>
        <p class="text-[11px] text-[var(--text-tertiary)]">
          {query ? t("memory.noMatchesHint") : isOffice ? t("memory.emptyHint") : t("memory.emptyAgentHint")}
        </p>
      </div>
    {/each}
  </div>
</div>
