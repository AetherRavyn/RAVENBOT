<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t, type TranslationKey } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import * as Dialog from "$lib/components/ui/dialog";
  import type {
    Bot,
    InstalledJob,
    JobCadence,
    JobGroup,
    JobOversight,
    JobTemplate,
  } from "$lib/types";
  import {
    ArrowRight,
    Briefcase,
    CheckCircle2,
    Clock,
    Eye,
    FileText,
    Loader2,
    RefreshCw,
    Search,
    ShieldCheck,
    TriangleAlert,
  } from "@lucide/svelte";

  interface Props {
    /** Current fleet, used only for the “installed” badge. */
    bots?: Bot[];
    onOpenAgent?: (bot: Bot) => void;
    onOpenRoutine?: (botId: string) => void;
  }

  let { bots = [], onOpenAgent, onOpenRoutine }: Props = $props();

  type GroupFilter = "all" | JobGroup;
  type CadenceFilter = "all" | JobCadence;

  const groups: { id: GroupFilter; key: string }[] = [
    { id: "all", key: "jobs.allGroups" },
    { id: "marketing", key: "jobs.group.marketing" },
    { id: "sales", key: "jobs.group.sales" },
    { id: "engineering", key: "jobs.group.engineering" },
    { id: "operations", key: "jobs.group.operations" },
    { id: "finance", key: "jobs.group.finance" },
    { id: "people", key: "jobs.group.people" },
    { id: "research", key: "jobs.group.research" },
    { id: "personal", key: "jobs.group.personal" },
  ];

  const cadences: { id: CadenceFilter; key: string }[] = [
    { id: "all", key: "jobs.allCadences" },
    { id: "one_off", key: "jobs.cadence.one_off" },
    { id: "recurring", key: "jobs.cadence.recurring" },
    { id: "monitoring", key: "jobs.cadence.monitoring" },
  ];

  const oversightOptions: { id: JobOversight; name: string; desc: string }[] = [
    { id: "low", name: "jobs.oversight.low", desc: "jobs.oversight.lowDesc" },
    { id: "review", name: "jobs.oversight.review", desc: "jobs.oversight.reviewDesc" },
    {
      id: "approval",
      name: "jobs.oversight.approval",
      desc: "jobs.oversight.approvalDesc",
    },
  ];

  let jobs = $state<JobTemplate[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let query = $state("");
  let group = $state<GroupFilter>("all");
  let cadence = $state<CadenceFilter>("all");

  let selected = $state<JobTemplate | null>(null);
  let installName = $state("");
  let installSchedule = $state("");
  let installOversight = $state<JobOversight>("review");
  let installing = $state(false);
  let installError = $state<string | null>(null);
  let installed = $state<InstalledJob | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      jobs = await invoke<JobTemplate[]>("list_job_library");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  let filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    return jobs.filter((job) => {
      if (group !== "all" && job.group !== group) return false;
      if (cadence !== "all" && job.cadence !== cadence) return false;
      if (!needle) return true;
      return `${job.name} ${job.summary} ${job.role_title} ${job.deliverable}`
        .toLowerCase()
        .includes(needle);
    });
  });

  function installedCount(jobId: string): number {
    return bots.filter((bot) => bot.config?.job_id === jobId).length;
  }

  function openJob(job: JobTemplate) {
    selected = job;
    installName = job.role_title;
    installSchedule = job.schedule;
    installOversight = job.oversight;
    installError = null;
    installed = null;
  }

  function closeJob() {
    selected = null;
    installed = null;
    installError = null;
  }

  function label(key: string): string {
    return t(key as TranslationKey);
  }

  function openInstalledAgent() {
    if (installed) onOpenAgent?.(installed.bot);
  }

  function openInstalledRoutine() {
    if (installed) onOpenRoutine?.(installed.bot.id);
  }

  function scheduleLabel(job: JobTemplate): string {
    return job.schedule === "@manual" ? t("jobs.manual") : job.schedule;
  }

  async function install() {
    if (!selected || installing) return;
    installing = true;
    installError = null;
    try {
      installed = await invoke<InstalledJob>("install_job", {
        jobId: selected.id,
        name: installName.trim() || null,
        schedule:
          selected.cadence === "one_off" ? null : installSchedule.trim() || null,
        oversight: installOversight,
      });
      window.dispatchEvent(new CustomEvent("fleet-changed"));
    } catch (e) {
      installError = String(e);
    } finally {
      installing = false;
    }
  }
