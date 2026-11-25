<script lang="ts">
  /**
   * First-run setup.
   *
   * Two things have to be true before RAVENBOT can design an office:
   *   1. a provider key exists, so the app can think;
   *   2. a default agent exists, so the app knows which agent designs offices,
   *      runs as the primary assistant, and which model it uses.
   *
   * The second step used to be implicit — the app guessed a model and designed
   * offices with no identity behind it. Asking once produces a real bot the
   * user can rename, re-model, or replace later, and gives every office a named
   * designer instead of an anonymous completion.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { Key, ChevronRight, X, Sparkles, Bot, Loader2, CheckCircle2, AlertTriangle } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  interface Props {
    open: boolean;
    onOpenSettings: () => void;
    onSkip: () => void;
  }

  let { open, onOpenSettings, onSkip }: Props = $props();

  let step = $state<0 | 1 | 2>(0);
  let checking = $state(false);
  let creating = $state(false);
  let error = $state<string | null>(null);
  let agentName = $state("The Raven");
  let provider = $state("");
  let model = $state("");
  let hasKeys = $state(false);
  let existingAgent = $state<any | null>(null);
  let localModels = $state<any[]>([]);
  let localAvailable = $derived(localModels.length > 0);
  let preferredLocal = $derived(
    localModels.find((m: any) => m.supports_tools) ?? localModels[0] ?? null,
  );

  async function check() {
    checking = true;
    error = null;
    try {
      const [keys, agent, local] = await Promise.all([
        invoke<string[]>("get_configured_providers"),
        invoke<any>("get_default_agent").catch(() => null),
        invoke<any>("probe_ollama").catch(() => null),
      ]);
      hasKeys = Array.isArray(keys) && keys.length > 0;
      existingAgent = agent ?? null;
      localModels = Array.isArray(local?.models) ? local.models : [];
      const localAgent =
        existingAgent?.config?.model_provider === "ollama" ||
        existingAgent?.config?.model_provider === "local";
      // A saved architect is not "ready" if the provider it needs has neither
      // a key nor a local endpoint.
      if (!hasKeys && !localAgent) {
        step = 0;
        return;
      }
      if (existingAgent) {
        step = 2;
        return;
      }
      const def: any = await invoke("get_default_model").catch(() => null);
      // Prefer the configured default; fall back to the keyless local path
      // rather than inventing a cloud provider the user may not have.
      provider = def?.provider || "ollama";
      model = def?.model || preferredLocal?.id || "llama3.1";
      step = 1;
    } catch (e) {
      error = String(e);
      step = 0;
    } finally {
      checking = false;
    }
  }

  function useLocalOllama() {
    if (!preferredLocal) return;
    provider = "ollama";
    model = preferredLocal.id || preferredLocal.name || "llama3.1";
    step = 1;
  }

  // Re-check whenever the overlay opens. `open` is the only dependency: the
  // user can add a key in Settings while this is mounted.
  $effect(() => {
    if (open) void check();
  });

  async function createAgent() {
    if (creating) return;
    creating = true;
    error = null;
    try {
      const bot = await invoke<any>("create_default_agent", {
        name: agentName.trim() || "The Raven",
        provider: provider || undefined,
        model: model || undefined,
      });
      window.dispatchEvent(new CustomEvent("bots-changed"));
      existingAgent = bot;
      step = 2;
      onSkip();
    } catch (e) {
      error = String(e);
    } finally {
      creating = false;
    }
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--surface-0)]/80 backdrop-blur-sm animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
    aria-labelledby="onboarding-title"
  >
    <div class="w-full max-w-md mx-4 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] shadow-2xl overflow-hidden">
      <!-- Header -->
      <div class="px-6 pt-6 pb-4">
        <div class="flex items-center gap-3 mb-3">
          <div class="size-10 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/30 flex items-center justify-center shrink-0">
            {#if step === 2}<CheckCircle2 class="size-5 text-success" />{:else}<Key class="size-5 text-[var(--brand-text)]" />{/if}
          </div>
          <div class="min-w-0">
            <h2 id="onboarding-title" class="text-sm font-bold text-[var(--text-primary)]">
              {step === 2 ? t("onboarding.ready") : t("onboarding.welcome")}
            </h2>
            <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
              {step === 0
                ? t("onboarding.providerStep")
                : step === 1
                  ? t("onboarding.agentStep")
                  : t("onboarding.readyStep")}
            </p>
          </div>
          <button
            type="button"
            class="ml-auto size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center cursor-pointer"
            aria-label={t("onboarding.close")}
            onclick={onSkip}
          >
            <X class="size-4" />
          </button>
        </div>
      </div>

      {#if step === 0}
        <div class="px-6 pb-4 space-y-2.5">
          <p class="text-[12px] text-[var(--text-secondary)] leading-relaxed">
            {localAvailable
              ? t("onboarding.introLocal")
              : t("onboarding.introCloud")}
          </p>
          {#if localAvailable && preferredLocal}
            <div class="rounded-xl border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-3 py-2.5 space-y-1.5">
              <p class="text-[12px] font-semibold text-[var(--brand-text)]">{t("onboarding.localDetected")}</p>
              <p class="text-[11px] text-[var(--text-secondary)]">
                {localModels.length === 1
                  ? t("onboarding.localModelOne")
                  : t("onboarding.localModels", { count: localModels.length })}
              </p>
              <button
                type="button"
                class="h-8 px-3 rounded-lg bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] text-[11px] font-semibold cursor-pointer"
                onclick={useLocalOllama}
              >
                {t("onboarding.useLocal", { model: preferredLocal.name || preferredLocal.id })}
              </button>
            </div>
          {/if}
          <div class="flex items-start gap-3">
            <span class="size-5 rounded-full bg-[var(--surface-3)] text-[11px] font-bold text-[var(--text-tertiary)] flex items-center justify-center shrink-0 mt-0.5">1</span>
            <div class="min-w-0">
              <p class="text-[12px] font-medium text-[var(--text-primary)]">{t("onboarding.openSettings")}</p>
              <p class="text-[11px] text-[var(--text-muted)]">{t("onboarding.settingsHint")}</p>
            </div>
          </div>
          <div class="flex items-start gap-3">
            <span class="size-5 rounded-full bg-[var(--surface-3)] text-[11px] font-bold text-[var(--text-tertiary)] flex items-center justify-center shrink-0 mt-0.5">2</span>
            <div class="min-w-0">
              <p class="text-[12px] font-medium text-[var(--text-primary)]">{t("onboarding.keyTitle")}</p>
              <p class="text-[11px] text-[var(--text-muted)]">{t("onboarding.keyHint")}</p>
            </div>
          </div>
        </div>
      {:else if step === 1}
        <div class="px-6 pb-4 space-y-3">
          <p class="text-[12px] text-[var(--text-secondary)] leading-relaxed">
            {t("onboarding.agentIntro")}
          </p>
          <div class="space-y-1.5">
            <label for="onboarding-agent-name" class="text-[11px] font-semibold text-[var(--text-tertiary)]">
              {t("onboarding.agentNameLabel")}
            </label>
            <input
              id="onboarding-agent-name"
              bind:value={agentName}
              class="w-full h-9 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] px-3 text-xs text-[var(--text-primary)] focus:outline-none focus:border-[var(--brand)]/60"
              placeholder={t("onboarding.agentNamePlaceholder")}
            />
          </div>
          <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2.5">
            <div class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)]">
              <Bot class="size-3.5 text-[var(--brand-text)] shrink-0" />
              <span class="font-semibold text-[var(--text-secondary)]">{t("onboarding.modelLabel")}</span>
              <span class="ml-auto font-mono truncate">{provider}/{model}</span>
            </div>
            <p class="text-[11px] text-[var(--text-muted)] mt-1 leading-relaxed">
              {t("onboarding.modelHint")}
            </p>
          </div>
          {#if error}
            <p class="text-[11px] text-danger leading-snug flex items-start gap-1.5">
              <AlertTriangle class="size-3 mt-0.5 shrink-0" />
              <span>{error}</span>
            </p>
          {/if}
        </div>
      {:else}
        <div class="px-6 pb-4 space-y-2">
          <p class="text-[12px] text-[var(--text-secondary)] leading-relaxed">
            {t("onboarding.readyBody", { name: existingAgent?.name || "The Raven" })}
          </p>
          <p class="text-[11px] text-[var(--text-muted)]">
            {t("onboarding.readyHint")}
          </p>
        </div>
      {/if}

      <!-- Actions -->
      <div class="px-6 pb-6 pt-2 flex items-center justify-between gap-3 border-t border-[var(--hairline)]">
        {#if step === 0}
          <button
            type="button"
            class="text-[11px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
            onclick={onSkip}
          >
            {t("onboarding.skip")}
          </button>
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="h-9 px-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer"
              onclick={check}
              disabled={checking}
            >
              {#if checking}<Loader2 class="size-3.5 animate-spin" />{:else}{t("onboarding.connected")}{/if}
            </button>
            <button
              type="button"
              class="h-9 px-4 rounded-xl bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer"
              onclick={onOpenSettings}
            >
              {t("onboarding.openSettings")}
              <ChevronRight class="size-3.5" />
            </button>
          </div>
        {:else if step === 1}
          <button
            type="button"
            class="text-[11px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
            onclick={onSkip}
          >
            {t("onboarding.skip")}
          </button>
          <button
            type="button"
            class="h-9 px-4 rounded-xl bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer disabled:opacity-50"
            onclick={createAgent}
            disabled={creating}
          >
            {#if creating}<Loader2 class="size-3.5 animate-spin" />{:else}<Sparkles class="size-3.5" />{/if}
            {creating ? t("onboarding.creating") : t("onboarding.create")}
          </button>
        {:else}
          <span></span>
          <button
            type="button"
            class="h-9 px-4 rounded-xl bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer"
            onclick={onSkip}
          >
            {t("onboarding.start")}
            <ChevronRight class="size-3.5" />
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
