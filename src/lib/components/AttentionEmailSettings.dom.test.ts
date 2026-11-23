// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AttentionEmailSettings from "./AttentionEmailSettings.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const emptyStatus = {
  configured: false,
  host: "",
  port: 587,
  security: "starttls",
  username: "",
  from: "",
  to: "",
  has_password: false,
};

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    if (command === "get_attention_email") return emptyStatus;
    if (command === "set_attention_email") {
      const input = args.input;
      return {
        configured: true,
        host: input.host,
        port: input.port,
        security: input.security,
        username: input.username,
        from: input.from,
        to: input.to,
        has_password: Boolean(input.password),
      };
    }
    if (command === "test_attention_email") return null;
    if (command === "clear_attention_email") return null;
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("AttentionEmailSettings", () => {
  it("saves SMTP settings, tests delivery, and removes the configuration", async () => {
    const { getByLabelText, getByText } = render(AttentionEmailSettings);
    await waitFor(() =>
      expect(getByText("No email alerts configured.")).toBeTruthy(),
    );

    await fireEvent.input(getByLabelText("SMTP host"), {
      target: { value: "smtp.example.com" },
    });
    await fireEvent.input(getByLabelText("Username"), {
      target: { value: "alerts@example.com" },
    });
    await fireEvent.input(getByLabelText("Password"), {
      target: { value: "smtp-secret" },
    });
    await fireEvent.input(getByLabelText("From address"), {
      target: { value: "alerts@example.com" },
    });
    await fireEvent.input(getByLabelText("To address"), {
      target: { value: "ops@example.com" },
    });
    await fireEvent.click(getByText("Save email"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_attention_email", {
        input: {
          host: "smtp.example.com",
          port: 587,
          security: "starttls",
          username: "alerts@example.com",
          password: "smtp-secret",
          from: "alerts@example.com",
          to: "ops@example.com",
        },
      }),
    );
    await waitFor(() =>
      expect(
        getByText("Configured for smtp.example.com:587"),
      ).toBeTruthy(),
    );
    expect(getByText("A password is saved.")).toBeTruthy();

    await fireEvent.click(getByText("Send test email"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("test_attention_email"),
    );
    await waitFor(() =>
      expect(getByText("Test email sent.")).toBeTruthy(),
    );

    await fireEvent.click(getByText("Remove email"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("clear_attention_email"),
    );
    await waitFor(() =>
      expect(getByText("Email alerts removed.")).toBeTruthy(),
    );
    await waitFor(() =>
      expect(getByText("No email alerts configured.")).toBeTruthy(),
    );
  });
});
