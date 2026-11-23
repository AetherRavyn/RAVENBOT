// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AttentionInbox from "./AttentionInbox.svelte";
import { attention, type AttentionItem } from "$lib/attention.svelte";
import { workspace } from "$lib/workspace.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const items: AttentionItem[] = [
  {
    id: "approval-1",
    kind: "approval",
    bot_id: "bot-1",
    bot_name: "Scout",
    thread_id: "thread-1",
    office_id: "office-1",
    office_name: "Acme",
    title: "Run a command",
    detail: "shell_exec — cargo test",
    risk: "high",
    state: "pending",
    question_kind: null,
    created_at: "2026-11-09T11:59:00.000Z",
  },
  {
    id: "question-1",
    kind: "question",
    bot_id: "bot-1",
    bot_name: "Scout",
    thread_id: "thread-1",
    office_id: null,
    office_name: null,
    title: "Which database?",
    detail: "Which database should I use?",
    risk: null,
    state: "pending",
    question_kind: "question",
    created_at: "2026-11-09T11:58:00.000Z",
  },
  {
    id: "queue-1",
    kind: "queue",
    bot_id: null,
    bot_name: "",
    thread_id: null,
    office_id: "office-1",
    office_name: "Acme",
    title: "Queued office message",
    detail: "ship it",
    risk: null,
    state: "pending",
    question_kind: null,
    created_at: "2026-11-09T11:57:00.000Z",
  },
  {
    id: "alert-1",
    kind: "alert",
    bot_id: null,
    bot_name: "",
    thread_id: null,
    office_id: "office-1",
    office_name: "Acme",
    title: "Office budget warning",
    detail: "$1.8500 spent of $2.0000 (93%)",
    risk: null,
    state: "warning",
    question_kind: null,
    alert_kind: "budget",
    percent: 92.5,
    budget: 2,
    cost: 1.85,
    created_at: "2026-11-09T11:56:00.000Z",
  },
];

beforeEach(() => {
  cleanup();
  setLocale("en");
  attention.items = [];
  attention.loading = false;
  attention.error = null;
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(items);
  workspace.chatrooms = [{ id: "office-1", name: "Acme" }];
  workspace.bots = [{ id: "bot-1", name: "Scout" }];
});

afterEach(() => {
  cleanup();
  attention.items = [];
  workspace.chatrooms = [];
  workspace.bots = [];
  workspace.selectedRoomId = null;
  workspace.selectedBotId = null;
});

describe("AttentionInbox", () => {
  it("lists every source and filters it", async () => {
    const { getByText, queryByText } = render(AttentionInbox);
    await waitFor(() => expect(getByText("Run a command")).toBeTruthy());
    expect(getByText("Which database?")).toBeTruthy();
    expect(getByText("Queued office message")).toBeTruthy();
    expect(getByText("Office budget warning")).toBeTruthy();

    await fireEvent.click(getByText("Approvals"));
    expect(getByText("Run a command")).toBeTruthy();
    expect(queryByText("Which database?")).toBeNull();
    expect(queryByText("Queued office message")).toBeNull();
    expect(queryByText("Office budget warning")).toBeNull();
  });

  it("shows budget alerts with spend, threshold, and office hand-off", async () => {
    const { getByText, queryByText } = render(AttentionInbox);
    await waitFor(() => expect(getByText("Office budget warning")).toBeTruthy());
    expect(getByText("$1.8500 spent of $2.0000 (93%)")).toBeTruthy();
    expect(getByText("Warning")).toBeTruthy();

    await fireEvent.click(getByText("Alerts"));
    expect(getByText("Office budget warning")).toBeTruthy();
    expect(queryByText("Run a command")).toBeNull();

    await fireEvent.click(getByText("Office budget warning"));
    expect(workspace.selectedRoomId).toBe("office-1");
  });

  it("opens a queued item in its office", async () => {
    const { getByText } = render(AttentionInbox);
    await waitFor(() => expect(getByText("Queued office message")).toBeTruthy());
    await fireEvent.click(getByText("Queued office message"));
    expect(workspace.selectedRoomId).toBe("office-1");
  });
});
