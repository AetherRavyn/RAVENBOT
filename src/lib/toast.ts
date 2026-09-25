// Tiny global toast store — no dependency, no gradients, theme-aware.
//
//   import { notify } from "$lib/toast";
//   notify("Connector assigned", "success");
//
// Render `<Toaster />` once in the layout.

export type ToastKind = "success" | "error" | "info";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

type Listener = (toasts: Toast[]) => void;

let toasts: Toast[] = [];
const listeners = new Set<Listener>();
let nextId = 1;

function emit() {
  const snapshot = toasts;
  for (const listener of listeners) listener(snapshot);
}

/** Show a transient toast. Returns its id. */
export function notify(message: string, kind: ToastKind = "info", ttlMs = 4000): number {
  const id = nextId++;
  toasts = [...toasts, { id, kind, message }];
  emit();
  if (ttlMs > 0) {
    setTimeout(() => dismissToast(id), ttlMs);
  }
  return id;
}

export function dismissToast(id: number) {
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

export function subscribeToasts(listener: Listener): () => void {
  listeners.add(listener);
  listener(toasts);
  return () => listeners.delete(listener);
}
