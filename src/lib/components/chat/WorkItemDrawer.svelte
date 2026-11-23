<script lang="ts">
  // Durable task detail. This is the readable half of the work ledger: the
  // board says what is running, this says what happened, who owns it, what it
  // produced, and why it stopped.
  import { invoke } from "@tauri-apps/api/core";
  import { t, type TranslationKey } from "$lib/i18n";
  import {
    Ban,
    Check,
    CircleAlert,
    FileText,
    History,
    Loader2,
    MessageSquare,
    Package,
    Play,
    RefreshCw,
    Send,
    X,
  } from "@lucide/svelte";

  interface WorkItem {
    id: string;
    title: string;
    instruction: string;
    status: string;
    owner_bot_id: string | null;
    thread_id: string | null;
    run_id: string | null;
    attempts: number;
    max_attempts: number;
    created_at: string;
    started_at: string | null;
    updated_at: string;
    completed_at: string | null;
    result: string | null;
    error: string | null;
    workspace: string | null;
    metadata: Record<string, unknown>;
  }

  interface WorkComment {
    id: string;
    author: string;
    body: string;
    created_at: string;
  }

  interface WorkArtifact {
    id: string;
    path: string;
    sha256: string | null;
    mime: string | null;
    size: number | null;
    created_at: string;
  }

  interface WorkEvent {
    id: string;
    kind: string;
    payload: Record<string, unknown>;
    created_at: string;
  }

  interface WorkItemDetail {
    item: WorkItem;
    comments: WorkComment[];
    artifacts: WorkArtifact[];
    events: WorkEvent[];
  }

  interface Props {
    itemId: string;
    label: string;
    botName: string;
    onClose: () => void;
  }

  let { itemId, label, botName, onClose }: Props = $props();

  let detail = $state<WorkItemDetail | null>(null);
  let loading = $state(true);
  let loadError = $state("");
  let comment = $state("");
  let posting = $state(false);
  let actionBusy = $state("");
  let actionError = $state("");
  let blockOpen = $state(false);
  let blockReason = $state("");
  let changesOpen = $state(false);
  let changesText = $state("");
  let panel: HTMLElement | undefined = $state();

  const statusKeys: Record<string, TranslationKey> = {
    pending: "work.status.pending",
    ready: "work.status.ready",
    running: "work.status.running",
    blocked: "work.status.blocked",
    review: "work.status.review",
    done: "work.status.done",
    failed: "work.status.failed",
    skipped: "work.status.skipped",
    archived: "work.status.archived",
  };

  const status = $derived(detail?.item.status ?? "pending");
  const canRetry = $derived(
    ["failed", "skipped", "blocked"].includes(detail?.item.status ?? ""),
  );
  const canReview = $derived(detail?.item.status === "done");
  const canApprove = $derived(detail?.item.status === "review");
  const canBlock = $derived(
    !["running", "done", "blocked"].includes(detail?.item.status ?? ""),
  );

  function statusLabel(value: string): string {
    return t(statusKeys[value] ?? "work.status.unknown");
  }

  function statusTone(value: string): string {
    switch (value) {
      case "running":
      case "review":
        return "bg-[var(--brand-soft)] text-[var(--brand-text)]";
      case "done":
        return "bg-success/10 text-success";
      case "failed":
        return "bg-danger/10 text-danger";
      case "blocked":
      case "skipped":
        return "bg-warning/10 text-warning";
      default:
        return "bg-[var(--surface-2)] text-[var(--text-muted)]";
    }
  }

  function formatTime(value: string | null): string {
    if (!value) return "—";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  function shortSha(value: string | null): string {
    return value ? value.slice(0, 12) : "—";
  }

  function eventSummary(event: WorkEvent): string {
    const payload = event.payload ?? {};
    const candidate =
      payload.body ??
      payload.reason ??
      payload.error ??
      payload.result_preview ??
      payload.path ??
      payload.author ??
      payload.reviewer ??
      payload.owner;
    const text = typeof candidate === "string" ? candidate : JSON.stringify(payload);
    return text.length > 160 ? `${text.slice(0, 160)}…` : text;
  }

  async function load(id: string) {
    loading = true;
    loadError = "";
    try {
      detail = await invoke<WorkItemDetail>("get_work_item", { id });
    } catch (error) {
      detail = null;
      loadError = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    const id = itemId;
    void load(id);
  });

  $effect(() => {
    panel?.focus();
  });

  async function runAction(name: string, command: string, args: Record<string, unknown>) {
    actionBusy = name;
    actionError = "";
    try {
      await invoke(command, args);
      await load(itemId);
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      actionBusy = "";
    }
  }

  async function submitComment() {
    const body = comment.trim();
    if (!body || !detail) return;
    posting = true;
    actionError = "";
    try {
      await invoke("add_work_comment", {
        id: detail.item.id,
        body,
        author: t("work.you"),
      });
      comment = "";
      await load(itemId);
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      posting = false;
    }
  }

  async function submitBlock() {
    const reason = blockReason.trim();
    if (!reason || !detail) return;
    await runAction("block", "block_work_item", {
      id: detail.item.id,
      reason,
    });
    if (!actionError) {
      blockOpen = false;
      blockReason = "";
    }
  }

  async function submitChanges() {
    const body = changesText.trim();
    if (!body || !detail) return;
    await runAction("changes", "request_work_changes", {
      id: detail.item.id,
      comment: body,
    });
    if (!actionError) {
      changesOpen = false;
      changesText = "";
    }
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape") onClose();
  }}
