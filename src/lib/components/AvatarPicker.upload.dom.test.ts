// @vitest-environment jsdom
/**
 * The picker's local-image contract.
 *
 * The image is a local identity, not a remote URL: it must be validated by its
 * bytes, kept under the 512 KB cap, replaceable, removable, and still committed
 * through the normal "Use Selected Avatar" action.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import AvatarPicker from "./AvatarPicker.svelte";
import { DEFAULT_AVATAR_STYLE } from "$lib/diceStyles";

afterEach(cleanup);

const PNG_BYTES = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
  0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
]);

function picker() {
  const onSelect = vi.fn();
  const view = render(AvatarPicker, {
    props: { seed: "Ada", style: DEFAULT_AVATAR_STYLE, customUrl: null, onSelect },
  });
  const input = view.container.querySelector('input[type="file"]') as HTMLInputElement;
  return { ...view, onSelect, input };
}

async function choose(input: HTMLInputElement, file: File) {
  Object.defineProperty(input, "files", { configurable: true, value: [file] });
  await fireEvent.change(input);
}

function imageDataUrl(): string {
  return (document.querySelector("#custom-url") as HTMLInputElement).value;
}

describe("AvatarPicker custom image upload", () => {
  it("accepts a local PNG and commits it as a custom avatar", async () => {
    const { onSelect, input } = picker();
    await choose(input, new File([PNG_BYTES], "me.png", { type: "image/png" }));

    await waitFor(() => expect(imageDataUrl()).toMatch(/^data:image\/png;base64,/));
    fireEvent.click(screen.getByRole("button", { name: /Use Selected Avatar/i }));

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect.mock.calls[0][1]).toBe("custom");
    expect(onSelect.mock.calls[0][0]).toMatch(/^data:image\/png;base64,/);
  });

  it("rejects a file whose bytes are not a supported image", async () => {
    const { onSelect, input } = picker();
    await choose(
      input,
      new File([new TextEncoder().encode("<svg>not an image</svg>")], "fake.png", {
        type: "image/png",
      }),
    );

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Choose a PNG, JPEG, WebP or GIF image.",
    );
    expect(imageDataUrl()).toBe("");
    fireEvent.click(screen.getByRole("button", { name: /Use Selected Avatar/i }));
    expect(onSelect).not.toHaveBeenCalledWith(
      expect.stringMatching(/^data:/),
      "custom",
    );
  });

  it("rejects an image larger than 512 KB before reading it", async () => {
    const { onSelect, input } = picker();
    const oversized = new File([new Uint8Array(512 * 1024 + 1)], "huge.png", {
      type: "image/png",
    });
    await choose(input, oversized);

    expect((await screen.findByRole("alert")).textContent).toContain(
      "That image is larger than 512 KB.",
    );
    expect(imageDataUrl()).toBe("");
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("removes an uploaded image and falls back to a generated style", async () => {
    const { onSelect, input } = picker();
    await choose(input, new File([PNG_BYTES], "me.png", { type: "image/png" }));
    await waitFor(() => expect(imageDataUrl()).toMatch(/^data:image\/png;base64,/));

    fireEvent.click(screen.getByRole("button", { name: /Remove image/i }));
    await waitFor(() => expect(imageDataUrl()).toBe(""));
    fireEvent.click(screen.getByRole("button", { name: /Use Selected Avatar/i }));

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect.mock.calls[0][1]).toBe(DEFAULT_AVATAR_STYLE);
    expect(onSelect.mock.calls[0][0]).toContain("dicebear.com");
  });
});
