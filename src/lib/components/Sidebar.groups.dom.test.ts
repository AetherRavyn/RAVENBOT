// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import Sidebar from "./Sidebar.svelte";
import { attention, type AttentionItem } from "$lib/attention.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const bots = [
  { id: "bot-1", name: "Alpha", status: "idle" },
  { id: "bot-2", name: "Bravo", status: "idle" },
  { id: "bot-3", name: "Charlie", status: "idle" },
  { id: "bot-4", name: "Delta", status: "idle" },
];

function attentionItem(): AttentionItem {
  return {
    id: "q1",
    kind: "question",
    bot_id: "bot-2",
    bot_name: "Bravo",
    thread_id: "thread-2",
    office_id: null,
    office_name: null,
    title: "Which branch?",
    detail: "Choose a branch",
    risk: null,
    state: "pending",
    question_kind: null,
    created_at: "2026-11-28T10:00:00Z",
  };
}

function renderSidebar() {
  return render(Sidebar, {
    props: {
      bots,
      selectedBotId: null,
      onSelectBot: vi.fn(),
      onBotCreated: vi.fn(),
      onBotUpdated: vi.fn(),
      onBotDeleted: vi.fn(),
      openSettings: vi.fn(),
    },
  });
}

describe("Sidebar fleet groups", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    window.localStorage.clear();
    attention.items = [attentionItem()];
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_bot_contacts") {
        return [
          { bot_id: "bot-1", pinned: true, hidden: false },
          { bot_id: "bot-3", pinned: false, hidden: true },
        ];
      }
      if (command === "get_unread_counts") return {};
      return null;
    });
  });

  afterEach(() => {
    cleanup();
    attention.items = [];
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it("splits pinned, waiting and ordinary agents into visible groups", async () => {
    const { container } = renderSidebar();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /^Pinned · 1/ })).toBeTruthy(),
    );

    expect(container.textContent).toContain("Alpha");
    expect(screen.getByRole("button", { name: /^Waiting on you · 1/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Agents · 1/ })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Hidden/ })).toBeNull();
    expect(container.textContent).not.toContain("Charlie");
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("shows hidden agents only when the eye toggle is on", async () => {
    const { container } = renderSidebar();
    await waitFor(() => expect(container.textContent).toContain("Alpha"));
    await fireEvent.click(screen.getByRole("button", { name: "Show hidden agents" }));
    await waitFor(() => expect(container.textContent).toContain("Charlie"));
    expect(screen.getByRole("button", { name: /^Hidden · 1/ })).toBeTruthy();
  });

  it("collapses a group, persists it, and expands it again", async () => {
    const { container } = renderSidebar();
    const pinned = await waitFor(() =>
      screen.getByRole("button", { name: /^Pinned · 1/ }),
    );
    await fireEvent.click(pinned);
    expect(container.textContent).not.toContain("Alpha");
    expect(JSON.parse(window.localStorage.getItem("raven.sidebar.collapsedGroups")!)).toMatchObject({
      pinned: true,
    });

    await fireEvent.click(pinned);
    expect(container.textContent).toContain("Alpha");
  });

  it("expands a collapsed group automatically while searching", async () => {
    const { container } = renderSidebar();
    const pinned = await waitFor(() =>
      screen.getByRole("button", { name: /^Pinned · 1/ }),
    );
    await fireEvent.click(pinned);
    expect(container.textContent).not.toContain("Alpha");

    await fireEvent.input(screen.getByPlaceholderText("Filter fleet..."), {
      target: { value: "Alpha" },
    });
    await waitFor(() => expect(container.textContent).toContain("Alpha"));
    expect(container.textContent).not.toContain("Bravo");
  });

  it("selects an agent from a group", async () => {
    const onSelectBot = vi.fn();
    const { container } = render(Sidebar, {
      props: {
        bots,
        selectedBotId: null,
        onSelectBot,
        onBotCreated: vi.fn(),
        onBotUpdated: vi.fn(),
        onBotDeleted: vi.fn(),
        openSettings: vi.fn(),
      },
    });
    await waitFor(() => expect(container.textContent).toContain("Alpha"));
    await fireEvent.click(screen.getByRole("button", { name: /Alpha/ }));
    expect(onSelectBot).toHaveBeenCalledWith("bot-1");
  });
});
