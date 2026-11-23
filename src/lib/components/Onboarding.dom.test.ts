// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import Onboarding from "$lib/components/Onboarding.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

describe("Onboarding", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_configured_providers") return [];
      if (command === "get_default_agent") return null;
      if (command === "probe_ollama") {
        return {
          base_url: "http://127.0.0.1:11434",
          count: 2,
          models: [
            { id: "llama3.1:8b", name: "Llama 3.1 8B", supports_tools: true },
            { id: "vision:latest", name: "Vision", supports_tools: false },
          ],
        };
      }
      if (command === "get_default_model") return { provider: "ollama", model: "llama3.1:8b" };
      if (command === "create_default_agent") {
        return { id: "bot-1", name: "The Raven" };
      }
      throw new Error(`unexpected command ${command}`);
    });
  });

  afterEach(cleanup);

  it("uses a detected local Ollama model without an API key", async () => {
    const onSkip = vi.fn();
    const { getByText, queryByText, container } = render(Onboarding, {
      props: { open: true, onOpenSettings: vi.fn(), onSkip },
    });

    await waitFor(() => expect(getByText("Local Ollama detected")).toBeTruthy());
    expect(await a11yViolations(container)).toEqual([]);
    await fireEvent.click(getByText("Use Llama 3.1 8B"));

    await waitFor(() => expect(getByText("ollama/llama3.1:8b")).toBeTruthy());
    await fireEvent.click(getByText("Create The Raven"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_default_agent", {
        name: "The Raven",
        provider: "ollama",
        model: "llama3.1:8b",
      }),
    );
    expect(onSkip).toHaveBeenCalled();
    expect(queryByText("Local Ollama detected")).toBeNull();
  });
});
