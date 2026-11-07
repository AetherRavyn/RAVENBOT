import { describe, expect, it } from "vitest";
import { adjacentMessageIndex } from "./navigation";

describe("adjacentMessageIndex", () => {
  it("moves one row in each direction and clamps at the ends", () => {
    expect(adjacentMessageIndex(2, 5, 1)).toBe(3);
    expect(adjacentMessageIndex(2, 5, -1)).toBe(1);
    expect(adjacentMessageIndex(0, 5, -1)).toBe(0);
    expect(adjacentMessageIndex(4, 5, 1)).toBe(4);
  });

  it("starts from the first or last row when nothing is visible yet", () => {
    expect(adjacentMessageIndex(-1, 5, 1)).toBe(0);
    expect(adjacentMessageIndex(-1, 5, -1)).toBe(4);
  });

  it("returns no target for an empty transcript", () => {
    expect(adjacentMessageIndex(0, 0, 1)).toBe(-1);
    expect(adjacentMessageIndex(-1, 0, -1)).toBe(-1);
  });
});
