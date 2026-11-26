<script lang="ts">
  /**
   * Review a skill learned from a completed run.
   *
   * The backend produces a deterministic draft from the run's successful tool
   * trace. This dialog is the review gate: nothing is written until the person
   * corrects the document and presses Save. Saving goes through the same
   * LocalSkillLibrary as every other skill, with `learned:run:<id>` provenance.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { notify } from "$lib/toast";
  import { rememberFocus, trapFocus } from "$lib/a11y";
  import {
    AlertTriangle,
    FileText,
    GraduationCap,
    Loader2,
    X,
  } from "@lucide/svelte";

  interface LearnedStep {
    tool: string;
    action?: string | null;
    summary: string;
    arguments: Record<string, unknown>;
  }

  interface SkillDraft {
    id: string;
    name: string;
    description: string;
    example_prompt: string;
    content: string;
    steps: LearnedStep[];
    tools: string[];
    warnings: string[];
    source_run_id: string;
    source_bot_name: string;
  }

  interface SkillView {
    id: string;
    name: string;
    version: string;
    provenance?: string | null;
  }

  interface Props {
    runId: string;
    onClose: () => void;
  }

  let { runId, onClose }: Props = $props();

  let loading = $state(true);
  let error = $state("");
  let draft = $state<SkillDraft | null>(null);
  let content = $state("");
  let saving = $state<"install" | "save" | null>(null);
  let installAfterSave = $state(true);
  let dialogEl = $state<HTMLElement | null>(null);
  let restoreFocus: (() => void) | null = null;

  onMount(() => {
    restoreFocus = rememberFocus();
    void load();
    return () => {
      if (restoreFocus) restoreFocus();
    };
  });

  $effect(() => {
    const element = dialogEl;
    if (!element) return;
    return trapFocus(element);
  });

  async function load() {
    loading = true;
    error = "";
    try {
      draft = await invoke<SkillDraft>("propose_skill_from_run", { runId });
      content = draft.content;
    } catch (e) {
      error = String(e);
      draft = null;
    } finally {
      loading = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
  }

  async function save(install: boolean) {
    if (!draft || saving) return;
    saving = install ? "install" : "save";
    error = "";
    try {
      const edited: SkillDraft = { ...draft, content };
      const view = await invoke<SkillView>("save_learned_skill", {
        draft: edited,
        install,
      });
      const name = view.name || draft.name;
      notify(
        t(install ? "learning.savedInstalled" : "learning.saved", { name }),
        "success",
      );
      onClose();
    } catch (e) {
      error = String(e);
      saving = null;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-50 bg-[var(--scrim)] flex items-center justify-center p-4"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    class="modal-panel w-full max-w-2xl max-h-[88vh] flex flex-col"
    role="dialog"
    aria-modal="true"
    aria-labelledby="learn-from-run-title"
    tabindex="-1"
    bind:this={dialogEl}
    onkeydown={handleKeydown}
    onclick={(event) => event.stopPropagation()}
  >
    <div class="flex items-center gap-2 border-b border-[var(--hairline)] px-5 py-3">
      <GraduationCap class="size-4 text-[var(--brand-text)]" />
      <h2 id="learn-from-run-title" class="text-sm font-semibold text-[var(--text-primary)]">
        {t("learning.title")}
      </h2>
      <button
        type="button"
        class="icon-btn size-7 ml-auto"
        onclick={onClose}
        aria-label={t("learning.close")}
        title={t("learning.close")}
      >
        <X class="size-3.5" />
      </button>
    </div>

    <div class="overflow-y-auto px-5 py-4 space-y-4">
      {#if loading}
        <p class="flex items-center gap-2 text-xs text-[var(--text-secondary)]">
          <Loader2 class="size-3.5 animate-spin" />
          {t("learning.loading")}
        </p>
      {:else if !draft}
        <div class="rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] p-3 space-y-2">
          <p class="flex items-start gap-2 text-xs text-[var(--danger)]">
            <AlertTriangle class="size-3.5 shrink-0 mt-0.5" />
            <span>
              <span class="font-semibold">{t("learning.loadError")}</span>
              <span class="block break-words">{error}</span>
            </span>
          </p>
          <button
            type="button"
            class="h-7 px-3 rounded-lg border border-[var(--hairline)] text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer"
            onclick={() => void load()}
          >
            {t("learning.retry")}
          </button>
        </div>
      {:else}
        <div class="space-y-1">
          <h3 class="text-sm font-semibold text-[var(--text-primary)]">{draft.name}</h3>
          <p class="text-xs text-[var(--text-secondary)]">{draft.description}</p>
          <p class="text-[11px] font-mono text-[var(--text-faint)]">
            {draft.id} · {t("learning.sourceRun", {
              run: draft.source_run_id.slice(0, 8),
            })}
          </p>
        </div>

        {#if draft.warnings.length}
          <div class="rounded-lg border border-[var(--warning-border)] bg-[var(--warning-soft)] p-3 space-y-1.5">
            <p class="flex items-center gap-1.5 text-[11px] font-semibold text-[var(--warning-text)]">
              <AlertTriangle class="size-3.5" />
              {t("learning.warnings")}
            </p>
            <ul class="space-y-1">
              {#each draft.warnings as warning}
                <li class="learned-warning text-[11px] leading-snug text-[var(--text-secondary)]">
                  {warning}
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <section class="space-y-2">
          <div class="flex items-baseline justify-between gap-2">
            <h4 class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
              {t("learning.steps")}
            </h4>
            <span class="text-[11px] text-[var(--text-faint)]">
              {t("learning.stepCount", { count: draft.steps.length })}
            </span>
          </div>
          {#if draft.steps.length === 0}
            <p class="text-[11px] text-[var(--text-muted)]">{t("learning.emptySteps")}</p>
          {:else}
            <ol class="space-y-1.5 max-h-56 overflow-y-auto no-scrollbar">
              {#each draft.steps as step, index}
                <li class="learned-step rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 space-y-1">
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="text-[11px] font-mono text-[var(--text-faint)] tabular-nums">{index + 1}.</span>
                    <span class="learned-step__tool shrink-0 rounded bg-[var(--surface-3)] px-1.5 py-0.5 text-[11px] font-mono text-[var(--brand-text)]">
                      {step.tool}
                    </span>
                    {#if step.action}
                      <span class="text-[11px] text-[var(--text-faint)] truncate">{step.action}</span>
                    {/if}
                  </div>
                  <p class="learned-step__summary text-[11px] leading-snug text-[var(--text-secondary)] break-words">
                    {step.summary}
                  </p>
                </li>
              {/each}
            </ol>
          {/if}
        </section>

        {#if draft.tools.length}
          <section class="space-y-1.5">
            <h4 class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
              {t("learning.tools")}
            </h4>
            <div class="flex flex-wrap gap-1.5">
              {#each draft.tools as tool}
                <span class="learned-tool rounded-md border border-[var(--hairline)] bg-[var(--surface-1)] px-1.5 py-0.5 text-[11px] font-mono text-[var(--text-secondary)]">
                  {tool}
                </span>
              {/each}
            </div>
          </section>
        {/if}

        <section class="space-y-2">
          <label
            for="learned-skill-content"
            class="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]"
          >
            <FileText class="size-3.5" />
            {t("learning.document")}
          </label>
          <textarea
            id="learned-skill-content"
            class="learned-document w-full h-56 resize-y rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] p-3 font-mono text-[11px] leading-relaxed text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--brand)]"
            bind:value={content}
            aria-describedby="learned-skill-edit-hint"
            spellcheck="false"
          ></textarea>
          <p id="learned-skill-edit-hint" class="text-[11px] text-[var(--text-faint)]">
            {t("learning.editHint")}
          </p>
        </section>

        {#if error}
          <p class="learned-save-error text-[11px] text-danger leading-snug break-words">{error}</p>
        {/if}
      {/if}
    </div>

    <div class="flex flex-wrap items-center gap-2 border-t border-[var(--hairline)] px-5 py-3">
      {#if draft}
        <label class="flex items-center gap-1.5 text-[11px] text-[var(--text-secondary)] cursor-pointer">
          <input
            type="checkbox"
            class="size-3.5 accent-[var(--brand)]"
            bind:checked={installAfterSave}
          />
          {t("learning.install")}
        </label>
      {/if}
      <div class="ml-auto flex items-center gap-2">
        <button
          type="button"
          class="h-7 px-3 rounded-lg border border-[var(--hairline)] text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer"
          onclick={onClose}
        >
          {t("projects.cancel")}
        </button>
        {#if draft}
          <button
            type="button"
            class="h-7 px-3 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer disabled:opacity-50"
            disabled={saving !== null}
            onclick={() => void save(false)}
          >
            {#if saving === "save"}<Loader2 class="size-3 animate-spin inline" />{/if}
            {t("learning.saveWithout")}
          </button>
          <button
            type="button"
            class="h-7 px-3 rounded-lg border border-transparent bg-[var(--surface-light)] text-[11px] font-medium text-[var(--text-on-light)] cursor-pointer disabled:opacity-50"
            disabled={saving !== null}
            onclick={() => void save(installAfterSave)}
          >
            {#if saving === "install"}<Loader2 class="size-3 animate-spin inline" />{/if}
            {installAfterSave ? t("learning.saveInstall") : t("learning.saveOnly")}
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>
