// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import RemoteSsoCard from "./RemoteSsoCard.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const savedSummary = {
  issuer: "https://idp.example.com",
  client_id: "ravenbot-desktop",
  redirect_url: "https://bot.example.com/api/sso/callback",
  scopes: "openid profile email",
  username_claim: "preferred_username",
};

const savedStatus = {
  enabled: true,
  running: true,
  url: "https://bot.example.com",
  port: 8790,
  allow_lan: false,
  tls: true,
  fingerprint: null,
  sso_enabled: true,
  sso: savedSummary,
  devices: [],
  audit: [],
};

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async () => savedStatus);
});
afterEach(cleanup);

describe("RemoteSsoCard", () => {
  it("renders the saved fields and the security help", () => {
    const { getByLabelText, getByText } = render(RemoteSsoCard, {
      props: {
        sso: savedSummary,
        enabled: true,
        onSaved: vi.fn(),
      },
    });

    expect((getByLabelText("Issuer URL") as HTMLInputElement).value).toBe(
      savedSummary.issuer,
    );
    expect((getByLabelText("Client ID") as HTMLInputElement).value).toBe(
      savedSummary.client_id,
    );
    expect((getByLabelText("Redirect URL") as HTMLInputElement).value).toBe(
      savedSummary.redirect_url,
    );
    expect((getByLabelText("Scopes") as HTMLInputElement).value).toBe(
      savedSummary.scopes,
    );
    expect((getByLabelText("Username claim") as HTMLInputElement).value).toBe(
      savedSummary.username_claim,
    );
    expect(
      (getByLabelText("Enable single sign-on") as HTMLInputElement).checked,
    ).toBe(true);

    expect(
      getByText(/Public client with authorization code \+ PKCE/),
    ).toBeTruthy();
    expect(getByText(/Existing RAVENBOT accounts only/)).toBeTruthy();
    expect(getByText(/No SCIM, group mapping or auto-provisioning/)).toBeTruthy();
    expect(getByText(/Saving restarts a running remote listener/)).toBeTruthy();
    expect(getByText(/Saving does not contact the provider/)).toBeTruthy();
  });

  it("saves with camelCase arguments, applies the response and calls onSaved", async () => {
    const onSaved = vi.fn();
    // The backend normalizes a trailing slash; the form should show the
    // response value, not the raw typed one.
    invokeMock.mockResolvedValueOnce({
      ...savedStatus,
      sso: { ...savedSummary, issuer: "https://idp.example.com" },
    });
    const { getByLabelText, getByText } = render(RemoteSsoCard, {
      props: { sso: null, enabled: false, onSaved },
    });

    await fireEvent.click(getByLabelText("Enable single sign-on"));
    await fireEvent.input(getByLabelText("Issuer URL"), {
      target: { value: "  https://idp.example.com/  " },
    });
    await fireEvent.input(getByLabelText("Client ID"), {
      target: { value: "ravenbot-desktop" },
    });
    await fireEvent.input(getByLabelText("Redirect URL"), {
      target: { value: "https://bot.example.com/api/sso/callback" },
    });
    await fireEvent.input(getByLabelText("Scopes"), {
      target: { value: "openid profile email" },
    });
    await fireEvent.input(getByLabelText("Username claim"), {
      target: { value: "preferred_username" },
    });

    await fireEvent.click(getByText("Save SSO settings"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("configure_remote_sso", {
        enabled: true,
        issuer: "https://idp.example.com/",
        clientId: "ravenbot-desktop",
        redirectUrl: "https://bot.example.com/api/sso/callback",
        scopes: "openid profile email",
        usernameClaim: "preferred_username",
      }),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith(savedStatus));
    expect(
      (getByLabelText("Issuer URL") as HTMLInputElement).value,
    ).toBe("https://idp.example.com");
    expect(getByText("SSO settings saved.")).toBeTruthy();
  });

  it("shows a rejected save without claiming success", async () => {
    invokeMock.mockRejectedValueOnce(
      new Error("The issuer must use HTTPS (plain HTTP is allowed only for loopback)."),
    );
    const { getByText, queryByText } = render(RemoteSsoCard, {
      props: {
        sso: savedSummary,
        enabled: true,
        onSaved: vi.fn(),
      },
    });

    await fireEvent.click(getByText("Save SSO settings"));

    await waitFor(() =>
      expect(getByText(/must use HTTPS/)).toBeTruthy(),
    );
    expect(queryByText("SSO settings saved.")).toBeNull();
  });

  it("can disable SSO and save enabled false", async () => {
    const onSaved = vi.fn();
    invokeMock.mockResolvedValueOnce({
      ...savedStatus,
      enabled: true,
      running: true,
      sso_enabled: false,
      sso: null,
    });
    const { getByLabelText, getByText } = render(RemoteSsoCard, {
      props: {
        sso: savedSummary,
        enabled: true,
        onSaved,
      },
    });

    await fireEvent.click(getByLabelText("Enable single sign-on"));
    await fireEvent.click(getByText("Save SSO settings"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "configure_remote_sso",
        expect.objectContaining({ enabled: false }),
      ),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(
      (getByLabelText("Enable single sign-on") as HTMLInputElement).checked,
    ).toBe(false);
  });

  it("does not offer a fake test or connection button", () => {
    const { queryAllByRole } = render(RemoteSsoCard, {
      props: {
        sso: savedSummary,
        enabled: true,
        onSaved: vi.fn(),
      },
    });

    const buttons = queryAllByRole("button");
    expect(buttons).toHaveLength(1);
    expect(buttons[0].textContent).toContain("Save SSO settings");
    expect(buttons.some((button) => /test|connect|verify/i.test(button.textContent ?? ""))).toBe(false);
  });
});
