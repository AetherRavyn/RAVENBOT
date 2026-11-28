import { describe, expect, it, vi } from "vitest";
import {
  backdropLabel,
  backdropSource,
  builtinBackdrop,
  chooseBuiltinBackdrop,
  parseThreadBackdrop,
  parseThreadBackdropState,
  BUILTIN_BACKDROPS,
} from "./backdrop";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
  invoke: vi.fn(async () => ({ current: null, previous: null })),
}));

const invokeMock = vi.mocked(invoke);

describe("thread backdrop parsing", () => {
  it("bundles four distinct local images", () => {
    expect(BUILTIN_BACKDROPS).toHaveLength(4);
    expect(new Set(BUILTIN_BACKDROPS.map((item) => item.id)).size).toBe(4);
    for (const item of BUILTIN_BACKDROPS) {
      expect(item.path.startsWith("/backdrops/")).toBe(true);
      expect(item.path.endsWith(".svg")).toBe(true);
      expect(builtinBackdrop(item.id)).toBe(item);
    }
  });

  it("accepts a known built-in and rejects an unknown one", () => {
    expect(parseThreadBackdrop({ kind: "builtin", id: "aurora" })).toEqual({
      kind: "builtin",
      id: "aurora",
    });
    expect(parseThreadBackdrop({ kind: "builtin", id: "remote" })).toBeNull();
    expect(parseThreadBackdrop({ kind: "builtin" })).toBeNull();
  });

  it("accepts local image and video paths, rejecting empty or remote shapes", () => {
    expect(
      parseThreadBackdrop({ kind: "image", path: "/home/a/backdrops/x.png", name: "x.png" }),
    ).toEqual({ kind: "image", path: "/home/a/backdrops/x.png", name: "x.png" });
    expect(parseThreadBackdrop({ kind: "video", path: "/tmp/clip.webm" })).toEqual({
      kind: "video",
      path: "/tmp/clip.webm",
      name: null,
    });
    expect(parseThreadBackdrop({ kind: "image", path: "   " })).toBeNull();
    expect(parseThreadBackdrop({ kind: "iframe", path: "/tmp/x" })).toBeNull();
    expect(parseThreadBackdrop(null)).toBeNull();
  });

  it("parses the current/undo pair and drops malformed halves", () => {
    expect(
      parseThreadBackdropState({
        current: { kind: "builtin", id: "grid" },
        previous: { kind: "builtin", id: "unknown" },
      }),
    ).toEqual({ current: { kind: "builtin", id: "grid" }, previous: null });
    expect(parseThreadBackdropState("nonsense")).toEqual({ current: null, previous: null });
  });

  it("resolves local sources and labels without a network fallback", () => {
    expect(backdropSource({ kind: "builtin", id: "bloom" })).toBe("/backdrops/bloom.svg");
    expect(backdropSource({ kind: "image", path: "/home/a/x.png" })).toContain(
      "asset://localhost/",
    );
    expect(backdropSource(null)).toBeNull();
    expect(backdropLabel({ kind: "builtin", id: "dunes" })).toBe("Dunes");
    expect(backdropLabel({ kind: "video", path: "/x", name: "clip.mp4" })).toBe("clip.mp4");
  });

  it("sends thread ids and file paths to the backend commands", async () => {
    await chooseBuiltinBackdrop("thread-1", "aurora");
    expect(invokeMock).toHaveBeenCalledWith("set_thread_backdrop_builtin", {
      threadId: "thread-1",
      builtinId: "aurora",
    });
  });
});
