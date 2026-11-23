// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import BrowserVaultSave from "./BrowserVaultSave.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "save_browser_secret_to_vault") {
      return { id: "v1", updated: true, website: "https://app.example" };
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("BrowserVaultSave", () => {
  it("saves the typed password with the card origin", async () => {
    const onSaved = vi.fn();
    const { getByText } = render(BrowserVaultSave, {
      props: {
        origin: "https://app.example",
        initialName: "App login",
        initialUsername: "ada",
        secret: "hunter2",
        onCancel: vi.fn(),
        onSaved,
      },
    });

    await fireEvent.click(getByText("Save login"));
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith(true));
    expect(invokeMock).toHaveBeenCalledWith("save_browser_secret_to_vault", {
      name: "App login",
      username: "ada",
      website: "https://app.example",
      secret: "hunter2",
    });
  });

  it("requires a name before enabling save", () => {
    const { getByText } = render(BrowserVaultSave, {
      props: {
        origin: "https://app.example",
        initialName: "",
        initialUsername: "",
        secret: "hunter2",
        onCancel: vi.fn(),
        onSaved: vi.fn(),
      },
    });
    const button = Array.from(document.querySelectorAll("button")).find(
      (candidate) => candidate.textContent?.trim() === "Save login",
    );
    expect(button?.disabled).toBe(true);
    expect(getByText("Website")).toBeTruthy();
  });
});
