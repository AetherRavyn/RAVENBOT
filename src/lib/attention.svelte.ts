/**
 * Cross-fleet attention.
 *
 * The inbox is a projection of four existing sources: pending approvals,
 * pending questions, queued/failed office messages, and office budget alerts.
 * It is refreshed from the stream rather than polled on every render, and it
 * keeps the last known list when a refresh fails so a transient IPC error does
 * not empty the pane.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  deliverAttentionEmail,
  deliverAttentionWebhook,
  notifyAttentionItem,
} from "$lib/notifications";

export type AttentionKind = "approval" | "question" | "queue" | "alert";
export type AttentionFilter = "all" | AttentionKind;

export interface AttentionItem {
  id: string;
  kind: AttentionKind;
  bot_id: string | null;
  bot_name: string;
  thread_id: string | null;
  office_id: string | null;
  office_name: string | null;
  title: string;
  detail: string;
  risk: string | null;
  state: "pending" | "running" | "failed" | string;
  question_kind: string | null;
  /** `budget` for an office health alert. */
  alert_kind?: string | null;
  percent?: number | null;
  budget?: number | null;
  cost?: number | null;
  created_at: string;
}

/** Broadcast after a local action that changes the inbox before the stream catches up. */
export const ATTENTION_CHANGED_EVENT = "ravenbot:attention-changed";

export function announceAttentionChanged(): void {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(ATTENTION_CHANGED_EVENT));
  }
}

class AttentionStore {
  items = $state<AttentionItem[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  private unlisten: UnlistenFn | null = null;
  private poll: ReturnType<typeof setInterval> | null = null;
  private debounce: ReturnType<typeof setTimeout> | null = null;
  private changed: (() => void) | null = null;
  private started = false;
  private knownKeys = new Set<string>();
  private primed = false;

  get count(): number {
    return this.items.length;
  }

  get needsCount(): number {
    return this.items.filter((item) => item.kind !== "queue").length;
  }

  get failedCount(): number {
    return this.items.filter((item) => item.state === "failed").length;
  }

  async refresh(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const rows = await invoke<AttentionItem[]>("list_attention_items");
      const next = Array.isArray(rows) ? rows : [];
      const keys = new Set(next.map(attentionKey));
      if (this.primed) {
        for (const item of next) {
          if (
            !this.knownKeys.has(attentionKey(item)) &&
            shouldNotifyAttention(item)
          ) {
            void notifyAttentionItem(item);
            void deliverAttentionWebhook(item);
            void deliverAttentionEmail(item);
          }
        }
      }
      this.knownKeys = keys;
      this.primed = true;
      this.items = next;
    } catch (error) {
      this.error = String(error);
    } finally {
      this.loading = false;
    }
  }

  /** Refresh shortly after a burst of stream events, not once per event. */
  schedule(): void {
    if (this.debounce) clearTimeout(this.debounce);
    this.debounce = setTimeout(() => {
      this.debounce = null;
      void this.refresh();
    }, 400);
  }

  async start(): Promise<void> {
    if (this.started) return;
    this.started = true;
    this.changed = () => this.schedule();
    window.addEventListener(ATTENTION_CHANGED_EVENT, this.changed);
    this.poll = setInterval(() => void this.refresh(), 30_000);
    await this.refresh();
    if (!this.started) return;
    try {
      const unlisten = await listen<any>("agent-stream", (event) => {
        const kind = String(event.payload?.kind || "");
        if (
          kind === "approval_requested" ||
          kind === "approval_decided" ||
          kind === "question_asked" ||
          kind === "question_answered" ||
          kind === "done" ||
          kind === "error" ||
          kind === "error_card"
        ) {
          this.schedule();
        }
      });
      // The view can unmount while the listener is being registered.
      if (!this.started) {
        unlisten();
        return;
      }
      this.unlisten = unlisten;
    } catch {
      /* the inbox still has its manual refresh and poll */
    }
  }

  stop(): void {
    this.started = false;
    this.unlisten?.();
    this.unlisten = null;
    if (this.poll) clearInterval(this.poll);
    this.poll = null;
    if (this.debounce) clearTimeout(this.debounce);
    this.debounce = null;
    if (this.changed) window.removeEventListener(ATTENTION_CHANGED_EVENT, this.changed);
    this.changed = null;
    this.knownKeys.clear();
    this.primed = false;
  }
}

export const attention = new AttentionStore();

function attentionKey(item: AttentionItem): string {
  return `${item.kind}:${item.id}:${item.state}`;
}

function shouldNotifyAttention(item: AttentionItem): boolean {
  return (
    item.kind === "approval" ||
    item.kind === "question" ||
    item.kind === "alert" ||
    (item.kind === "queue" && item.state === "failed")
  );
}

/** A compact relative time for the inbox; uses the active locale's rules. */
export function relativeAttentionTime(iso: string, locale: string, now = Date.now()): string {
  const then = Date.parse(iso);
  if (!Number.isFinite(then)) return "";
  const seconds = Math.round((then - now) / 1000);
  const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  const absolute = Math.abs(seconds);
  if (absolute < 60) return formatter.format(seconds, "second");
  const minutes = Math.round(seconds / 60);
  if (Math.abs(minutes) < 60) return formatter.format(minutes, "minute");
  const hours = Math.round(minutes / 60);
  if (Math.abs(hours) < 24) return formatter.format(hours, "hour");
  const days = Math.round(hours / 24);
  return formatter.format(days, "day");
}

/**
 * Bots with a gate a person has to answer.
 *
 * Office queue rows and budget alerts belong in the inbox, but they do not make
 * an agent's card "waiting on me": a queued message is work the office has not
 * started, and a budget alert is a policy warning. Approvals and questions are
 * the two kinds that actually park a run on a human.
 */
export function awaitingBots(items: AttentionItem[]): Set<string> {
  const ids = new Set<string>();
  for (const item of items) {
    if (item.bot_id && (item.kind === "approval" || item.kind === "question")) {
      ids.add(item.bot_id);
    }
  }
  return ids;
}

export function awaitingCount(items: AttentionItem[]): number {
  return items.filter(
    (item) => item.kind === "approval" || item.kind === "question",
  ).length;
}
