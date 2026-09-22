<script lang="ts">
  import { onMount } from "svelte";
  import { dismissToast, subscribeToasts, type Toast } from "$lib/toast";
  import { CheckCircle2, AlertTriangle, Info, X } from "@lucide/svelte";

  let toasts = $state<Toast[]>([]);

  onMount(() => subscribeToasts((next) => (toasts = next)));

  function styles(kind: Toast["kind"]) {
    switch (kind) {
      case "success":
        return { border: "border-success/40", bg: "bg-success/10", text: "text-success", icon: CheckCircle2 };
      case "error":
        return { border: "border-danger/40", bg: "bg-danger/10", text: "text-danger", icon: AlertTriangle };
      default:
        return { border: "border-[var(--hairline-strong)]", bg: "bg-[var(--surface-1)]", text: "text-[var(--text-secondary)]", icon: Info };
    }
  }
</script>

<div class="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 w-[22rem] max-w-[calc(100vw-2rem)] pointer-events-none">
  {#each toasts as toast (toast.id)}
    {@const s = styles(toast.kind)}
    <div
      class="pointer-events-auto flex items-start gap-2.5 rounded-xl border {s.border} {s.bg}  px-3.5 py-2.5 shadow-2xl animate-scale-in"
      role="status"
    >
      <s.icon class="size-4 shrink-0 mt-0.5 {s.text}" />
      <p class="flex-1 text-xs leading-relaxed {s.text}">{toast.message}</p>
      <button
        type="button"
        class="size-5 rounded-md flex items-center justify-center text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] cursor-pointer shrink-0"
        onclick={() => dismissToast(toast.id)}
        aria-label="Dismiss"
      >
        <X class="size-3" />
      </button>
    </div>
  {/each}
</div>
