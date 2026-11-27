// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { openExternal } from "$lib/external";
import CommercePanel from "./CommercePanel.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("$lib/external", () => ({ openExternal: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const openMock = vi.mocked(openExternal);

const settings = {
  enabled: true,
  currency: "USD",
  per_request_limit_minor: "20000",
  daily_limit_minor: "50000",
  allowlist: "example.com",
  valid: true,
  validation_error: null as string | null,
};

function request(overrides: Record<string, unknown> = {}) {
  return {
    id: "spend-1",
    bot_id: "bot-1",
    thread_id: "thread-1",
    run_id: "run-1",
    title: "Replacement SSD",
    merchant: "Example Store",
    merchant_url: "https://shop.example.com/checkout/ssd",
    amount_minor: 12_500,
    currency: "USD",
    reason: "The test machine's drive failed.",
    status: "pending",
    decided_by: null,
    decided_at: null,
    decision_note: null,
    purchased_at: null,
    created_at: "2026-11-27T10:00:00Z",
    updated_at: "2026-11-27T10:00:00Z",
    ...overrides,
  };
}

function renderPanel(row = request(), policy = settings) {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_commerce_settings") return policy;
    if (command === "list_spend_requests") return [row];
    if (command === "set_commerce_settings") return policy;
    if (command === "approve_spend_request") {
      return request({ status: "approved", decided_by: "owner", decided_at: "2026-11-27T11:00:00Z" });
    }
    if (command === "deny_spend_request") {
      return request({ status: "denied", decided_by: "owner", decided_at: "2026-11-27T11:00:00Z" });
    }
    if (command === "mark_spend_purchased") {
      return request({
        status: "purchased",
        decided_by: "owner",
        decided_at: "2026-11-27T11:00:00Z",
        purchased_at: "2026-11-27T12:00:00Z",
      });
    }
    if (command === "cancel_spend_request") {
      return request({ status: "cancelled", decided_by: "owner", decided_at: "2026-11-27T11:00:00Z" });
    }
    throw new Error(`unexpected command ${command}`);
  });
  return render(CommercePanel);
}

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  openMock.mockReset();
  vi.spyOn(window, "confirm").mockReturnValue(true);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("CommercePanel", () => {
  it("loads the policy and pending request with honest copy", async () => {
    const { getByRole } = renderPanel();
    await waitFor(() =>
      expect(document.body.textContent).toContain("Replacement SSD"),
    );
    expect(document.body.textContent).toContain("only you can approve it");
    expect(document.body.textContent).toContain("RAVENBOT never checks out");
    expect(document.body.textContent).toContain("USD");
    expect(document.body.textContent).toContain("minor units");
    expect(document.body.textContent).toContain("The test machine's drive failed.");
    expect((getByRole("button", { name: /approve/i }) as HTMLButtonElement).disabled).toBe(
      false,
    );
    expect(document.body.textContent).toContain("Pending");
  });

  it("saves the policy as normalized minor-unit strings", async () => {
    renderPanel();
    await waitFor(() => expect(document.body.textContent).toContain("Replacement SSD"));
    const currency = document.querySelector("#commerce-currency") as HTMLInputElement;
    const perRequest = document.querySelector("#commerce-per-request") as HTMLInputElement;
    await fireEvent.input(currency, { target: { value: "eur" } });
    await fireEvent.input(perRequest, { target: { value: "25000" } });
    await fireEvent.click(
      Array.from(document.querySelectorAll("button")).find((button) =>
        button.textContent?.includes("Save policy"),
      )!,
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_commerce_settings", {
        input: {
          enabled: true,
          currency: "eur",
          per_request_limit_minor: "25000",
          daily_limit_minor: "50000",
          allowlist: "example.com",
        },
      }),
    );
  });

  it("approves a pending request without opening checkout automatically", async () => {
    const { getByRole } = renderPanel();
    await waitFor(() => expect(document.body.textContent).toContain("Replacement SSD"));
    await fireEvent.click(getByRole("button", { name: /^approve$/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("approve_spend_request", {
        id: "spend-1",
      }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Approved"));
    expect(openMock).not.toHaveBeenCalled();
    expect(getByRole("button", { name: /open checkout/i })).toBeTruthy();
  });

  it("opens the merchant checkout only after approval", async () => {
    const { getByRole } = renderPanel(
      request({
        status: "approved",
        decided_by: "owner",
        decided_at: "2026-11-27T11:00:00Z",
      }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Approved"));
    await fireEvent.click(getByRole("button", { name: /open checkout/i }));
    expect(openMock).toHaveBeenCalledWith("https://shop.example.com/checkout/ssd");
  });

  it("denies a pending request", async () => {
    const view = renderPanel();
    await waitFor(() => expect(document.body.textContent).toContain("Replacement SSD"));
    await fireEvent.click(view.getByRole("button", { name: /^deny$/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("deny_spend_request", { id: "spend-1" }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Denied"));
    expect(view.queryByRole("button", { name: /open checkout/i })).toBeNull();
  });

  it("marks an approved request purchased only after confirmation", async () => {
    const { getByRole } = renderPanel(
      request({ status: "approved", decided_by: "owner", decided_at: "2026-11-27T11:00:00Z" }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Approved"));
    await fireEvent.click(getByRole("button", { name: /mark purchased/i }));
    expect(window.confirm).toHaveBeenCalled();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("mark_spend_purchased", { id: "spend-1" }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Purchased"));
  });

  it("cancels an approved request", async () => {
    const { getByRole } = renderPanel(
      request({ status: "approved", decided_by: "owner", decided_at: "2026-11-27T11:00:00Z" }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Approved"));
    await fireEvent.click(getByRole("button", { name: /^cancel$/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("cancel_spend_request", { id: "spend-1" }),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Cancelled"));
  });

  it("shows an invalid saved policy instead of hiding it", async () => {
    renderPanel(request(), {
      ...settings,
      valid: false,
      validation_error: "The daily limit is invalid: too small.",
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("The saved policy is invalid"),
    );
    expect(document.body.textContent).toContain("The daily limit is invalid: too small.");
  });

  it("has no serious accessibility violations", async () => {
    const view = renderPanel();
    await waitFor(() => expect(document.body.textContent).toContain("Replacement SSD"));
    const main = document.createElement("main");
    document.body.appendChild(main);
    main.appendChild(view.container);
    expect(await a11yViolations(main)).toEqual([]);
  });
});
