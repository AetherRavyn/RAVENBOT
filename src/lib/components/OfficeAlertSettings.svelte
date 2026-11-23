<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { BellRing, Loader2, Save } from "@lucide/svelte";

  interface Policy {
    enabled: boolean;
    warning_percent: number;
    critical_percent: number;
  }

  let { chatroomId }: { chatroomId: string } = $props();

  let enabled = $state(true);
  let warning = $state("90");
  let critical = $state("100");
  let loading = $state(true);
  let saving = $state(false);
  let notice = $state("");
  let error = $state("");

  async function load() {
    loading = true;
    error = "";
    try {
      const policy = await invoke<Policy>("get_office_alert_policy", {
        chatroomId,
      });
      enabled = policy.enabled;
      warning = String(policy.warning_percent);
      critical = String(policy.critical_percent);
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function save() {
    if (saving) return;
    const warningPercent = Number(warning);
    const criticalPercent = Number(critical);
    if (
      !Number.isFinite(warningPercent) ||
      !Number.isFinite(criticalPercent) ||
      warningPercent < 1 ||
      warningPercent > 100 ||
      criticalPercent < warningPercent ||
      criticalPercent > 100
    ) {
      error = t("office.alertInvalid");
      return;
    }
    saving = true;
    notice = "";
    error = "";
    try {
      const policy = await invoke<Policy>("set_office_alert_policy", {
        chatroomId,
        enabled,
        warningPercent,
        criticalPercent,
      });
      enabled = policy.enabled;
      warning = String(policy.warning_percent);
      critical = String(policy.critical_percent);
      notice = t("office.alertSaved");
    } catch (cause) {
      error = String(cause);
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    const id = chatroomId;
    if (id) void load();
  });
</script>

<section
  class="space-y-3 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4"
  aria-label={t("office.alertTitle")}
>
  <div>
    <h4 class="flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-[var(--text-primary)] font-mono">
      <BellRing class="size-3.5 text-warning" />
      {t("office.alertTitle")}
    </h4>
    <p class="mt-0.5 text-[11px] text-[var(--text-tertiary)]">
      {t("office.alertDesc")}
    </p>
  </div>

  {#if loading}
    <p class="flex items-center gap-2 text-[11px] text-[var(--text-muted)]" role="status">
      <Loader2 class="size-3.5 animate-spin" />
      {t("ui.loading")}
    </p>
  {:else}
    <label class="flex items-center gap-2 text-xs text-[var(--text-secondary)]">
      <input
        type="checkbox"
        checked={enabled}
        onchange={(event) => (enabled = event.currentTarget.checked)}
        disabled={saving}
      />
      {t("office.alertEnabled")}
    </label>

    <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
        <span class="block">{t("office.alertWarning")}</span>
        <input
          class="h-9 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2 font-mono text-xs text-[var(--text-primary)]"
          type="number"
          min="1"
          max="100"
          bind:value={warning}
          disabled={saving || !enabled}
          aria-label={t("office.alertWarning")}
        />
      </label>
      <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
        <span class="block">{t("office.alertCritical")}</span>
        <input
          class="h-9 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2 font-mono text-xs text-[var(--text-primary)]"
          type="number"
          min="1"
          max="100"
          bind:value={critical}
          disabled={saving || !enabled}
          aria-label={t("office.alertCritical")}
        />
      </label>
    </div>
    <p class="text-[11px] leading-4 text-[var(--text-muted)]">
      {t("office.alertHint")}
    </p>

    {#if notice}
      <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
    {/if}
    {#if error}
      <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">{error}</p>
    {/if}

    <div class="flex justify-end">
      <button
        type="button"
        class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] cursor-pointer disabled:opacity-50"
        onclick={() => void save()}
        disabled={saving}
      >
        {#if saving}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <Save class="size-3.5" />
        {/if}
        {t("office.alertSave")}
      </button>
    </div>
  {/if}
</section>
