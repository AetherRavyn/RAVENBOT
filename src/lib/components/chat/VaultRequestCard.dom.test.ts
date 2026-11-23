// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import VaultRequestCard from "./VaultRequestCard.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

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
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "list_vault_items") return [item];
    if (command === "fill_browser_from_vault") {
      return { ok: true, origin: "https://github.com", item: "GitHub" };
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("VaultRequestCard", () => {
  it("fills the chosen login and answers with metadata, not the secret", async () => {
    const onAnswer = vi.fn();
    const { getByText } = render(VaultRequestCard, {
      props: {
        header: "Sign in",
        question: "Sign in to GitHub?",
        payload: { site: "https://github.com", purpose: "Check the issue" },
        onAnswer,
      },
    });
    await waitFor(() => expect(getByText("GitHub")).toBeTruthy());
    await fireEvent.click(getByText("GitHub"));
    await waitFor(() => expect(onAnswer).toHaveBeenCalled());
    const answer = JSON.parse(onAnswer.mock.calls[0][0]);
    expect(answer.action_id).toBe("fill");
    expect(answer.values.item_id).toBe("v1");
    expect(answer.values.origin).toBe("https://github.com");
    expect(JSON.stringify(answer)).not.toContain("password");
  });
});
