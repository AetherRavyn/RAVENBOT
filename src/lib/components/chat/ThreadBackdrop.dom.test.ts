// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import ThreadBackdrop from "./ThreadBackdrop.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
  invoke: vi.fn(),
}));

afterEach(cleanup);

describe("ThreadBackdrop", () => {
  it("renders a built-in image behind a theme-aware, non-interactive scrim", () => {
    const { container } = render(ThreadBackdrop, {
      props: { backdrop: { kind: "builtin", id: "aurora" } },
    });
    const layer = container.querySelector("[data-thread-backdrop]") as HTMLElement;
    expect(layer).toBeTruthy();
    expect(layer.className).toContain("pointer-events-none");
    expect(layer.getAttribute("aria-hidden")).toBe("true");
    expect(layer.querySelector("img")?.getAttribute("src")).toBe("/backdrops/aurora.svg");
    expect(layer.querySelector("video")).toBeNull();
    const scrim = layer.querySelector("[data-backdrop-scrim]") as HTMLElement;
    expect(scrim.style.background).toContain("color-mix");
    expect(scrim.style.background).toContain("var(--surface-0)");
  });

  it("renders an uploaded video muted, looping and without controls", () => {
    const { container } = render(ThreadBackdrop, {
      props: { backdrop: { kind: "video", path: "/home/a/backdrops/clip.webm" } },
    });
    const video = container.querySelector("video") as HTMLVideoElement;
    expect(video).toBeTruthy();
    expect(video.muted).toBe(true);
    expect(video.loop).toBe(true);
    expect(video.autoplay).toBe(true);
    expect(video.hasAttribute("controls")).toBe(false);
    expect(video.getAttribute("src")).toContain("asset://localhost/");
  });

  it("renders nothing when there is no backdrop", () => {
    const { container } = render(ThreadBackdrop, { props: { backdrop: null } });
    expect(container.querySelector("[data-thread-backdrop]")).toBeNull();
  });
});
