<script lang="ts">
  /**
   * First-run onboarding overlay.
   *
   * Shown once, on the first launch where no provider key is configured, so a
   * new user is never dropped into an empty chat with no idea why nothing
   * works. Dismissed permanently once a key is saved (or the user explicitly
   * skips), and never shown again.
   *
   * The point is not to teach the whole app — it is to answer the one question
   * a new user has: "why won't it talk to me?" The answer is almost always
   * "no API key configured", and this is where that gets said.
   */
  import { t } from "$lib/i18n";
  import { Key, ChevronRight, X } from "@lucide/svelte";

  interface Props {
    open: boolean;
    onOpenSettings: () => void;
    onSkip: () => void;
  }

  let { open, onOpenSettings, onSkip }: Props = $props();
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
            <Key class="size-5 text-[var(--brand-text)]" />
          </div>
          <div>
            <h2 id="onboarding-title" class="text-sm font-bold text-[var(--text-primary)]">Welcome to RAVENBOT</h2>
            <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">One step before your first conversation</p>
          </div>
        </div>
        <p class="text-[12px] text-[var(--text-secondary)] leading-relaxed">
          RAVENBOT runs on your machine, but it needs an API key to think. Pick any provider
          you have an account with — OpenRouter, Anthropic, OpenAI, a local LM Studio, or any
          OpenAI-compatible endpoint — and paste the key. Free providers like Ollama work
          with no key at all.
        </p>
      </div>

      <!-- Steps -->
      <div class="px-6 pb-4 space-y-2.5">
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
            <p class="text-[11px] text-[var(--text-muted)]">Paste your key under <span class="font-mono text-[var(--text-secondary)]">API Keys</span>, or add a custom OpenAI-compatible endpoint (LM Studio, OpenRouter, …).</p>
          </div>
        </div>
        <div class="flex items-start gap-3">
          <span class="size-5 rounded-full bg-[var(--surface-3)] text-[11px] font-bold text-[var(--text-tertiary)] flex items-center justify-center shrink-0 mt-0.5">3</span>
          <div class="min-w-0">
            <p class="text-[12px] font-medium text-[var(--text-primary)]">Start chatting</p>
            <p class="text-[11px] text-[var(--text-muted)]">Your first message will use the model you configured. You can change it per conversation.</p>
          </div>
        </div>
      </div>

      <!-- Actions -->
      <div class="px-6 pb-6 pt-2 flex items-center justify-between gap-3">
        <button
          type="button"
          class="text-[11px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          onclick={onSkip}
        >
          Skip for now
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
    </div>
  </div>
{/if}
