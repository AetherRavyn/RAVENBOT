// @vitest-environment jsdom
/**
 * The picker's OpenBot contract.
 *
 * The picker is the only place an agent's look is committed, so this file
 * pins the two things that would silently break the feature: an OpenBot URL
 * must not be mistaken for a custom image, and apply must emit both the URL
 * and the `openbot:` style.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import AvatarPicker from "./AvatarPicker.svelte";
import { openBotCuratedColour, openBotStyleId, openBotUrl } from "$lib/openBotAvatar";

afterEach(cleanup);

describe("AvatarPicker — OpenBot", () => {
  it("opens the OpenBot studio and commits the character, colour and style", async () => {
    const onSelect = vi.fn();
    const { container } = render(AvatarPicker, {
      props: { seed: "Ada", style: "clay", customUrl: null, onSelect },
    });

    const tile = container.querySelector('section[aria-label="OpenBot character"] button')!;
    expect(tile).not.toBeNull();
    fireEvent.click(tile);
    await tick();
    expect(container.querySelector(".openbot-studio")).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /Ghost/ }));
    fireEvent.click(screen.getByRole("button", { name: "Use character" }));

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(
      openBotUrl("ghost", openBotCuratedColour("ghost")),
      openBotStyleId("ghost"),
    );
  });

  it("previews a stored OpenBot URL through the avatar, not as a custom image", () => {
    const onSelect = vi.fn();
    const { container } = render(AvatarPicker, {
      props: {
        seed: "Ada",
        style: "openbot:ghost",
        customUrl: "openbot://ghost?color=%23f4f4f4",
        onSelect,
      },
    });
    const hero = container.querySelector(".size-20 .raven-avatar")!;
    expect(hero.getAttribute("data-source")).toBe("openbot");
    expect(container.querySelector(".size-20 img[alt='Avatar preview']")).toBeNull();
  });

  it("confirms a stored OpenBot character as OpenBot, never as a DiceBear URL", () => {
    const onSelect = vi.fn();
    render(AvatarPicker, {
      props: {
        seed: "Ada",
        style: "openbot:ghost",
        customUrl: "openbot://ghost?color=%23f4f4f4",
        onSelect,
      },
    });
    fireEvent.click(screen.getByRole("button", { name: /Use Selected Avatar/i }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    const [url, styleValue] = onSelect.mock.calls[0];
    expect(url).toBe(openBotUrl("ghost", "#f4f4f4"));
    expect(styleValue).toBe("openbot:ghost");
    expect(url).not.toContain("dicebear.com");
  });

  it("leaves OpenBot mode when a DiceBear style is picked afterwards", () => {
    const onSelect = vi.fn();
    render(AvatarPicker, {
      props: {
        seed: "Ada",
        style: "openbot:ghost",
        customUrl: "openbot://ghost?color=%23f4f4f4",
        onSelect,
      },
    });
    fireEvent.click(screen.getByRole("radio", { name: /Clay/i }));
    fireEvent.click(screen.getByRole("button", { name: /Use Selected Avatar/i }));
    const [url, styleValue] = onSelect.mock.calls.at(-1)!;
    expect(styleValue).toBe("clay");
    expect(url).toContain("dicebear.com");
  });

  it("shows a typed custom image over an old OpenBot URL", async () => {
    const onSelect = vi.fn();
    const { container } = render(AvatarPicker, {
      props: {
        seed: "Ada",
        style: "openbot:ghost",
        customUrl: "openbot://ghost?color=%23f4f4f4",
        onSelect,
      },
    });
    const input = container.querySelector("#custom-url") as HTMLInputElement;
    input.value = "https://example.com/me.png";
    fireEvent.input(input);
    await tick();
    const hero = container.querySelector(".size-20 .raven-avatar")!;
    expect(hero.getAttribute("data-source")).toBe("custom");
    expect(container.querySelector(".size-20 img")!.getAttribute("src")).toBe(
      "https://example.com/me.png",
    );
  });
});
