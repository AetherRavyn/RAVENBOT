// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AttentionWebhookSettings from "./AttentionWebhookSettings.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "get_attention_webhook") {
      return { configured: false, host: null };
    }
    if (command === "set_attention_webhook") {
      return { configured: true, host: "hooks.example.com" };
    }
    if (command === "test_attention_webhook") return null;
    if (command === "clear_attention_webhook") return null;
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("AttentionWebhookSettings", () => {
  it("saves, tests, and removes an HTTPS webhook", async () => {
    const { getByLabelText, getByText } = render(AttentionWebhookSettings);
    await waitFor(() =>
      expect(getByText("No webhook configured.")).toBeTruthy(),
    );
    await fireEvent.input(getByLabelText("Webhook URL"), {
      target: { value: "https://hooks.example.com/secret" },
    });
    await fireEvent.click(getByText("Save webhook"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_attention_webhook", {
        url: "https://hooks.example.com/secret",
      }),
    );
    await waitFor(() =>
      expect(getByText("Configured for hooks.example.com")).toBeTruthy(),
    );

    await fireEvent.click(getByText("Send test"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("test_attention_webhook"),
    );
    await waitFor(() => expect(getByText("Test alert delivered.")).toBeTruthy());

    await fireEvent.click(getByText("Remove webhook"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("clear_attention_webhook"),
    );
    await waitFor(() =>
      expect(getByText("No webhook configured.")).toBeTruthy(),
    );
  });
});
