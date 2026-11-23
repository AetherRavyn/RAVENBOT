// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  deliverAttentionEmail,
  deliverAttentionWebhook,
  notifyAttentionItem,
} from "$lib/notifications";
import { attention } from "./attention.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("$lib/notifications", () => ({
  notifyAttentionItem: vi.fn(),
  deliverAttentionWebhook: vi.fn(),
  deliverAttentionEmail: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);
const notifyMock = vi.mocked(notifyAttentionItem);
const webhookMock = vi.mocked(deliverAttentionWebhook);
const emailMock = vi.mocked(deliverAttentionEmail);

const approval = {
  id: "approval-1",
  kind: "approval" as const,
  bot_id: "bot-1",
  bot_name: "Scout",
  thread_id: "thread-1",
  office_id: "office-1",
  office_name: "Acme",
  title: "Run a command",
  detail: "shell_exec",
  risk: "high",
  state: "pending",
  question_kind: null,
  created_at: "2026-11-09T11:59:00.000Z",
};

beforeEach(() => {
  attention.stop();
  attention.items = [];
  invokeMock.mockReset();
  notifyMock.mockReset();
  webhookMock.mockReset();
  emailMock.mockReset();
});
afterEach(() => {
  attention.stop();
});

describe("attention notifications", () => {
  it("does not notify existing items on first load and deduplicates refreshes", async () => {
    invokeMock.mockResolvedValueOnce([]);
    await attention.refresh();

    invokeMock.mockResolvedValue([approval]);
    await attention.refresh();
    expect(notifyMock).toHaveBeenCalledTimes(1);
    expect(notifyMock).toHaveBeenCalledWith(approval);
    expect(webhookMock).toHaveBeenCalledWith(approval);
    expect(emailMock).toHaveBeenCalledWith(approval);

    await attention.refresh();
    expect(notifyMock).toHaveBeenCalledTimes(1);
  });

  it("notifies on a pending-to-failed queue transition but not pending queue", async () => {
    const pending = {
      ...approval,
      id: "queue-1",
      kind: "queue" as const,
      state: "pending",
    };
    invokeMock.mockResolvedValue([pending]);
    await attention.refresh();
    await attention.refresh();
    expect(notifyMock).not.toHaveBeenCalled();

    const failed = { ...pending, state: "failed" };
    invokeMock.mockResolvedValue([failed]);
    await attention.refresh();
    expect(notifyMock).toHaveBeenCalledWith(failed);
  });
});
