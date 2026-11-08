// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import UsagePanel from "$lib/components/UsagePanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const summary = {
  days: 30,
  total_runs: 12,
  total_tokens: 1234,
  total_cost: 1.2345,
  failed_runs: 2,
  completed_runs: 10,
  agents: [
    { bot_id: "bot-1", runs: 10, tokens: 1000, cost: 1.2, failed: 1, completed: 8 },
    { bot_id: "bot-2", runs: 2, tokens: 234, cost: 0.0345, failed: 1, completed: 2 },
  ],
};

const bots = [
  { id: "bot-1", name: "Billing Agent", avatar_url: null, avatar_style: null },
  { id: "bot-2", name: "Research Agent", avatar_url: null, avatar_style: null },
];

describe("UsagePanel", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_usage_summary") return summary;
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("shows real fleet totals and per-agent spend", async () => {
    render(UsagePanel, { props: { bots } });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));
    expect(document.body.textContent).toContain("$1.2345");
    expect(document.body.textContent).toContain("1,234");
    expect(document.body.textContent).toContain("83.3%");
    expect(document.body.textContent).toContain("Research Agent");
    expect(invokeMock).toHaveBeenCalledWith("get_usage_summary", { days: 30 });
  });

  it("asks for the selected window", async () => {
    render(UsagePanel, { props: { bots } });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));

    const sevenDays = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "7 days",
    );
    await fireEvent.click(sevenDays!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_usage_summary", { days: 7 }),
    );
  });
});
