// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor, within } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import VaultPanel from "./VaultPanel.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const openMock = vi.mocked(openDialog);

const item = {
  id: "v1",
  kind: "login",
  name: "GitHub",
  username: "ada",
  website: "https://github.com",
  last4: null,
  created_at: "2026-11-09T00:00:00Z",
  updated_at: "2026-11-09T00:00:00Z",
};

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  openMock.mockReset();
  openMock.mockResolvedValue("/tmp/vault.json");
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_vault_items") return [item];
    if (command === "reveal_vault_item") return { password: "hunter2" };
    if (command === "delete_vault_item") return null;
    if (command === "preview_vault_import") {
      return {
        format: "Bitwarden JSON",
        total: 3,
        importable: 1,
        duplicates: 1,
        skipped: 1,
        more: false,
        items: [
          {
            kind: "login",
            name: "GitLab",
            username: "ada",
            website: "https://gitlab.com",
            duplicate: false,
          },
          {
            kind: "login",
            name: "GitHub",
            username: "ada",
            website: "https://github.com",
            duplicate: true,
          },
        ],
      };
    }
    if (command === "import_vault_items") {
      return { format: "Bitwarden JSON", imported: 1, duplicates: 1, skipped: 1, failed: 0 };
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("VaultPanel", () => {
  it("lists metadata and reveals a secret only on request", async () => {
    const { getByText, queryByText } = render(VaultPanel);
    await waitFor(() => expect(getByText("GitHub")).toBeTruthy());
    expect(queryByText(/hunter2/)).toBeNull();

    await fireEvent.click(document.querySelector('[aria-label="Reveal"]')!);
    await waitFor(() => expect(getByText(/hunter2/)).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith("reveal_vault_item", { id: "v1" });
  });

  it("previews an export and imports only non-duplicates", async () => {
    const { getByText, getByLabelText } = render(VaultPanel);
    await waitFor(() => expect(getByText("GitHub")).toBeTruthy());

    await fireEvent.click(getByText("Import"));
    await waitFor(() => expect(getByLabelText("Import vault")).toBeTruthy());
    const preview = getByLabelText("Import vault");
    expect(openMock).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("preview_vault_import", {
      path: "/tmp/vault.json",
    });
    expect(within(preview).getByText("GitLab")).toBeTruthy();
    expect(within(preview).getByText("1 importable")).toBeTruthy();
    expect(within(preview).getByText("1 duplicates")).toBeTruthy();
    expect(within(preview).getByText("Duplicate")).toBeTruthy();
    expect(preview.textContent).not.toContain("hunter2");

    await fireEvent.click(within(preview).getByText("Import 1 items"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_vault_items", {
        path: "/tmp/vault.json",
      }),
    );
    await waitFor(() =>
      expect(getByText("Imported 1 vault items.")).toBeTruthy(),
    );
  });

  it("saves a login with only the secret in the payload", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_vault_items") return [];
      if (command === "save_vault_item") return "v2";
      throw new Error(`unexpected command ${command}`);
    });
    const { getByText, getByLabelText } = render(VaultPanel);
    await waitFor(() => expect(getByText("The vault is empty.")).toBeTruthy());
    await fireEvent.click(getByText("Add item"));
    await fireEvent.input(getByLabelText("Name"), { target: { value: "Bank" } });
    await fireEvent.input(getByLabelText("Username / holder"), { target: { value: "ada" } });
    await fireEvent.input(getByLabelText("Secret"), { target: { value: "hunter2" } });
    await fireEvent.click(getByText("Save to vault"));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("save_vault_item", {
      item: expect.objectContaining({
        kind: "login",
        name: "Bank",
        username: "ada",
        secret: { password: "hunter2" },
      }),
    }));
  });
});
