// @vitest-environment jsdom
/**
 * The OpenBot studio.
 *
 * The studio's whole reason to exist is the six-state strip: a person should
 * be able to see "will I recognise a blocked agent?" before committing. The
 * tests below pin the gallery, the six native buttons, and the commit shape —
 * definition and colour are stored, the previewed state is not.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import OpenBotStudio from "./OpenBotStudio.svelte";
import { OPENBOT_GALLERY, OPENBOT_STATES, openBotCuratedColour, openBotStyleId, openBotUrl } from "$lib/openBotAvatar";

afterEach(cleanup);

function studio(props: Record<string, unknown> = {}) {
  const onApply = vi.fn();
  const onClose = vi.fn();
  render(OpenBotStudio, {
    props: {
      seed: "Ada",
      definitionId: "cat",
      colour: "#f8a01c",
      onApply,
      onClose,
      ...props,
    },
  });
  return { onApply, onClose };
}

describe("OpenBotStudio", () => {
  it("renders a gallery tile for every bundled character, with a still preview and a name", () => {
    studio();
    const tiles = document.querySelectorAll(".openbot-studio__tile");
    expect(tiles).toHaveLength(OPENBOT_GALLERY.length);
    for (const tile of tiles) {
      const img = tile.querySelector("img")!;
      expect(img.getAttribute("src")!.startsWith("data:image/svg+xml")).toBe(true);
      expect(tile.textContent!.trim().length).toBeGreaterThan(0);
    }
    expect(screen.getByRole("button", { name: /Ghost/ })).toBeTruthy();
  });

  it("renders the six state buttons in the reference order", () => {
    studio();
    const stateButtons = [...document.querySelectorAll(".openbot-studio__state")];
    expect(stateButtons).toHaveLength(6);
    expect(stateButtons.map((button) => button.textContent!.trim())).toEqual([
      "Idle",
      "Thinking",
      "Working",
      "Waiting",
      "Blocked",
      "Done",
    ]);
    expect(stateButtons.map((button) => button.getAttribute("aria-pressed"))).toEqual([
      "true",
      "false",
      "false",
      "false",
      "false",
      "false",
    ]);
  });

  it("uses native buttons for the state strip, so keyboard activation works", async () => {
    studio();
    for (const state of OPENBOT_STATES) {
      const button = screen.getByRole("button", {
        name: new RegExp(`^${state[0].toUpperCase()}${state.slice(1)}$`, "i"),
      });
      expect(button.tagName).toBe("BUTTON");
      expect(button.tabIndex).toBe(0);
      (button as HTMLButtonElement).focus();
      expect(document.activeElement).toBe(button);
    }
    const blocked = screen.getByRole("button", { name: "Blocked" });
    fireEvent.click(blocked);
    await tick();
    expect(blocked.getAttribute("aria-pressed")).toBe("true");
  });

  it("applies the chosen definition and colour, never the previewed state", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { onApply } = studio();
    fireEvent.click(screen.getByRole("button", { name: "Blocked" }));
    fireEvent.click(screen.getByRole("button", { name: /Ghost/ }));
    fireEvent.click(screen.getByRole("button", { name: "#1fbf68" }));
    fireEvent.click(screen.getByRole("button", { name: "Use character" }));

    expect(onApply).toHaveBeenCalledTimes(1);
    expect(onApply).toHaveBeenCalledWith(
      openBotUrl("ghost", "#1fbf68"),
      openBotStyleId("ghost"),
    );
    expect(onApply.mock.calls[0][0]).not.toContain("blocked");
    expect(onApply.mock.calls[0][0]).not.toContain("state");
  });

  it("starts a newly picked character on its curated upstream colour", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { onApply } = studio();
    fireEvent.click(screen.getByRole("button", { name: /Rocket/ }));
    fireEvent.click(screen.getByRole("button", { name: "Use character" }));
    expect(onApply).toHaveBeenCalledWith(
      openBotUrl("rocket", openBotCuratedColour("rocket")),
      openBotStyleId("rocket"),
    );
  });

  it("emits close without applying", () => {
    const { onApply, onClose } = studio();
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onApply).not.toHaveBeenCalled();
  });
});
