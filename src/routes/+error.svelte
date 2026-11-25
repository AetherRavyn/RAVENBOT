<script lang="ts">
  import { page } from "$app/stores";
  import { AlertTriangle, RotateCw } from "@lucide/svelte";
</script>

<svelte:head>
  <title>Something went wrong — RAVENBOT</title>
</svelte:head>

<div class="flex min-h-screen items-center justify-center bg-[var(--surface-0)] p-6 text-[var(--text-primary)]">
  <div class="w-full max-w-lg rounded-2xl border border-[var(--danger-border)] bg-[var(--surface-1)] p-6 text-center shadow-2xl">
    <div class="mx-auto flex size-12 items-center justify-center rounded-2xl border border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger-text)]">
      <AlertTriangle class="size-6" />
    </div>
    <h1 class="mt-4 text-lg font-bold tracking-tight">Something went wrong</h1>
    <p class="mt-1 text-sm text-[var(--text-tertiary)]">
      RAVENBOT hit an unexpected error while rendering this view. Your conversations
      and files are still on disk.
    </p>
    <p class="mt-3 break-words rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2 text-left font-mono text-[11px] text-[var(--text-secondary)]">
      {$page.status}: {$page.error?.message ?? "The application could not render this screen."}
    </p>
    <button
      type="button"
      class="mt-4 inline-flex h-9 items-center gap-2 rounded-lg bg-[var(--brand)] px-4 text-xs font-semibold text-[var(--text-on-light)] hover:bg-[var(--brand-hover)] cursor-pointer"
      onclick={() => window.location.reload()}
    >
      <RotateCw class="size-3.5" />
      Reload RAVENBOT
    </button>
  </div>
</div>
