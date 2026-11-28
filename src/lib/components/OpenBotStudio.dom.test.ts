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

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

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

  it("applies the chosen definition, colour and render options, never the previewed state", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { onApply } = studio();
    fireEvent.click(screen.getByRole("button", { name: "Blocked" }));
    fireEvent.click(screen.getByRole("button", { name: /Ghost/ }));
    fireEvent.click(screen.getByRole("button", { name: "#1fbf68" }));
    fireEvent.click(screen.getByRole("button", { name: "Use character" }));

    expect(onApply).toHaveBeenCalledTimes(1);
    expect(onApply).toHaveBeenCalledWith(
      openBotUrl("ghost", {
        colour: "#1fbf68",
        depth: 6,
        followPointer: false,
        finish: "flat",
      }),
      openBotStyleId("ghost"),
    );
    const url = onApply.mock.calls[0][0];
    expect(url).toContain("depth=6");
    expect(url).toContain("follow=0");
    expect(url).toContain("finish=flat");
    expect(url).not.toContain("blocked");
    expect(url).not.toContain("state");
  });

  it("reopens with the saved depth, pointer-follow and finish", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { onApply } = studio({
      imageUrl: "openbot://cat?color=%23f8a01c&depth=12&follow=1&finish=fluffy",
    });
    const range = document.querySelector("#openbot-volume") as HTMLInputElement;
    expect(range.value).toBe("12");
    expect(
      (screen.getByRole("checkbox", { name: /Follow the pointer/i }) as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(screen.getByRole("button", { name: "Fluffy" }).getAttribute("aria-pressed")).toBe(
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "Use character" }));
    expect(onApply).toHaveBeenCalledWith(
      "openbot://cat?color=%23f8a01c&depth=12&follow=1&finish=fluffy",
      openBotStyleId("cat"),
    );
  });

  it("starts a newly picked character on its curated upstream colour", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { onApply } = studio();
    fireEvent.click(screen.getByRole("button", { name: /Rocket/ }));
    fireEvent.click(screen.getByRole("button", { name: "Use character" }));
    expect(onApply).toHaveBeenCalledWith(
      openBotUrl("rocket", {
        colour: openBotCuratedColour("rocket"),
        depth: 6,
        followPointer: false,
        finish: "flat",
      }),
      openBotStyleId("rocket"),
    );
  });

  it("surprises a new character and colour without committing or losing render choices", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(Math, "random").mockReturnValue(0);
    const { onApply } = studio();
    const hero = () => document.querySelector(".openbot-studio__hero") as HTMLElement;

    // Choose the options Surprise me must preserve.
    const range = document.querySelector("#openbot-volume") as HTMLInputElement;
    await fireEvent.input(range, { target: { value: "12" } });
    await fireEvent.click(screen.getByRole("checkbox", { name: /Follow the pointer/i }));
    await fireEvent.click(screen.getByRole("button", { name: "Fluffy" }));
    expect(hero().dataset.heroId).toBe("cat");

    await fireEvent.click(screen.getByRole("button", { name: "Surprise me" }));
    await tick();
    const first = hero().dataset.heroId!;
    expect(first).not.toBe("cat");
    expect(hero().dataset.heroColour).toBe(openBotCuratedColour(first));
    expect(onApply).not.toHaveBeenCalled();

    // A second click must move again, not settle on the same candidate.
    await fireEvent.click(screen.getByRole("button", { name: "Surprise me" }));
    await tick();
    const second = hero().dataset.heroId!;
    expect(second).not.toBe(first);
    expect(second).not.toBe("cat");
    expect(hero().dataset.heroColour).toBe(openBotCuratedColour(second));
    expect(onApply).not.toHaveBeenCalled();

    expect((document.querySelector("#openbot-volume") as HTMLInputElement).value).toBe("12");
    expect(
      (screen.getByRole("checkbox", { name: /Follow the pointer/i }) as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(screen.getByRole("button", { name: "Fluffy" }).getAttribute("aria-pressed")).toBe(
      "true",
    );

    await fireEvent.click(screen.getByRole("button", { name: "Use character" }));
    expect(onApply).toHaveBeenCalledWith(
      openBotUrl(second, {
        colour: openBotCuratedColour(second),
        depth: 12,
        followPointer: true,
        finish: "fluffy",
      }),
      openBotStyleId(second),
    );
  });

  it("emits close without applying", () => {
    const { onApply, onClose } = studio();
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onApply).not.toHaveBeenCalled();
  });
});
