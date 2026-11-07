// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import CommandPalette from "$lib/components/CommandPalette.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const hit = {
  message_id: "message-1",
  thread_id: "thread-1",
  bot_id: "bot-1",
  thread_title: "Invoice review",
  role: "user",
  snippet: "The invoice is overdue and needs approval.",
  created_at: "2026-11-05T01:00:00Z",
};

function props(overrides: Record<string, unknown> = {}) {
  return {
    open: true,
    onClose: vi.fn(),
    bots: [
      {
        id: "bot-1",
        name: "Billing Agent",
        description: "Reconciles invoices and ledger entries",
        rank: "Billing Engineer",
        config: { model_provider: "openrouter" },
      },
      { id: "bot-2", name: "Research Agent", description: "Finds sources" },
    ],
    chatrooms: [
      {
        id: "room-1",
        name: "Ledger Office",
        description: "Monthly close and reconciliation",
        office_template: "finance",
      },
    ],
    onSelectBot: vi.fn(),
    onSelectRoom: vi.fn(),
    onCreateBot: vi.fn(),
    onNewChat: vi.fn(),
    onOpenSettings: vi.fn(),
    onNavigate: vi.fn(),
    onOpenPlugins: vi.fn(),
    onOpenSkills: vi.fn(),
    onToggleBrowser: vi.fn(),
    onOpenMessage: vi.fn(),
    ...overrides,
  };
}

function searchInput(): HTMLInputElement {
  return document.querySelector('input[aria-label^="Search agents"]') as HTMLInputElement;
}

describe("CommandPalette global search", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "search_messages") return [hit];
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("shows agents, offices, and quick actions before typing", async () => {
    render(CommandPalette, { props: props() });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));
    expect(document.body.textContent).toContain("Ledger Office");
    expect(document.body.textContent).toContain("New chat");
    expect(document.body.textContent).toContain("Messages");
    expect(document.body.textContent).toContain("Settings");
  });

  it("searches message content and opens the exact hit", async () => {
    const view = props();
    render(CommandPalette, { props: view });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));

    await fireEvent.input(searchInput(), { target: { value: "invoice" } });
    await waitFor(
      () =>
        expect(invokeMock).toHaveBeenCalledWith("search_messages", {
          query: "invoice",
          limit: 30,
        }),
      { timeout: 2_000 },
    );
    await waitFor(() => expect(document.body.textContent).toContain("Invoice review"));

    const result = Array.from(document.querySelectorAll('[role="option"]')).find((item) =>
      item.textContent?.includes("Invoice review"),
    );
    await fireEvent.click(result!);
    expect(view.onOpenMessage).toHaveBeenCalledWith(hit);
    expect(view.onClose).toHaveBeenCalled();
  });

  it("selects the first result with Enter and dismisses on Escape", async () => {
    const view = props();
    render(CommandPalette, { props: view });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));

    const input = searchInput();
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(view.onSelectBot).toHaveBeenCalledWith("bot-1");

    const dialog = document.querySelector('[role="dialog"]')!;
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(view.onClose).toHaveBeenCalled();
  });

  it("finds a settings page by name", async () => {
    const view = props();
    render(CommandPalette, { props: view });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));

    await fireEvent.input(searchInput(), { target: { value: "theme" } });
    await waitFor(() => expect(document.body.textContent).toContain("Themes"));
    const result = Array.from(document.querySelectorAll('[role="option"]')).find((item) =>
      item.textContent?.includes("Themes"),
    );
    await fireEvent.click(result!);
    expect(view.onOpenSettings).toHaveBeenCalledWith("themes");
  });
});
