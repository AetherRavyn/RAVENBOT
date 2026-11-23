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
  total_spend: 1.3845,
  total_model_cost: 1.0,
  total_tool_cost: 0.2345,
  total_utility_cost: 0.15,
  total_utility_tokens: 300,
  tool_pricing: [
    {
      id: "tavily_search",
      label: "Tavily Search",
      price_usd: 0.008,
      source: "catalog",
      note: "one Tavily basic-search credit at $0.008",
    },
    {
      id: "image_gen",
      label: "Image Generation",
      price_usd: 0.5,
      source: "user",
      note: "your configured per-call price",
    },
    {
      id: "other_tool",
      label: "Other Tool",
      price_usd: 0,
      source: "inactive",
      note: "not configured",
    },
  ],
  failed_runs: 2,
  completed_runs: 10,
  agents: [
    { bot_id: "bot-1", runs: 10, tokens: 1000, cost: 1.2, failed: 1, completed: 8 },
    { bot_id: "bot-2", runs: 2, tokens: 234, cost: 0.0345, failed: 1, completed: 2 },
  ],
  offices: [
    { office_id: "office-1", name: "Billing Office", budget: 2.0, runs: 8, tokens: 800, cost: 1.0, failed: 0, completed: 8 },
    { office_id: null, name: null, runs: 4, tokens: 434, cost: 0.2345, failed: 2, completed: 2 },
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
    expect(document.body.textContent).toContain("$1.3845");
    expect(document.body.textContent).toContain("1,234");
    expect(document.body.textContent).toContain("83.3%");
    expect(document.body.textContent).toContain("Research Agent");
    expect(invokeMock).toHaveBeenCalledWith("get_usage_summary", { days: 30 });
  });

  it("switches to office attribution and keeps direct runs visible", async () => {
    render(UsagePanel, { props: { bots } });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));

    const byOffice = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "By office",
    );
    await fireEvent.click(byOffice!);
    await waitFor(() =>
      expect(document.body.textContent).toContain("Billing Office"),
    );
    expect(document.body.textContent).toContain("Direct chats");
    expect(document.body.textContent).toContain("$1.0000");
    expect(document.body.textContent).toContain("$0.2345");
    expect(document.body.textContent).toContain("50% of $2.0000");
    expect(document.body.textContent).toContain(
      "Office totals include its own conversation",
    );
  });

  it("shows the spend split and the paid-tool price audit", async () => {
    render(UsagePanel, { props: { bots } });
    await waitFor(() => expect(document.body.textContent).toContain("Billing Agent"));

    expect(document.body.textContent).toContain("Where the spend went");
    expect(document.body.textContent).toContain("$1.3845");
    expect(document.body.textContent).toContain("$1.0000");
    expect(document.body.textContent).toContain("$0.2345");
    expect(document.body.textContent).toContain("$0.1500");
    expect(document.body.textContent).toContain(
      "Utility calls run outside an agent turn",
    );
    expect(document.body.textContent).toContain("Tavily Search");
    expect(document.body.textContent).toContain("$0.0080/call");
    expect(document.body.textContent).toContain("estimate");
    expect(document.body.textContent).toContain("Image Generation");
    expect(document.body.textContent).toContain("your price");
    expect(document.body.textContent).not.toContain("Other Tool");
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
