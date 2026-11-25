// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import TeamPanel from "./TeamPanel.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const owner = {
  username: "swadhin",
  role: "owner",
  owner: true,
  disabled: false,
  created_at: "2026-01-01T00:00:00Z",
};
const alice = {
  username: "alice",
  role: "operator",
  owner: false,
  disabled: false,
  created_at: "2026-01-02T00:00:00Z",
};

beforeEach(() => {
  cleanup();
  setLocale("en");
  if (!window.matchMedia) {
    window.matchMedia = ((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;
  }
  if (!globalThis.ResizeObserver) {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
  let members: any[] = [owner, alice];
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "list_workspace_members":
        return members;
      case "add_workspace_member":
        members = [
          ...members,
          {
            username: args?.username,
            role: args?.role,
            owner: false,
            disabled: false,
            created_at: "2026-01-03T00:00:00Z",
          },
        ];
        return members[members.length - 1];
      case "revoke_workspace_access":
        members = members.filter((member) => member.username !== args?.username);
        return true;
      default:
        return null;
    }
  });
});

afterEach(cleanup);

describe("TeamPanel", () => {
  it("lists the owner and members and adds a new account", async () => {
    const { getByText, getByLabelText, getByRole } = render(TeamPanel, {
      props: { currentAccount: { username: "swadhin", role: "owner" } },
    });
    await waitFor(() => expect(getByText("alice")).toBeTruthy());
    expect(getByText("swadhin")).toBeTruthy();
    expect(getByText("Owner")).toBeTruthy();

    await fireEvent.input(getByLabelText("Username"), { target: { value: "bob" } });
    await fireEvent.input(getByLabelText("Password"), {
      target: { value: "bob password 123" },
    });
    await fireEvent.click(getByRole("button", { name: "Add member" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("add_workspace_member", {
        username: "bob",
        password: "bob password 123",
        role: "viewer",
      }),
    );
    await waitFor(() => expect(getByText("bob")).toBeTruthy());
    expect(
      getByText("bob can now open this workspace."),
    ).toBeTruthy();
  });

  it("removes a member only after the inline confirmation", async () => {
    const { getByText, getByRole } = render(TeamPanel, {
      props: { currentAccount: { username: "swadhin", role: "owner" } },
    });
    await waitFor(() => expect(getByText("alice")).toBeTruthy());

    await fireEvent.click(getByRole("button", { name: "Remove alice" }));
    expect(
      getByText("Remove alice from this workspace? Their own account and files stay."),
    ).toBeTruthy();
    await fireEvent.click(getByRole("button", { name: "Remove" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("revoke_workspace_access", {
        username: "alice",
      }),
    );
    await waitFor(() => expect(getByText("alice was removed from this workspace.")).toBeTruthy());
  });

  it("has no serious accessibility violations", async () => {
    const { container, getByText } = render(TeamPanel, {
      props: { currentAccount: { username: "swadhin", role: "owner" } },
    });
    await waitFor(() => expect(getByText("alice")).toBeTruthy());
    expect(await a11yViolations(container)).toEqual([]);
  });

  it("does not fetch the team for a non-owner", async () => {
    const { getByText } = render(TeamPanel, {
      props: { currentAccount: { username: "alice", role: "operator" } },
    });
    expect(getByText("Only the workspace owner can manage the team.")).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith("list_workspace_members");
  });
});
