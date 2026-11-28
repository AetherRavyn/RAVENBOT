// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import CommandPalette from "$lib/components/CommandPalette.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

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
    onOpenShortcuts: vi.fn(),
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

  it("has no serious accessibility violations", async () => {
    render(CommandPalette, { props: props() });
    await waitFor(() => expect(document.activeElement).toBe(searchInput()));
    expect(await a11yViolations(document.body)).toEqual([]);
  });

  it("focuses the search field when it opens", async () => {
    render(CommandPalette, { props: props() });
    await waitFor(() => expect(document.activeElement).toBe(searchInput()));
  });

  it("opens the shortcut sheet from the quick actions", async () => {
    const onOpenShortcuts = vi.fn();
    render(CommandPalette, { props: props({ onOpenShortcuts }) });
    await waitFor(() => expect(document.activeElement).toBe(searchInput()));
    await fireEvent.input(searchInput(), { target: { value: "keyboard" } });
    expect(searchInput().value).toBe("keyboard");
    const option = await waitFor(() => {
      const element = document.getElementById("palette-item-action:shortcuts");
      expect(element).not.toBeNull();
      return element!;
    });
    await fireEvent.click(option);
    expect(onOpenShortcuts).toHaveBeenCalledTimes(1);
  });

  it("accepts printable keys when focus is on the dialog shell", async () => {
    const view = props();
    render(CommandPalette, { props: view });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));

    const dialog = document.querySelector('[role="dialog"]')!;
    await fireEvent.keyDown(dialog, { key: "b" });
    expect(searchInput().value).toBe("b");
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

  it("moves the selection with arrow keys", async () => {
    const view = props();
    render(CommandPalette, { props: view });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));
    const input = searchInput();
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(view.onSelectBot).toHaveBeenCalledWith("bot-2");
  });

  it("returns focus to the opener when it closes", async () => {
    const opener = document.createElement("button");
    opener.textContent = "Open palette";
    document.body.appendChild(opener);
    opener.focus();
    const view = props();
    const { rerender } = render(CommandPalette, { props: view });
    await waitFor(() => expect(document.activeElement).toBe(searchInput()));
    await rerender({ ...view, open: false });
    await waitFor(() => expect(document.activeElement).toBe(opener));
    opener.remove();
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
