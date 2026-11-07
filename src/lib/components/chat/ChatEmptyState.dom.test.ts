// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import ChatEmptyState from "./ChatEmptyState.svelte";
import { setLocale } from "$lib/i18n";

describe("ChatEmptyState", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
  });
  afterEach(cleanup);

  it("gives a fresh thread a purpose and three real prompts", () => {
    const { container, getByText } = render(ChatEmptyState, {
      props: {
        name: "Scout",
        description: "Reads the workspace before it writes.",
        onPick: vi.fn(),
      },
    });

    expect(getByText("Scout")).toBeTruthy();
    expect(getByText("Reads the workspace before it writes.")).toBeTruthy();
    expect(container.querySelectorAll(".chat-empty__suggestion")).toHaveLength(3);
  });

  it("sends the full prompt, not just the card label", async () => {
    const onPick = vi.fn();
    const { getByText } = render(ChatEmptyState, {
      props: { name: "Scout", onPick },
    });
    await fireEvent.click(getByText("Map this workspace"));
    expect(onPick).toHaveBeenCalledWith(
      "Map this workspace: entry points, test command, and the riskiest module.",
    );
  });
});
