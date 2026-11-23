/**
 * Opt-in desktop notifications for new attention items.
 *
 * Bodies are deliberately generic: they name the agent/office and the category
 * only. Prompt text, questions, secrets and budget amounts are never included
 * because OS notifications can appear on a lock screen.
 */
import { invoke } from "@tauri-apps/api/core";
import { t } from "$lib/i18n";

const PREFERENCE_KEY = "raven.attention.notifications";

export interface AttentionNotificationItem {
  kind: string;
  state: string;
  office_name?: string | null;
  bot_name?: string | null;
  percent?: number | null;
}

export function notificationPreference(): boolean {
  try {
    return localStorage.getItem(PREFERENCE_KEY) === "1";
  } catch {
    return false;
  }
}

export function setNotificationPreference(enabled: boolean): boolean {
  try {
    localStorage.setItem(PREFERENCE_KEY, enabled ? "1" : "0");
  } catch {
    /* storage unavailable; the setting is session-only */
  }
  return enabled;
}

export async function sendAttentionNotification(
  title: string,
  body: string,
): Promise<void> {
  try {
    await invoke("send_attention_notification", { title, body });
  } catch {
    /* desktop notification daemons can be absent; never break the inbox */
  }
}

export async function notifyAttentionItem(
  item: AttentionNotificationItem,
): Promise<void> {
  if (!notificationPreference()) return;
  if (typeof document !== "undefined" && document.hasFocus?.()) return;
  const where =
    item.office_name || item.bot_name || t("notifications.whereFallback");
  if (item.kind === "approval") {
    await sendAttentionNotification(
      t("notifications.approval"),
      t("notifications.approvalBody", { where }),
    );
  } else if (item.kind === "question") {
    await sendAttentionNotification(
      t("notifications.question"),
      t("notifications.questionBody", { where }),
    );
  } else if (item.kind === "queue" && item.state === "failed") {
    await sendAttentionNotification(
      t("notifications.queueFailed"),
      t("notifications.queueFailedBody", { where }),
    );
  } else if (item.kind === "alert") {
    await sendAttentionNotification(
      t("notifications.budgetAlert"),
      t("notifications.budgetBody", {
        where,
        percent: Math.round(item.percent ?? 0),
      }),
    );
  }
}

/** Deliver a redacted attention event to the configured webhook, if any. */
export async function deliverAttentionWebhook(
  item: AttentionNotificationItem,
): Promise<void> {
  if (
    item.kind !== "approval" &&
    item.kind !== "question" &&
    item.kind !== "alert" &&
    !(item.kind === "queue" && item.state === "failed")
  ) {
    return;
  }
  try {
    await invoke("deliver_attention_webhook", {
      kind: item.kind,
      stateName: item.state,
      officeName: item.office_name ?? null,
      botName: item.bot_name ?? null,
    });
  } catch {
    /* webhook delivery is best-effort */
  }
}

/** Deliver a redacted attention event to the configured SMTP inbox, if any. */
export async function deliverAttentionEmail(
  item: AttentionNotificationItem,
): Promise<void> {
  if (
    item.kind !== "approval" &&
    item.kind !== "question" &&
    item.kind !== "alert" &&
    !(item.kind === "queue" && item.state === "failed")
  ) {
    return;
  }
  try {
    await invoke("deliver_attention_email", {
      kind: item.kind,
      stateName: item.state,
      officeName: item.office_name ?? null,
      botName: item.bot_name ?? null,
    });
  } catch {
    /* email delivery is best-effort */
  }
}

export async function sendTestNotification(): Promise<string> {
  await invoke("send_attention_notification", {
    title: t("notifications.testTitle"),
    body: t("notifications.testBody"),
  });
  return t("notifications.testSent");
}
