<script lang="ts">
  /**
   * The starter-role strip in the create-agent dialog.
   *
   * Picking a card only prefills the form; it does not create anything and
   * every field remains editable. The same component is used in the agent
   * picker so a template has one visual definition.
   */
  import { Receipt, Telescope, LifeBuoy, Rocket, Check } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import {
    AGENT_TEMPLATES,
    type AgentTemplate,
    type AgentTemplateId,
  } from "$lib/agentTemplates";

  interface Props {
    selectedId?: AgentTemplateId | null;
    onSelect: (template: AgentTemplate) => void;
  }

  let { selectedId = null, onSelect }: Props = $props();

  const ICONS = {
    billing: Receipt,
    research: Telescope,
    support: LifeBuoy,
    release: Rocket,
  } as const;
</script>

<div class="space-y-1.5">
  <div class="flex items-baseline justify-between gap-2">
    <span class="text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
      {t("agentTemplates.title")}
    </span>
    <span class="truncate text-[11px] text-[var(--text-muted)]">{t("agentTemplates.hint")}</span>
  </div>
  <div class="grid grid-cols-2 gap-1.5" role="group" aria-label={t("agentTemplates.title")}>
    {#each AGENT_TEMPLATES as template (template.id)}
      {@const Icon = ICONS[template.icon]}
      {@const selected = selectedId === template.id}
      <button
        type="button"
        class="flex min-w-0 items-start gap-2 rounded-xl border px-2.5 py-2 text-left cursor-pointer transition-colors {selected
          ? 'border-[var(--brand)] bg-[var(--brand-soft)]'
          : 'border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-3)]'}"
        aria-pressed={selected}
        onclick={() => onSelect(template)}
      >
        <span class="mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] text-[var(--brand-text)]">
          <Icon class="size-3.5" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="flex items-center gap-1">
            <span class="truncate text-xs font-semibold text-[var(--text-primary)]">{t(template.nameKey)}</span>
            {#if selected}
              <Check class="size-3 shrink-0 text-[var(--success-text)]" aria-hidden="true" />
            {/if}
          </span>
          <span class="mt-0.5 line-clamp-2 text-[11px] leading-4 text-[var(--text-tertiary)]">{t(template.descriptionKey)}</span>
        </span>
      </button>
    {/each}
  </div>
</div>
