<script lang="ts">
  /**
   * The Home setup card.
   *
   * It is a projection of the first-run facts, not a wizard: each row says
   * whether the thing exists, and only the next incomplete row has an action.
   * That keeps a dismissed onboarding modal from hiding missing setup behind a
   * clean Home screen.
   */
  import { Check, Circle, ArrowRight } from "@lucide/svelte";
  import { t, type TranslationKey } from "$lib/i18n";
  import {
    setupComplete,
    type SetupStep,
    type SetupStepId,
  } from "$lib/workspace/setupChecklist";

  interface Props {
    steps: SetupStep[];
    next: SetupStepId | null;
    onAction: (id: SetupStepId) => void;
  }

  let { steps, next, onAction }: Props = $props();

  const TITLE: Record<SetupStepId, TranslationKey> = {
    provider: "setup.provider",
    agent: "setup.agent",
    office: "setup.office",
  };
  const HINT: Record<SetupStepId, TranslationKey> = {
    provider: "setup.providerHint",
    agent: "setup.agentHint",
    office: "setup.officeHint",
  };
  const ACTION: Record<SetupStepId, TranslationKey> = {
    provider: "setup.providerAction",
    agent: "setup.agentAction",
    office: "setup.officeAction",
  };

  let done = $derived(steps.filter((step) => step.done).length);
  let complete = $derived(setupComplete(steps));
</script>

{#if !complete}
  <section
    class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3.5 text-left"
    aria-labelledby="setup-checklist-title"
  >
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <h2 id="setup-checklist-title" class="text-xs font-bold text-[var(--text-primary)]">
          {t("setup.title")}
        </h2>
        <p class="mt-0.5 text-[11px] text-[var(--text-tertiary)]" aria-live="polite">
          {t("setup.progress", { done, total: steps.length })}
        </p>
      </div>
      <span class="shrink-0 font-mono text-[11px] text-[var(--text-muted)]" aria-hidden="true">
        {done}/{steps.length}
      </span>
    </div>

    <ol class="mt-2.5 space-y-1.5">
      {#each steps as step (step.id)}
        <li class="flex min-w-0 items-center gap-2 rounded-lg px-1 py-1">
          <span
            class="flex size-5 shrink-0 items-center justify-center rounded-full {step.done
              ? 'bg-[var(--success-soft)] text-[var(--success-text)]'
              : 'border border-[var(--hairline-strong)] text-[var(--text-muted)]'}"
            aria-hidden="true"
          >
            {#if step.done}
              <Check class="size-3" />
            {:else}
              <Circle class="size-2.5" />
            {/if}
          </span>
          <span class="min-w-0 flex-1">
            <span class="block truncate text-xs font-medium {step.done ? 'text-[var(--text-tertiary)] line-through' : 'text-[var(--text-secondary)]'}">
              {t(TITLE[step.id])}
            </span>
            {#if !step.done}
              <span class="block truncate text-[11px] text-[var(--text-muted)]">{t(HINT[step.id])}</span>
            {/if}
          </span>
          {#if step.done}
            <span class="shrink-0 font-mono text-[11px] text-[var(--success-text)]">{t("setup.done")}</span>
          {:else if next === step.id}
            <button
              type="button"
              class="inline-flex h-7 shrink-0 items-center gap-1 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2 text-[11px] font-semibold text-[var(--text-secondary)] hover:border-[var(--hairline-strong)] hover:text-[var(--text-primary)] cursor-pointer"
              onclick={() => onAction(step.id)}
            >
              {t(ACTION[step.id])}
              <ArrowRight class="size-3" />
            </button>
          {/if}
        </li>
      {/each}
    </ol>
  </section>
{/if}
