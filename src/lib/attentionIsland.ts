/**
 * The attention island's projection of the summary.
 *
 * The island is a glance, not a second inbox. These helpers keep that glance
 * honest: the newest item decides the face, the agent's stored identity decides
 * what the face looks like, and the dot says whether the item is merely waiting
 * or actually risky. Everything here is pure so the mapping can be tested
 * without a window or a Tauri host.
 */
import type { AvatarMood } from "$lib/avatar";

export type IslandItemKind = "approval" | "question";

export interface IslandAttentionItem {
  kind: IslandItemKind;
  label: string;
  detail: string;
  thread_id?: string | null;
  risk?: string | null;
  /** `pending`, `running`, or `failed`. Older hosts omit it. */
  state?: string | null;
  bot_id?: string | null;
  bot_name?: string | null;
  avatar_url?: string | null;
  avatar_style?: string | null;
  created_at?: string | null;
}

export interface IslandAttentionSummary {
  approvals: number;
  questions: number;
  active_runs: number;
  items: IslandAttentionItem[];
}

/** The newest item, or `null` when the island has nothing to show. */
export function latestIslandItem(
  items: readonly IslandAttentionItem[] | null | undefined,
): IslandAttentionItem | null {
  if (!items?.length) return null;
  return items.reduce((newest, item) =>
    createdAt(item) > createdAt(newest) ? item : newest,
  );
}

/** A timestamp sort key that treats a missing date as oldest. */
function createdAt(item: IslandAttentionItem): number {
  const value = Date.parse(item.created_at ?? "");
  return Number.isFinite(value) ? value : 0;
}

/**
 * The agent's name, trimmed.
 *
 * A queue or alert item has no agent, and a host older than the avatar fields
 * has no name either. Returning an empty string lets the caller keep the
 * text-only layout rather than inventing an identity.
 */
export function islandAgentName(item: IslandAttentionItem | null): string {
  return (item?.bot_name ?? "").trim();
}

/**
 * The mood the island avatar wears.
 *
 * A pending gate is `waiting`, which is the same attentive face the roster
 * uses. A failed queue row is `failed`; a running one is `working`. Anything
 * unknown stays `waiting` because the item reached the island precisely
 * because it wants attention.
 */
export function islandMood(item: IslandAttentionItem | null): AvatarMood {
  switch (item?.state) {
    case "failed":
      return "failed";
    case "running":
      return "working";
    default:
      return "waiting";
  }
}

export type IslandDotTone = "danger" | "warning" | "info" | "success";

/**
 * The dot's tone, in priority order: danger beats waiting beats activity.
 *
 * The dot is a second signal beside the avatar, so it must agree with the
 * text: a high-risk approval is danger even if it is the only thing waiting.
 * `null` is the all-clear state.
 */
export function islandDotTone(
  item: IslandAttentionItem | null,
  waiting: number,
): IslandDotTone {
  if (!item) return waiting > 0 ? "warning" : "success";
  if (item.state === "failed") return "danger";
  if (item.risk === "high" || item.risk === "critical") return "danger";
  if (waiting > 0) return "warning";
  if (item.state === "running") return "info";
  return "success";
}