/>

<div
  class="fixed inset-0 z-50 bg-[var(--scrim)]"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    bind:this={panel}
    tabindex="-1"
    role="dialog"
    aria-modal="true"
    aria-labelledby="work-item-title"
    class="absolute top-0 right-0 h-full w-[500px] max-w-[94vw] bg-[var(--surface-0)] border-l border-[var(--hairline)] flex flex-col shadow-2xl outline-none"
  >
    <header class="flex items-start gap-2 px-4 py-3 border-b border-[var(--hairline)] shrink-0">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2 min-w-0">
          <h2 id="work-item-title" class="text-sm font-semibold text-[var(--text-primary)] truncate">
            {detail?.item.title || label}
          </h2>
          <span
            class="text-[11px] font-semibold px-1.5 py-0.5 rounded-full shrink-0 {statusTone(status)}"
          >
            {statusLabel(status)}
          </span>
        </div>
        <div class="mt-0.5 text-[11px] text-[var(--text-muted)] truncate">
          {botName} · {t("work.title")}
        </div>
      </div>
      <button
        type="button"
        class="size-7 rounded-lg flex items-center justify-center text-[var(--text-muted)] hover:bg-[var(--surface-2)] hover:text-[var(--text-primary)] transition-colors cursor-pointer shrink-0"
        aria-label={t("room.close")}
        onclick={onClose}
      >
        <X class="size-4" />
      </button>
    </header>

    <div class="flex-1 min-h-0 overflow-y-auto px-4 py-3 space-y-4">
      {#if loading}
        <div class="flex items-center gap-2 text-[11px] text-[var(--text-muted)] py-6">
          <Loader2 class="size-3.5 animate-spin" />
          <span>{t("work.loading")}</span>
        </div>
      {:else if !detail}
        <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4">
          <div class="flex items-center gap-2 text-xs font-semibold text-[var(--text-primary)]">
            <CircleAlert class="size-4 text-warning" />
            <span>{t("work.notFound")}</span>
          </div>
          {#if loadError}
            <p class="mt-1.5 text-[11px] text-[var(--text-muted)] break-words">{loadError}</p>
          {/if}
          <button
            type="button"
            class="mt-3 inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[11px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] cursor-pointer"
            onclick={() => void load(itemId)}
          >
            <RefreshCw class="size-3" />
            {t("work.refresh")}
          </button>
        </div>
      {:else}
        <section class="space-y-1.5">
          <div class="text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
            {t("work.instruction")}
          </div>
          <p class="text-xs text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap">
            {detail.item.instruction}
          </p>
        </section>

        {#if detail.item.result}
          <section class="rounded-xl bg-[var(--surface-1)] p-3 space-y-1">
            <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
              <FileText class="size-3" />
              {t("work.result")}
            </div>
            <p class="text-xs text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap">
              {detail.item.result}
            </p>
          </section>
        {/if}

        {#if detail.item.error}
          <section class="rounded-xl bg-danger/10 p-3 space-y-1">
            <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-danger">
              <CircleAlert class="size-3" />
              {t("work.error")}
            </div>
            <p class="text-xs text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap">
              {detail.item.error}
            </p>
          </section>
        {/if}

        <section class="grid grid-cols-2 gap-x-3 gap-y-1.5 text-[11px]">
          <div class="text-[var(--text-muted)]">{t("work.attempts", { done: detail.item.attempts, max: detail.item.max_attempts })}</div>
          <div class="text-[var(--text-muted)]">{t("work.thread")}: <span class="font-mono text-[var(--text-tertiary)]">{detail.item.thread_id?.slice(0, 8) ?? "—"}</span></div>
          <div class="text-[var(--text-muted)]">{t("work.created")}: {formatTime(detail.item.created_at)}</div>
          <div class="text-[var(--text-muted)]">{t("work.updated")}: {formatTime(detail.item.updated_at)}</div>
        </section>

        {#if actionError}
          <p class="text-[11px] text-danger" role="alert">{actionError}</p>
        {/if}

        <section class="space-y-2">
          <div class="text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
            {t("work.actions")}
          </div>
          <div class="flex flex-wrap gap-1.5">
            {#if canRetry}
              <button
                type="button"
                class="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg bg-[var(--brand)] text-[var(--text-on-light)] text-[11px] font-semibold cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== ""}
                onclick={() => void runAction("retry", "retry_work_item", { id: detail?.item.id ?? "" })}
              >
                {#if actionBusy === "retry"}<Loader2 class="size-3 animate-spin" />{:else}<Play class="size-3" />{/if}
                {t("work.retry")}
              </button>
            {/if}
            {#if detail.item.status === "blocked"}
              <button
                type="button"
                class="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[11px] text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== ""}
                onclick={() => void runAction("unblock", "unblock_work_item", { id: detail?.item.id ?? "" })}
              >
                {#if actionBusy === "unblock"}<Loader2 class="size-3 animate-spin" />{:else}<Play class="size-3" />{/if}
                {t("work.unblock")}
              </button>
            {/if}
            {#if canReview}
              <button
                type="button"
                class="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[11px] text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== ""}
                onclick={() => void runAction("review", "request_work_review", { id: detail?.item.id ?? "" })}
              >
                {#if actionBusy === "review"}<Loader2 class="size-3 animate-spin" />{:else}<Send class="size-3" />{/if}
                {t("work.requestReview")}
              </button>
            {/if}
            {#if canApprove}
              <button
                type="button"
                class="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg bg-success/15 text-success text-[11px] font-semibold cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== ""}
                onclick={() => void runAction("approve", "approve_work_review", { id: detail?.item.id ?? "" })}
              >
                {#if actionBusy === "approve"}<Loader2 class="size-3 animate-spin" />{:else}<Check class="size-3" />{/if}
                {t("work.approve")}
              </button>
              <button
                type="button"
                class="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[11px] text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== ""}
                onclick={() => (changesOpen = !changesOpen)}
              >
                <MessageSquare class="size-3" />
                {t("work.requestChanges")}
              </button>
            {/if}
            {#if canBlock}
              <button
                type="button"
                class="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[11px] text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== ""}
                onclick={() => (blockOpen = !blockOpen)}
              >
                <Ban class="size-3" />
                {t("work.block")}
              </button>
            {/if}
          </div>

          {#if blockOpen}
            <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2.5 space-y-2">
              <label class="text-[11px] text-[var(--text-muted)]" for="work-block-reason">{t("work.blockReason")}</label>
              <textarea
                id="work-block-reason"
                class="w-full min-h-16 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] px-2 py-1.5 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--brand)]"
                bind:value={blockReason}
              ></textarea>
              <button
                type="button"
                class="h-7 px-2.5 rounded-lg bg-warning/15 text-warning text-[11px] font-semibold cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== "" || blockReason.trim() === ""}
                onclick={() => void submitBlock()}
              >
                {t("work.block")}
              </button>
            </div>
          {/if}

          {#if changesOpen}
            <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-2.5 space-y-2">
              <label class="text-[11px] text-[var(--text-muted)]" for="work-changes">{t("work.changesPlaceholder")}</label>
              <textarea
                id="work-changes"
                class="w-full min-h-16 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] px-2 py-1.5 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--brand)]"
                bind:value={changesText}
              ></textarea>
              <button
                type="button"
                class="h-7 px-2.5 rounded-lg bg-[var(--brand)] text-[var(--text-on-light)] text-[11px] font-semibold cursor-pointer disabled:opacity-50"
                disabled={actionBusy !== "" || changesText.trim() === ""}
                onclick={() => void submitChanges()}
              >
                {t("work.requestChanges")}
              </button>
            </div>
          {/if}
        </section>

        <section class="space-y-2">
          <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
            <MessageSquare class="size-3" />
            {t("work.comments")}
          </div>
          {#if detail.comments.length === 0}
            <p class="text-[11px] text-[var(--text-faint)]">{t("work.noComments")}</p>
          {:else}
            <ul class="space-y-1.5">
              {#each detail.comments as entry (entry.id)}
                <li class="rounded-lg bg-[var(--surface-1)] px-2.5 py-2">
                  <div class="flex items-baseline gap-2 text-[11px]">
                    <span class="font-semibold text-[var(--text-primary)]">{entry.author}</span>
                    <span class="text-[var(--text-faint)]">{formatTime(entry.created_at)}</span>
                  </div>
                  <p class="mt-0.5 text-xs text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap">{entry.body}</p>
                </li>
              {/each}
            </ul>
          {/if}
          <div class="flex items-end gap-1.5">
            <textarea
              class="flex-1 min-h-14 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)] px-2 py-1.5 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--brand)]"
              aria-label={t("work.commentPlaceholder")}
              placeholder={t("work.commentPlaceholder")}
              bind:value={comment}
            ></textarea>
            <button
              type="button"
              class="h-8 px-2.5 rounded-lg bg-[var(--brand)] text-[var(--text-on-light)] text-[11px] font-semibold inline-flex items-center gap-1.5 cursor-pointer disabled:opacity-50 shrink-0"
              disabled={posting || comment.trim() === ""}
              onclick={() => void submitComment()}
            >
              {#if posting}<Loader2 class="size-3 animate-spin" />{:else}<Send class="size-3" />{/if}
              {t("work.addComment")}
            </button>
          </div>
        </section>

        <section class="space-y-2">
          <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
            <Package class="size-3" />
            {t("work.artifacts")}
          </div>
          {#if detail.artifacts.length === 0}
            <p class="text-[11px] text-[var(--text-faint)]">{t("work.noArtifacts")}</p>
          {:else}
            <ul class="space-y-1">
              {#each detail.artifacts as artifact (artifact.id)}
                <li class="rounded-lg bg-[var(--surface-1)] px-2.5 py-1.5">
                  <div class="text-[11px] font-mono text-[var(--text-secondary)] break-all">{artifact.path}</div>
                  <div class="mt-0.5 text-[11px] text-[var(--text-faint)]">
                    {artifact.mime ?? "file"}{#if artifact.size != null} · {artifact.size} B{/if} · {shortSha(artifact.sha256)}
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <section class="space-y-2">
          <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
            <History class="size-3" />
            {t("work.timeline")}
          </div>
          {#if detail.events.length === 0}
            <p class="text-[11px] text-[var(--text-faint)]">{t("work.noEvents")}</p>
          {:else}
            <ol class="space-y-1">
              {#each detail.events as event (event.id)}
                <li class="flex items-baseline gap-2 text-[11px]">
                  <span class="font-mono text-[var(--text-tertiary)] shrink-0">{event.kind.replaceAll("_", " ")}</span>
                  <span class="text-[var(--text-faint)] truncate">{eventSummary(event)}</span>
                  <span class="text-[var(--text-faint)] shrink-0 ml-auto">{formatTime(event.created_at)}</span>
                </li>
              {/each}
            </ol>
          {/if}
        </section>
      {/if}
    </div>
  </div>
</div>
