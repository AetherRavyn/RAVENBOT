<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import {
    AlertTriangle,
    BarChart3,
    Brain,
    Cloud,
    HardDrive,
    Loader2,
    RefreshCw,
    Save,
    Server,
    ShieldCheck,
  } from "@lucide/svelte";

  interface RetrievalEvalQuery {
    query: string;
    expected: string;
    rank: number | null;
    vector_rank: number | null;
    top: string[];
  }

  interface RetrievalEvalReport {
    k: number;
    dimension: number;
    recall_at_k: number;
    mean_reciprocal_rank: number;
    vector_recall_at_k: number;
    vector_mean_reciprocal_rank: number;
    queries: RetrievalEvalQuery[];
  }

  interface MemoryEmbeddingConfig {
    provider: "local" | "ollama" | "openai";
    model: string;
    base_url: string;
    has_api_key: boolean;
    agent_memories: number;
    office_memories: number;
  }

  let config = $state<MemoryEmbeddingConfig | null>(null);
  let provider = $state<"local" | "ollama" | "openai">("local");
  let model = $state("");
  let baseUrl = $state("");
  let apiKey = $state("");
  let busy = $state(false);
  let reindexing = $state(false);
  let loaded = $state(false);
  let notice = $state("");
  let error = $state("");
  let evaluating = $state(false);
  let evalReport = $state<RetrievalEvalReport | null>(null);
  let evalError = $state("");

  const providers = [
    {
      id: "local" as const,
      label: () => t("memoryEmbedding.local"),
      description: () => t("memoryEmbedding.localDesc"),
      icon: HardDrive,
    },
    {
      id: "ollama" as const,
      label: () => t("memoryEmbedding.ollama"),
      description: () => t("memoryEmbedding.ollamaDesc"),
      icon: Server,
    },
    {
      id: "openai" as const,
      label: () => t("memoryEmbedding.openai"),
      description: () => t("memoryEmbedding.openaiDesc"),
      icon: Cloud,
    },
  ];

  const needsKey = $derived(provider === "openai");
  const totalMemories = $derived(
    (config?.agent_memories ?? 0) + (config?.office_memories ?? 0),
  );
  const dirty = $derived(
    config !== null &&
      (provider !== config.provider ||
        (provider !== "local" &&
          (model.trim() !== config.model.trim() ||
            baseUrl.trim() !== config.base_url.trim()))),
  );
  const canSave = $derived(
    loaded &&
      !busy &&
      (provider === "local" ||
        (model.trim().length > 0 &&
          (!needsKey || apiKey.trim().length > 0 || config?.has_api_key))),
  );
  const destination = $derived(baseUrl.trim() || t("memoryEmbedding.configuredEndpoint"));

  function apply(next: MemoryEmbeddingConfig) {
    config = next;
    provider = next.provider;
    model = next.model;
    baseUrl = next.base_url;
    apiKey = "";
  }

  async function load() {
    try {
      apply(await invoke<MemoryEmbeddingConfig>("get_memory_embedding_config"));
      error = "";
    } catch (cause) {
      error = String(cause);
    } finally {
      loaded = true;
    }
  }

  function defaultModel(id: "local" | "ollama" | "openai"): string {
    if (id === "ollama") return "nomic-embed-text";
    if (id === "openai") return "text-embedding-3-small";
    return "";
  }

  function choose(id: "local" | "ollama" | "openai") {
    if (provider === id) return;
    provider = id;
    // Restore the saved values when returning to the configured backend;
    // otherwise start from sensible defaults instead of carrying the previous
    // provider's model or URL across. The backend still fills the endpoint.
    const saved = config?.provider === id ? config : null;
    model = saved?.model ?? defaultModel(id);
    baseUrl = saved?.base_url ?? "";
    apiKey = "";
    notice = "";
    error = "";
    evalReport = null;
    evalError = "";
  }

  async function save() {
    if (!canSave) return;
    busy = true;
    notice = "";
    error = "";
    evalReport = null;
    evalError = "";
    try {
      const next = await invoke<MemoryEmbeddingConfig>(
        "set_memory_embedding_config",
        {
          provider,
          model: model.trim() || null,
          baseUrl: baseUrl.trim() || null,
          apiKey: apiKey.trim() || null,
        },
      );
      apply(next);
      const count = next.agent_memories + next.office_memories;
      notice = t("memoryEmbedding.saved", { count });
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function reindex() {
    if (reindexing || busy || dirty) return;
    reindexing = true;
    notice = "";
    error = "";
    evalReport = null;
    evalError = "";
    try {
      const result = await invoke<{ agent: number; office: number }>(
        "reindex_memory_embeddings",
      );
      notice = t("memoryEmbedding.reindexed", {
        count: result.agent + result.office,
      });
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      reindexing = false;
    }
  }

  async function evaluate() {
    if (!config || evaluating || busy || reindexing || dirty) return;
    evaluating = true;
    evalReport = null;
    evalError = "";
    try {
      evalReport = await invoke<RetrievalEvalReport>(
        "evaluate_memory_retrieval",
      );
    } catch (cause) {
      evalError = String(cause);
    } finally {
      evaluating = false;
    }
  }

  onMount(() => void load());
</script>

<div class="space-y-4">
  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-5 space-y-4">
    <div class="flex items-start gap-3">
      <div class="flex size-9 shrink-0 items-center justify-center rounded-xl border border-[var(--brand)]/25 bg-[var(--brand-soft)]">
        <Brain class="size-4 text-[var(--brand-text)]" />
      </div>
      <div class="min-w-0">
        <h3 class="text-sm font-bold text-[var(--text-primary)]">
          {t("memoryEmbedding.title")}
        </h3>
        <p class="mt-1 text-[11px] leading-5 text-[var(--text-tertiary)]">
          {t("memoryEmbedding.desc")}
        </p>
      </div>
    </div>

    <div>
      <Label class="text-xs font-bold text-[var(--text-secondary)]">
        {t("memoryEmbedding.provider")}
      </Label>
      <div class="mt-2 grid gap-2 sm:grid-cols-3">
        {#each providers as option (option.id)}
          {@const Icon = option.icon}
          <button
            type="button"
            class="flex min-h-24 flex-col items-start gap-1.5 rounded-xl border p-3 text-left transition-colors cursor-pointer {provider ===
            option.id
              ? 'border-[var(--brand)]/50 bg-[var(--brand-soft)]'
              : 'border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--hairline-strong)]'}"
            aria-pressed={provider === option.id}
            onclick={() => choose(option.id)}
            disabled={busy || !loaded}
          >
            <span class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-primary)]">
              <Icon class="size-3.5 text-[var(--brand-text)]" />
              {option.label()}
            </span>
            <span class="text-[11px] leading-4 text-[var(--text-tertiary)]">
              {option.description()}
            </span>
          </button>
        {/each}
      </div>
    </div>

    {#if provider !== "local"}
      <div class="grid gap-3 sm:grid-cols-2">
        <label class="space-y-1.5 text-[11px] text-[var(--text-secondary)]">
          <span class="block font-semibold">{t("memoryEmbedding.model")}</span>
          <Input
            bind:value={model}
            aria-label={t("memoryEmbedding.model")}
            placeholder={provider === "ollama"
              ? t("memoryEmbedding.modelPlaceholderOllama")
              : t("memoryEmbedding.modelPlaceholderOpenai")}
            class="h-8.5 border-[var(--hairline)] bg-[var(--surface-2)] text-xs"
            disabled={busy}
          />
        </label>
        <label class="space-y-1.5 text-[11px] text-[var(--text-secondary)]">
          <span class="block font-semibold">{t("memoryEmbedding.baseUrl")}</span>
          <Input
            bind:value={baseUrl}
            aria-label={t("memoryEmbedding.baseUrl")}
            placeholder={provider === "ollama"
              ? "http://127.0.0.1:11434"
              : "https://api.openai.com/v1"}
            class="h-8.5 border-[var(--hairline)] bg-[var(--surface-2)] font-mono text-xs"
            disabled={busy}
          />
        </label>
      </div>

      {#if needsKey}
        <label class="block space-y-1.5 text-[11px] text-[var(--text-secondary)]">
          <span class="block font-semibold">{t("memoryEmbedding.apiKey")}</span>
          <Input
            bind:value={apiKey}
            aria-label={t("memoryEmbedding.apiKey")}
            type="password"
            autocomplete="off"
            spellcheck="false"
            placeholder={config?.has_api_key
              ? t("memoryEmbedding.apiKeyHint")
              : t("memoryEmbedding.apiKeyPlaceholder")}
            class="h-8.5 border-[var(--hairline)] bg-[var(--surface-2)] font-mono text-xs"
            disabled={busy}
          />
          {#if config?.has_api_key}
            <span class="flex items-center gap-1 text-[11px] text-[var(--success-text)]">
              <ShieldCheck class="size-3" />
              {t("memoryEmbedding.apiKeySaved")}
            </span>
          {/if}
        </label>
      {/if}

      <p class="text-[11px] leading-4 text-[var(--text-muted)]">
        {provider === "ollama"
          ? t("memoryEmbedding.baseUrlHintOllama")
          : t("memoryEmbedding.baseUrlHintOpenai")}
      </p>
    {/if}

    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-[var(--hairline)] pt-3 text-[11px] text-[var(--text-tertiary)]">
      <span>
        {t("memoryEmbedding.counts", {
          agent: config?.agent_memories ?? 0,
          office: config?.office_memories ?? 0,
        })}
      </span>
      {#if totalMemories > 0}
        <span class="flex items-center gap-1 text-[var(--text-muted)]">
          <RefreshCw class="size-3" />
          {t("memoryEmbedding.warningReindex", { count: totalMemories })}
        </span>
      {/if}
    </div>

    {#if provider === "openai"}
      <p class="flex items-start gap-1.5 rounded-lg border border-[var(--warning-border)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] leading-4 text-[var(--warning-text)]">
        <AlertTriangle class="mt-0.5 size-3.5 shrink-0" />
        <span>{t("memoryEmbedding.warningRemote", { host: destination })}</span>
      </p>
    {:else if provider === "ollama"}
      <p class="flex items-start gap-1.5 rounded-lg border border-[var(--success-text)]/25 bg-[var(--success-soft)] px-2.5 py-2 text-[11px] leading-4 text-[var(--success-text)]">
        <ShieldCheck class="mt-0.5 size-3.5 shrink-0" />
        <span>{t("memoryEmbedding.noteOllama")}</span>
      </p>
    {/if}

    {#if notice}
      <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
    {/if}
    {#if error}
      <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">{error}</p>
    {/if}

    <div class="flex flex-wrap items-center gap-2">
      <Button
        size="sm"
        class="h-8.5 cursor-pointer gap-1.5 bg-[var(--brand)] text-xs font-medium text-[var(--text-on-light)] hover:bg-[var(--brand-hover)]"
        onclick={() => void save()}
        disabled={!canSave}
      >
        {#if busy}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <Save class="size-3.5" />
        {/if}
        {busy ? t("memoryEmbedding.saving") : t("memoryEmbedding.save")}
      </Button>
      <Button
        size="sm"
        variant="outline"
        class="h-8.5 cursor-pointer gap-1.5 border-[var(--hairline)] text-xs"
        onclick={() => void reindex()}
        disabled={!config || !loaded || busy || reindexing || dirty}
        title={dirty
          ? t("memoryEmbedding.unsavedReindex")
          : t("memoryEmbedding.reindexHint")}
      >
        {#if reindexing}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <RefreshCw class="size-3.5" />
        {/if}
        {reindexing
          ? t("memoryEmbedding.reindexing")
          : t("memoryEmbedding.reindex")}
      </Button>
      {#if dirty}
        <span class="text-[11px] text-[var(--warning-text)]">
          {t("memoryEmbedding.unsavedReindex")}
        </span>
      {/if}
    </div>
  </div>

  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-5 space-y-3">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="flex items-center gap-1.5 text-xs font-bold text-[var(--text-primary)]">
          <BarChart3 class="size-4 text-[var(--brand-text)]" />
          {t("memoryEmbedding.evalTitle")}
        </p>
        <p class="mt-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
          {t("memoryEmbedding.evalFixtureNote")}
        </p>
      </div>
      <Button
        size="sm"
        variant="outline"
        class="h-8.5 shrink-0 cursor-pointer gap-1.5 border-[var(--hairline)] text-xs"
        onclick={() => void evaluate()}
        disabled={!config || busy || reindexing || evaluating || dirty}
        title={dirty
          ? t("memoryEmbedding.unsavedReindex")
          : t("memoryEmbedding.evalHint")}
      >
        {#if evaluating}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <BarChart3 class="size-3.5" />
        {/if}
        {evaluating
          ? t("memoryEmbedding.evaluating")
          : t("memoryEmbedding.evaluate")}
      </Button>
    </div>

    {#if evalError}
      <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
        {evalError}
      </p>
    {/if}

    {#if evalReport}
      <div class="grid grid-cols-2 gap-2">
        <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-3">
          <p class="text-[11px] text-[var(--text-tertiary)]">
            {t("memoryEmbedding.evalRecall", { k: evalReport.k })}
          </p>
          <p class="text-xl font-bold text-[var(--text-primary)]">
            {Math.round(evalReport.recall_at_k * 100)}%
          </p>
        </div>
        <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-3">
          <p class="text-[11px] text-[var(--text-tertiary)]">
            {t("memoryEmbedding.evalMrr")}
          </p>
          <p class="text-xl font-bold text-[var(--text-primary)]">
            {evalReport.mean_reciprocal_rank.toFixed(2)}
          </p>
        </div>
      </div>
      <p class="font-mono text-[11px] text-[var(--text-muted)]">
        {t("memoryEmbedding.evalDimension", {
          dimension: evalReport.dimension,
        })}
      </p>
      <p class="text-[11px] leading-4 text-[var(--text-tertiary)]">
        {t("memoryEmbedding.evalVectorOnly", {
          recall: Math.round(evalReport.vector_recall_at_k * 100),
          mrr: evalReport.vector_mean_reciprocal_rank.toFixed(2),
        })}
      </p>
      <div class="max-h-56 space-y-1 overflow-y-auto no-scrollbar pr-1">
        {#each evalReport.queries as query, index (index)}
          <div class="flex items-center justify-between gap-2 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 py-1.5">
            <span class="min-w-0 truncate text-[11px] text-[var(--text-secondary)]">
              {query.query}
            </span>
            {#if query.rank !== null && query.rank <= evalReport.k}
              <span class="shrink-0 font-mono text-[11px] text-[var(--success-text)]">
                {t("memoryEmbedding.evalRank", { rank: query.rank })}
              </span>
            {:else}
              <span class="shrink-0 font-mono text-[11px] text-[var(--danger-text)]">
                {t("memoryEmbedding.evalMiss")}
              </span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
