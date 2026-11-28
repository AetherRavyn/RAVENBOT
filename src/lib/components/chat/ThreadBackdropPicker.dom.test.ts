// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";
import ThreadBackdropPicker from "./ThreadBackdropPicker.svelte";
import type { ThreadBackdrop, ThreadBackdropState } from "$lib/chat/backdrop";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const openMock = vi.mocked(openDialog);

const empty: ThreadBackdropState = { current: null, previous: null };

function renderPicker(initial: ThreadBackdropState = empty) {
  const onStateChange = vi.fn();
  const onClose = vi.fn();
  render(ThreadBackdropPicker, {
    props: { threadId: "thread-1", initial, onStateChange, onClose },
  });
  return { onStateChange, onClose };
}

describe("ThreadBackdropPicker", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    openMock.mockReset();
    invokeMock.mockResolvedValue(empty);
  });

  afterEach(cleanup);

  it("applies a built-in image and exposes it as pressed", async () => {
    invokeMock.mockResolvedValue({
      current: { kind: "builtin", id: "aurora" },
      previous: null,
    });
    const { onStateChange } = renderPicker();

    const aurora = screen.getByRole("button", { name: "Aurora" });
    await fireEvent.click(aurora);

    await waitFor(() =>
      expect(onStateChange).toHaveBeenCalledWith({
        current: { kind: "builtin", id: "aurora" },
        previous: null,
      }),
    );
    expect(invokeMock).toHaveBeenCalledWith("set_thread_backdrop_builtin", {
      threadId: "thread-1",
      builtinId: "aurora",
    });
    expect(aurora.getAttribute("aria-pressed")).toBe("true");
    expect(await a11yViolations(document.body)).toEqual([]);
  });

  it("imports a local image through the file dialog", async () => {
    openMock.mockResolvedValue("/tmp/photo.png");
    invokeMock.mockResolvedValue({
      current: {
        kind: "image",
        path: "/home/me/RAVENBOT/backdrops/123.png",
        name: "photo.png",
      },
      previous: null,
    });
    const { onStateChange } = renderPicker();

    await fireEvent.click(screen.getByRole("button", { name: "Upload image" }));

    await waitFor(() => expect(onStateChange).toHaveBeenCalled());
    expect(openMock).toHaveBeenCalledWith(
      expect.objectContaining({
        multiple: false,
        directory: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
      }),
    );
    expect(invokeMock).toHaveBeenCalledWith("import_thread_backdrop", {
      threadId: "thread-1",
      sourcePath: "/tmp/photo.png",
    });
    expect(onStateChange.mock.calls[0][0].current.kind).toBe("image");
  });

  it("clears the current backdrop and can undo it", async () => {
    const current: ThreadBackdrop = { kind: "builtin", id: "grid" };
    const previous: ThreadBackdrop = {
      kind: "image",
      path: "/home/me/RAVENBOT/backdrops/a.png",
      name: "a.png",
    };
    invokeMock
      .mockResolvedValueOnce({ current: null, previous: current })
      .mockResolvedValueOnce({ current: current, previous: null });
    const { onStateChange } = renderPicker({ current, previous });

    await fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    await waitFor(() => expect(onStateChange).toHaveBeenCalledTimes(1));
    expect(invokeMock).toHaveBeenCalledWith("clear_thread_backdrop", {
      threadId: "thread-1",
    });

    await fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(onStateChange).toHaveBeenCalledTimes(2));
    expect(invokeMock).toHaveBeenCalledWith("undo_thread_backdrop", {
      threadId: "thread-1",
    });
  });

  it("shows a backend refusal without losing the dialog", async () => {
    invokeMock.mockRejectedValue(new Error("backdrops must be 100 MB or smaller"));
    const { onStateChange, onClose } = renderPicker();

    await fireEvent.click(screen.getByRole("button", { name: "Aurora" }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("100 MB"),
    );
    expect(onStateChange).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes from the footer", async () => {
    const { onClose } = renderPicker();
    const buttons = screen.getAllByRole("button", { name: "Close" });
    await fireEvent.click(buttons[buttons.length - 1]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
