// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  deliverAttentionEmail,
  deliverAttentionWebhook,
  notificationPreference,
  notifyAttentionItem,
  setNotificationPreference,
} from "./notifications";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  setLocale("en");
  localStorage.clear();
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(null);
  vi.spyOn(document, "hasFocus").mockReturnValue(false);
});
afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("attention notifications", () => {
  it("defaults off and only sends when enabled", async () => {
    expect(notificationPreference()).toBe(false);
    await notifyAttentionItem({
      kind: "approval",
      state: "pending",
      office_name: "Acme",
    });
    expect(invokeMock).not.toHaveBeenCalled();

    expect(setNotificationPreference(true)).toBe(true);
    expect(notificationPreference()).toBe(true);
    await notifyAttentionItem({
      kind: "approval",
      state: "pending",
      office_name: "Acme",
    });
    expect(invokeMock).toHaveBeenCalledWith("send_attention_notification", {
      title: "Approval needed",
      body: "Acme is waiting for approval.",
    });
  });

  it("does not notify while the app is focused", async () => {
    setNotificationPreference(true);
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    await notifyAttentionItem({
      kind: "question",
      state: "pending",
      bot_name: "Scout",
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("posts a redacted webhook event for actionable items", async () => {
    await deliverAttentionWebhook({
      kind: "approval",
      state: "pending",
      office_name: "Acme",
      bot_name: "Scout",
    });
    expect(invokeMock).toHaveBeenCalledWith("deliver_attention_webhook", {
      kind: "approval",
      stateName: "pending",
      officeName: "Acme",
      botName: "Scout",
    });

    invokeMock.mockClear();
    await deliverAttentionWebhook({
      kind: "queue",
      state: "pending",
      office_name: "Acme",
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("posts a redacted email event for actionable items", async () => {
    await deliverAttentionEmail({
      kind: "approval",
      state: "pending",
      office_name: "Acme",
      bot_name: "Scout",
    });
    expect(invokeMock).toHaveBeenCalledWith("deliver_attention_email", {
      kind: "approval",
      stateName: "pending",
      officeName: "Acme",
      botName: "Scout",
    });

    invokeMock.mockClear();
    await deliverAttentionEmail({
      kind: "queue",
      state: "pending",
      office_name: "Acme",
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("keeps notification bodies generic and category-specific", async () => {
    setNotificationPreference(true);
    await notifyAttentionItem({
      kind: "queue",
      state: "pending",
      office_name: "Acme",
    });
    expect(invokeMock).not.toHaveBeenCalled();

    await notifyAttentionItem({
      kind: "queue",
      state: "failed",
      office_name: "Acme",
    });
    await notifyAttentionItem({
      kind: "alert",
      state: "critical",
      office_name: "Acme",
      percent: 104.4,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(
      1,
      "send_attention_notification",
      {
        title: "Office queue failed",
        body: "Acme has a failed queued item.",
      },
    );
    expect(invokeMock).toHaveBeenNthCalledWith(
      2,
      "send_attention_notification",
      {
        title: "Budget alert",
        body: "Acme is at 104% of its budget.",
      },
    );
  });
});
