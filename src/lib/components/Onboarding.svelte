<script lang="ts">
  /**
   * First-run setup.
   *
   * Two things have to be true before RAVENBOT can design an office:
   *   1. a provider key exists, so the app can think;
   *   2. an office architect exists, so the app knows which agent designs
   *      offices and on which model.
   *
   * The second step used to be implicit — the app guessed a model and designed
   * offices with no identity behind it. Asking once produces a real bot the
   * user can rename, re-model, or replace later, and gives every office a named
   * designer instead of an anonymous completion.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { Key, ChevronRight, X, Sparkles, Bot, Loader2, CheckCircle2, AlertTriangle } from "@lucide/svelte";

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
  let agentName = $state("Chief of Staff");
  let provider = $state("");
  let model = $state("");
  let hasKeys = $state(false);
  let existingAgent = $state<any | null>(null);

  async function check() {
    checking = true;
    error = null;
    try {
      const [keys, agent] = await Promise.all([
        invoke<string[]>("get_configured_providers"),
        invoke<any>("get_default_agent").catch(() => null),
      ]);
      hasKeys = Array.isArray(keys) && keys.length > 0;
      existingAgent = agent ?? null;
      if (existingAgent) {
        step = 2;
        return;
      }
      if (hasKeys) {
        const def: any = await invoke("get_default_model").catch(() => null);
        provider = def?.provider || "opencode-go";
        model = def?.model || "deepseek-v4.1-flash";
        step = 1;
      } else {
        step = 0;
      }
    } catch (e) {
      error = String(e);
      step = 0;
    } finally {
      checking = false;
    }
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
        name: agentName.trim() || "Chief of Staff",
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
              {step === 2 ? "You're ready" : "Welcome to RAVENBOT"}
            </h2>
            <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
              {step === 0
                ? "Connect a provider so the app can think"
                : step === 1
                  ? "Choose the agent that designs your offices"
                  : "Your office architect is ready"}
            </p>
          </div>
          <button
            type="button"
            class="ml-auto size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center cursor-pointer"
            aria-label="Close"
            onclick={onSkip}
          >
            <X class="size-4" />
          </button>
        </div>
      </div>

      {#if step === 0}
        <div class="px-6 pb-4 space-y-2.5">
          <p class="text-[12px] text-[var(--text-secondary)] leading-relaxed">
            RAVENBOT runs on your machine, but it needs an API key to think. Pick any provider
            you have an account with — OpenRouter, Anthropic, OpenAI, OpenCode Go, a local
            LM Studio, or any OpenAI-compatible endpoint — and paste the key.
          </p>
          <div class="flex items-start gap-3">
            <span class="size-5 rounded-full bg-[var(--surface-3)] text-[11px] font-bold text-[var(--text-tertiary)] flex items-center justify-center shrink-0 mt-0.5">1</span>
            <div class="min-w-0">
              <p class="text-[12px] font-medium text-[var(--text-primary)]">Open Settings</p>
              <p class="text-[11px] text-[var(--text-muted)]">Press ⌘, or click the gear in the sidebar.</p>
            </div>
          </div>
          <div class="flex items-start gap-3">
            <span class="size-5 rounded-full bg-[var(--surface-3)] text-[11px] font-bold text-[var(--text-tertiary)] flex items-center justify-center shrink-0 mt-0.5">2</span>
            <div class="min-w-0">
              <p class="text-[12px] font-medium text-[var(--text-primary)]">Add a provider key</p>
              <p class="text-[11px] text-[var(--text-muted)]">Paste your key under <span class="font-mono text-[var(--text-secondary)]">API Keys</span>, or add a custom OpenAI-compatible endpoint.</p>
            </div>
          </div>
        </div>
      {:else if step === 1}
        <div class="px-6 pb-4 space-y-3">
          <p class="text-[12px] text-[var(--text-secondary)] leading-relaxed">
            Your office architect reads the tools, connectors, skills, and plugins installed
            here and turns a name and a mission into a charter and a team. It designs offices;
            it does not do their work.
          </p>
          <div class="space-y-1.5">
            <label for="onboarding-agent-name" class="text-[11px] font-semibold text-[var(--text-tertiary)]">
              Agent name
            </label>
            <input
              id="onboarding-agent-name"
              bind:value={agentName}
              class="w-full h-9 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] px-3 text-xs text-[var(--text-primary)] focus:outline-none focus:border-[var(--brand)]/60"
              placeholder="Chief of Staff"
            />
          </div>
          <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2.5">
            <div class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)]">
              <Bot class="size-3.5 text-[var(--brand-text)] shrink-0" />
              <span class="font-semibold text-[var(--text-secondary)]">Model</span>
              <span class="ml-auto font-mono truncate">{provider}/{model}</span>
            </div>
            <p class="text-[11px] text-[var(--text-muted)] mt-1 leading-relaxed">
              Change the default model in Settings → Model Manager first if you want a different brain.
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
            <span class="font-semibold text-[var(--text-primary)]">{existingAgent?.name || "Chief of Staff"}</span>
            will design each office from its name and mission, using only the tools that exist
            on this machine.
          </p>
          <p class="text-[11px] text-[var(--text-muted)]">
            You can change it later in Settings → Model Manager.
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
            Skip for now
          </button>
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="h-9 px-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer"
              onclick={check}
              disabled={checking}
            >
              {#if checking}<Loader2 class="size-3.5 animate-spin" />{:else}I've connected it{/if}
            </button>
            <button
              type="button"
              class="h-9 px-4 rounded-xl bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer"
              onclick={onOpenSettings}
            >
              Open Settings
              <ChevronRight class="size-3.5" />
            </button>
          </div>
        {:else if step === 1}
          <button
            type="button"
            class="text-[11px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
            onclick={onSkip}
          >
            Skip for now
          </button>
          <button
            type="button"
            class="h-9 px-4 rounded-xl bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer disabled:opacity-50"
            onclick={createAgent}
            disabled={creating}
          >
            {#if creating}<Loader2 class="size-3.5 animate-spin" />{:else}<Sparkles class="size-3.5" />{/if}
            {creating ? "Creating…" : "Create default agent"}
          </button>
        {:else}
          <span></span>
          <button
            type="button"
            class="h-9 px-4 rounded-xl bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer"
            onclick={onSkip}
          >
            Start working
            <ChevronRight class="size-3.5" />
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