</script>

<div class="h-full min-h-0 overflow-y-auto px-4 py-3">
  <div class="mx-auto max-w-5xl space-y-4">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div class="flex items-start gap-2.5 min-w-0">
        <div
          class="size-9 shrink-0 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-[var(--brand-text)] flex items-center justify-center"
        >
          <Briefcase class="size-4" />
        </div>
        <div class="min-w-0">
          <h1 class="text-[13px] font-bold text-[var(--text-primary)] truncate">
            {t("jobs.title")}
          </h1>
          <p class="mt-0.5 text-[11px] leading-relaxed text-[var(--text-tertiary)]">
            {t("jobs.subtitle")}
          </p>
        </div>
      </div>
      <span
        class="shrink-0 rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-2 py-1 text-[11px] font-medium text-[var(--text-secondary)]"
      >
        {t("jobs.jobCount", { count: jobs.length })}
      </span>
    </header>

    <div class="relative">
      <Search
        class="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-[var(--text-muted)]"
      />
      <Input
        bind:value={query}
        class="h-9 pl-9 text-xs"
        placeholder={t("jobs.search")}
        aria-label={t("jobs.search")}
      />
    </div>

    <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label={t("jobs.allGroups")}>
      {#each groups as item (item.id)}
        <button
          type="button"
          aria-pressed={group === item.id}
          onclick={() => (group = item.id)}
          class="rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors cursor-pointer {group ===
          item.id
            ? 'border-[var(--brand)]/45 bg-[var(--brand-soft)] text-[var(--brand-text)]'
            : 'border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)]'}"
        >
          {label(item.key)}
        </button>
      {/each}
    </div>

    <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label={t("jobs.allCadences")}>
      {#each cadences as item (item.id)}
        <button
          type="button"
          aria-pressed={cadence === item.id}
          onclick={() => (cadence = item.id)}
          class="rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors cursor-pointer {cadence ===
          item.id
            ? 'border-[var(--brand)]/45 bg-[var(--brand-soft)] text-[var(--brand-text)]'
            : 'border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)]'}"
        >
          {label(item.key)}
        </button>
      {/each}
    </div>

    {#if loading}
      <div class="flex items-center justify-center gap-2 py-16 text-xs text-[var(--text-muted)]" role="status">
        <Loader2 class="size-4 animate-spin" />
        {t("jobs.loading")}
      </div>
    {:else if error}
      <div
        class="flex flex-col items-start gap-2 rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] p-4 text-xs text-[var(--danger-text)]"
        role="alert"
      >
        <p class="font-semibold">{t("jobs.error")}</p>
        <p class="break-words">{error}</p>
        <Button variant="outline" size="sm" onclick={load}>
          <RefreshCw class="size-3.5" />
          {t("ui.retry")}
        </Button>
      </div>
    {:else if filtered.length === 0}
      <div class="py-16 text-center text-xs text-[var(--text-muted)]">{t("jobs.empty")}</div>
    {:else}
      <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {#each filtered as job (job.id)}
          {@const count = installedCount(job.id)}
          <button
            type="button"
            onclick={() => openJob(job)}
            class="group flex h-full flex-col rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3.5 text-left transition-colors cursor-pointer hover:border-[var(--brand)]/35 hover:bg-[var(--surface-2)]"
          >
            <div class="flex items-start justify-between gap-2">
              <div class="flex flex-wrap items-center gap-1.5">
                <span
                  class="rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 py-0.5 text-[11px] font-medium text-[var(--text-secondary)]"
                >
                  {label(`jobs.group.${job.group}`)}
                </span>
                <span class="text-[11px] text-[var(--text-muted)]">
                  {label(`jobs.cadence.${job.cadence}`)}
                </span>
              </div>
              {#if count > 0}
                <span
                  class="shrink-0 rounded-full bg-[var(--success-soft)] px-1.5 py-0.5 text-[11px] font-semibold text-[var(--success-text)]"
                >
                  {count === 1 ? t("jobs.installed") : t("jobs.installedCount", { count })}
                </span>
              {/if}
            </div>
            <h2 class="mt-2 text-[13px] font-bold text-[var(--text-primary)]">{job.name}</h2>
            <p class="mt-1 flex-1 text-[11px] leading-relaxed text-[var(--text-tertiary)]">
              {job.summary}
            </p>
            <div
              class="mt-2.5 flex items-center gap-2 border-t border-[var(--hairline-faint)] pt-2 text-[11px] text-[var(--text-muted)]"
            >
              <Clock class="size-3" />
              <span class="font-mono truncate">{scheduleLabel(job)}</span>
              <span class="ml-auto shrink-0">{label(`jobs.oversight.${job.oversight}`)}</span>
            </div>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<Dialog.Root
  open={!!selected}
  onOpenChange={(open) => {
    if (!open) closeJob();
  }}
>
  <Dialog.Content
    class="sm:max-w-2xl max-h-[88vh] overflow-y-auto bg-[var(--surface-1)] border-[var(--hairline-strong)] rounded-xl"
  >
    {#if selected}
      <div class="space-y-4">
        <div>
          <div class="flex flex-wrap items-center gap-1.5">
            <span
              class="rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 py-0.5 text-[11px] font-medium text-[var(--text-secondary)]"
            >
              {label(`jobs.group.${selected.group}`)}
            </span>
            <span class="text-[11px] text-[var(--text-muted)]">
              {label(`jobs.cadence.${selected.cadence}`)}
            </span>
            {#if installedCount(selected.id) > 0}
              <span class="text-[11px] font-medium text-[var(--success-text)]">
                {t("jobs.installed")}
              </span>
            {/if}
          </div>
          <h2 class="mt-1.5 text-lg font-bold text-[var(--text-primary)]">{selected.name}</h2>
          <p class="mt-1 text-xs leading-relaxed text-[var(--text-tertiary)]">
            {selected.summary}
          </p>
        </div>

        {#if installed}
          <div
            class="space-y-2 rounded-xl border border-[var(--success-text)]/30 bg-[var(--success-soft)] p-3.5"
            role="status"
          >
            <div class="flex items-center gap-2 text-sm font-bold text-[var(--success-text)]">
              <CheckCircle2 class="size-4" />
              {t("jobs.installedTitle")}
            </div>
            <p class="text-xs leading-relaxed text-[var(--text-secondary)]">
              {t("jobs.installedBody", {
                name: installed.bot.name,
                routine: installed.routine.name,
              })}
            </p>
            {#if installed.duplicate}
              <p class="text-[11px] text-[var(--text-tertiary)]">{t("jobs.duplicate")}</p>
            {/if}
            <p class="text-[11px] text-[var(--text-tertiary)]">{t("jobs.pausedNote")}</p>
            <div class="flex flex-wrap gap-2 pt-1">
              <Button size="sm" onclick={openInstalledAgent}>
                {t("jobs.openAgent")}
                <ArrowRight class="size-3.5" />
              </Button>
              <Button variant="outline" size="sm" onclick={openInstalledRoutine}>
                {t("jobs.openRoutine")}
              </Button>
            </div>
          </div>
        {:else}
          <section class="space-y-1.5">
            <h3 class="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wide text-[var(--text-secondary)]">
              <Eye class="size-3.5" />
              {t("jobs.role")}
            </h3>
            <p class="text-xs font-semibold text-[var(--text-primary)]">{selected.role_title}</p>
            <p class="text-[11px] leading-relaxed text-[var(--text-tertiary)]">
              {selected.agent_prompt}
            </p>
          </section>

          <section class="space-y-1.5">
            <h3 class="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wide text-[var(--text-secondary)]">
              <FileText class="size-3.5" />
              {t("jobs.routine")}
            </h3>
            <p class="text-[11px] leading-relaxed text-[var(--text-tertiary)]">
              {selected.routine_instruction}
            </p>
            <p class="text-[11px] text-[var(--text-secondary)]">
              <span class="font-semibold">{t("jobs.deliverable")}:</span>
              {selected.deliverable}
            </p>
          </section>

          <div class="grid gap-3 sm:grid-cols-2">
            <div class="space-y-1">
              <Label for="job-agent-name">{t("jobs.agentName")}</Label>
              <Input
                id="job-agent-name"
                bind:value={installName}
                maxlength={80}
                class="h-9 text-xs"
              />
            </div>
            <div class="space-y-1">
              <Label for="job-schedule">{t("jobs.schedule")}</Label>
              {#if selected.cadence === "one_off"}
                <div
                  class="flex h-9 items-center rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 text-xs text-[var(--text-secondary)]"
                >
                  {t("jobs.manual")}
                </div>
                <p class="text-[11px] leading-relaxed text-[var(--text-muted)]">
                  {t("jobs.manualHint")}
                </p>
              {:else}
                <Input
                  id="job-schedule"
                  bind:value={installSchedule}
                  class="h-9 font-mono text-xs"
                  aria-describedby="job-schedule-hint"
                />
                <p
                  id="job-schedule-hint"
                  class="text-[11px] leading-relaxed text-[var(--text-muted)]"
                >
                  {t("jobs.scheduleHint")}
                </p>
              {/if}
            </div>
          </div>

          <div class="space-y-1.5">
            <div class="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wide text-[var(--text-secondary)]">
              <ShieldCheck class="size-3.5" />
              {t("jobs.oversightTitle")}
            </div>
            <div class="grid gap-2 sm:grid-cols-3">
              {#each oversightOptions as option (option.id)}
                <button
                  type="button"
                  aria-pressed={installOversight === option.id}
                  onclick={() => (installOversight = option.id)}
                  class="rounded-xl border p-2.5 text-left transition-colors cursor-pointer {installOversight ===
                  option.id
                    ? 'border-[var(--warning-text)] bg-[var(--warning-soft)]'
                    : 'border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--hairline-strong)]'}"
                >
                  <div class="text-xs font-bold text-[var(--text-primary)]">
                    {label(option.name)}
                  </div>
                  <div class="mt-0.5 text-[11px] leading-relaxed text-[var(--text-tertiary)]">
                    {label(option.desc)}
                  </div>
                </button>
              {/each}
            </div>
            <p class="text-[11px] leading-relaxed text-[var(--text-muted)]">
              {t("jobs.oversightHint")}
            </p>
          </div>

          {#if installError}
            <div
              class="flex items-start gap-2 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2.5 text-[11px] text-[var(--danger-text)]"
              role="alert"
            >
              <TriangleAlert class="mt-0.5 size-3.5 shrink-0" />
              <span class="break-words">{installError}</span>
            </div>
          {/if}

          <div class="flex justify-end gap-2 border-t border-[var(--hairline)] pt-3">
            <Button variant="ghost" size="sm" onclick={closeJob}>
              {t("ui.cancel")}
            </Button>
            <Button size="sm" onclick={install} disabled={installing || !installName.trim()}>
              {#if installing}
                <Loader2 class="size-3.5 animate-spin" />
                {t("jobs.installing")}
              {:else}
                {t("jobs.install")}
              {/if}
            </Button>
          </div>
        {/if}
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
