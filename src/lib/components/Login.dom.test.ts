// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import Login from "$lib/components/Login.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

describe("account login", () => {
  beforeEach(() => {
    setLocale("en");
    invokeMock.mockReset();
  });

  afterEach(() => cleanup());

  it("offers to create the owner account on a fresh install", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "account_launcher_status") {
        return {
          accounts: [],
          can_create_owner: true,
          desktop_running: false,
        };
      }
      return undefined;
    });
    render(Login);
    await waitFor(() =>
      expect(document.body.textContent).toContain("Create the owner account"),
    );
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("signs in to an existing account without axe violations", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "account_launcher_status") {
        return {
          accounts: [
            {
              username: "owner",
              role: "owner",
              owner: true,
              created_at: "2026-11-18 10:00:00",
            },
          ],
          can_create_owner: false,
          desktop_running: false,
        };
      }
      return undefined;
    });
    render(Login);
    await waitFor(() => expect(document.body.textContent).toContain("Sign in"));
    await waitFor(() =>
      expect(
        (
          document.querySelector(
            'input[autocomplete="username"]',
          ) as HTMLInputElement | null
        )?.value,
      ).toBe("owner"),
    );
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("shows a workspace picker when an account has shared access", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "account_launcher_status") {
        return {
          accounts: [
            {
              username: "bob",
              role: "owner",
              owner: true,
              created_at: "2026-11-18 10:00:00",
            },
          ],
          can_create_owner: false,
          desktop_running: false,
        };
      }
      if (command === "login_account") {
        return {
          login_id: "login-1",
          workspaces: [
            { id: "w1", username: "alice", role: "operator", owner: false },
            { id: "w2", username: "bob", role: "owner", owner: true },
          ],
        };
      }
      return undefined;
    });
    render(Login);
    await waitFor(() =>
      expect(document.body.textContent).toContain("Sign in"),
    );
    const password = document.querySelector(
      'input[type="password"]',
    ) as HTMLInputElement;
    await fireEvent.input(password, { target: { value: "another good password" } });
    const form = document.querySelector("form") as HTMLFormElement;
    await fireEvent.submit(form);
    await waitFor(() =>
      expect(document.body.textContent).toContain("Choose a workspace"),
    );
    expect(document.body.textContent).toContain("alice");
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });
});
