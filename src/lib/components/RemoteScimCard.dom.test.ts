// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import RemoteScimCard from "./RemoteScimCard.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const savedScim = {
  enabled: true,
  active: true,
  token_configured: true,
  default_role: "operator",
  last_used_at: "2026-11-27T10:00:00Z",
};

const savedStatus = {
  enabled: true,
  running: true,
  url: "https://bot.example.com",
  port: 8790,
  allow_lan: false,
  tls: true,
  fingerprint: null,
  sso_enabled: false,
  sso: null,
  scim: savedScim,
  devices: [],
  audit: [],
};

let clipboardWrite: ReturnType<typeof vi.fn>;

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(savedStatus);
  clipboardWrite = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: clipboardWrite },
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("RemoteScimCard", () => {
  it("renders the saved role, token state, last-used time and help", () => {
    const { getByLabelText, getByRole, getByText } = render(RemoteScimCard, {
      props: { scim: savedScim, onSaved: vi.fn() },
    });

    expect(
      (getByLabelText("Enable SCIM provisioning") as HTMLInputElement).checked,
    ).toBe(true);
    expect(
      getByRole("button", { name: "Default workspace role" }).textContent,
    ).toContain("Operator");
    expect(getByText("A token is configured.")).toBeTruthy();
    expect(getByText(/Last used /)).toBeTruthy();
    expect(getByText(/Users only/)).toBeTruthy();
    expect(getByText(/workspace owner and host owner/)).toBeTruthy();
    expect(getByText(/shared with another workspace/)).toBeTruthy();
    expect(getByText(/soft deactivate/)).toBeTruthy();
    expect(getByText(/random password/)).toBeTruthy();
    expect(getByText(/Saving restarts a running remote listener/)).toBeTruthy();
    expect(getByText(/does not contact the IdP/)).toBeTruthy();
  });

  it("saves with camelCase arguments, applies the response and calls onSaved", async () => {
    const onSaved = vi.fn();
    const response = {
      ...savedStatus,
      scim: { ...savedScim, default_role: "operator" },
    };
    invokeMock.mockResolvedValueOnce(response);
    const { getByLabelText, getByRole, getByText } = render(RemoteScimCard, {
      props: {
        scim: { ...savedScim, enabled: false, default_role: "operator" },
        onSaved,
      },
    });

    await fireEvent.click(getByLabelText("Enable SCIM provisioning"));
    await fireEvent.click(getByText("Save SCIM settings"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("configure_remote_scim", {
        enabled: true,
        defaultRole: "operator",
      }),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith(response));
    expect(getByText("SCIM settings saved.")).toBeTruthy();
    expect(
      getByRole("button", { name: "Default workspace role" }).textContent,
    ).toContain("Operator");
  });

  it("generates a token, shows it once, copies it and calls onSaved", async () => {
    const onSaved = vi.fn();
    const response = {
      token: "a".repeat(64),
      status: { ...savedStatus, scim: { ...savedScim, token_configured: true } },
    };
    invokeMock.mockResolvedValueOnce(response);
    const { getByLabelText, getByText, queryByText } = render(RemoteScimCard, {
      props: {
        scim: {
          ...savedScim,
          active: false,
          token_configured: false,
          last_used_at: null,
        },
        onSaved,
      },
    });

    expect(getByText("Never used")).toBeTruthy();
    await fireEvent.click(getByText("Generate token"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_remote_scim_token"),
    );
    await waitFor(() =>
      expect(
        (getByLabelText("Bearer token") as HTMLInputElement).value,
      ).toBe(response.token),
    );
    expect(getByText(/only time the token will be shown/)).toBeTruthy();
    expect(onSaved).toHaveBeenCalledWith(response.status);

    await fireEvent.click(getByText("Copy token"));
    await waitFor(() =>
      expect(clipboardWrite).toHaveBeenCalledWith(response.token),
    );
    await waitFor(() => expect(getByText("Copied")).toBeTruthy());
    expect(queryByText("Save SCIM settings")).toBeTruthy();
  });

  it("confirms replace and revoke before calling the backend", async () => {
    const onSaved = vi.fn();
    const confirmMock = vi.spyOn(window, "confirm").mockReturnValue(false);
    invokeMock
      .mockResolvedValueOnce({
        token: "b".repeat(64),
        status: { ...savedStatus, scim: { ...savedScim } },
      })
      .mockResolvedValueOnce({
        ...savedStatus,
        scim: {
          ...savedScim,
          enabled: false,
          active: false,
          token_configured: false,
          last_used_at: null,
        },
      });
    const { getByText, queryByLabelText } = render(RemoteScimCard, {
      props: { scim: savedScim, onSaved },
    });

    await fireEvent.click(getByText("Replace token"));
    expect(confirmMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalled();

    confirmMock.mockReturnValue(true);
    await fireEvent.click(getByText("Replace token"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_remote_scim_token"),
    );
    await waitFor(() =>
      expect(queryByLabelText("Bearer token")).toBeTruthy(),
    );

    await fireEvent.click(getByText("Revoke token"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("revoke_remote_scim_token"),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(2));
    expect(queryByLabelText("Bearer token")).toBeNull();
    expect(getByText("No token yet. Generate one before enabling SCIM.")).toBeTruthy();
  });

  it("shows rejected save and generate errors without a false saved state", async () => {
    invokeMock
      .mockRejectedValueOnce(new Error("Saved configuration is invalid."))
      .mockRejectedValueOnce(new Error("Could not create a token."));
    const onSaved = vi.fn();
    const { getByText, queryByText } = render(RemoteScimCard, {
      props: {
        scim: { ...savedScim, token_configured: false, active: false },
        onSaved,
      },
    });

    await fireEvent.click(getByText("Save SCIM settings"));
    await waitFor(() =>
      expect(getByText("Saved configuration is invalid.")).toBeTruthy(),
    );
    expect(queryByText("SCIM settings saved.")).toBeNull();
    expect(onSaved).not.toHaveBeenCalled();

    await fireEvent.click(getByText("Generate token"));
    await waitFor(() => expect(getByText("Could not create a token.")).toBeTruthy());
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("warns when SCIM is enabled but inactive or has no token", () => {
    const { getByText, rerender } = render(RemoteScimCard, {
      props: {
        scim: { ...savedScim, active: false, token_configured: true },
        onSaved: vi.fn(),
      },
    });
    expect(getByText(/enabled but not accepting requests/)).toBeTruthy();

    rerender({
      scim: { ...savedScim, active: true, token_configured: false },
      onSaved: vi.fn(),
    });
    expect(getByText(/enabled but not accepting requests/)).toBeTruthy();
  });

  it("does not offer a fake test or connection button", () => {
    const { queryAllByRole } = render(RemoteScimCard, {
      props: { scim: savedScim, onSaved: vi.fn() },
    });

    const buttons = queryAllByRole("button");
    expect(buttons.length).toBeGreaterThan(0);
    expect(
      buttons.some((button) =>
        /test|connect|verify/i.test(button.textContent ?? ""),
      ),
    ).toBe(false);
  });
});
