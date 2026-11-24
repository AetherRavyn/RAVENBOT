import { describe, expect, it } from "vitest";
import {
  describeSlice,
  estimateMessageHeight,
  prefixHeights,
  sliceRows,
  tailRows,
  type VirtualRowSize,
} from "./virtualRows";

function rows(count: number, height = 100): VirtualRowSize[] {
  return Array.from({ length: count }, (_, index) => ({
    key: `row-${index}`,
    height,
  }));
}

describe("prefixHeights", () => {
  it("puts the top of every row at the end of the previous one", () => {
    expect(prefixHeights(rows(3, 40))).toEqual([0, 40, 80, 120]);
  });

  it("never lets a zero or invalid height collapse a row", () => {
    expect(
      prefixHeights([
        { key: "a", height: 0 },
        { key: "b", height: Number.NaN },
        { key: "c", height: -20 },
      ]),
    ).toEqual([0, 1, 2, 3]);
  });
});

describe("sliceRows", () => {
  const sizes = rows(100, 100);

  it("mounts the viewport plus overscan, not the whole list", () => {
    const slice = sliceRows(sizes, 2_000, 500, 200);
    expect(slice.start).toBe(18);
    expect(slice.end).toBe(27);
    expect(slice.top).toBe(1_800);
    expect(slice.bottom).toBe(7_300);
    expect(slice.total).toBe(10_000);
  });

  it("covers a viewport that starts inside a tall row", () => {
    const tall = [
      { key: "short", height: 50 },
      { key: "tall", height: 2_000 },
      { key: "next", height: 50 },
    ];
    const slice = sliceRows(tall, 1_000, 300, 0);
    expect(slice.start).toBe(1);
    expect(slice.end).toBe(2);
    expect(slice.top).toBe(50);
    expect(slice.bottom).toBe(50);
  });

  it("mounts the tail when the viewport is past the last row", () => {
    const slice = sliceRows(sizes, 99_000, 500, 0);
    expect(slice.start).toBe(99);
    expect(slice.end).toBe(100);
    expect(slice.top).toBe(9_900);
    expect(slice.bottom).toBe(0);
  });

  it("handles an empty list and a zero-height viewport", () => {
    expect(sliceRows([], 0, 500)).toEqual({
      start: 0,
      end: 0,
      top: 0,
      bottom: 0,
      total: 0,
    });
    const slice = sliceRows(sizes, 1_000, 0, 0);
    expect(slice.start).toBe(10);
    expect(slice.end).toBe(11);
  });

  it("always mounts at least one row", () => {
    expect(sliceRows(rows(1, 10), 5_000, 500, 0)).toMatchObject({
      start: 0,
      end: 1,
    });
  });
});

describe("tailRows", () => {
  const sizes = rows(100, 100);

  it("mounts a bottom tail that fills the viewport", () => {
    const slice = tailRows(sizes, 500, 3, 200);
    expect(slice.start).toBe(93);
    expect(slice.end).toBe(100);
    expect(slice.top).toBe(9_300);
    expect(slice.bottom).toBe(0);
  });

  it("honours the minimum row count on a first frame with no height", () => {
    const slice = tailRows(sizes, 0, 8, 0);
    expect(slice.start).toBe(92);
    expect(slice.end).toBe(100);
  });

  it("mounts the whole short list", () => {
    expect(tailRows(rows(4, 100), 500, 8, 200)).toMatchObject({
      start: 0,
      end: 4,
      top: 0,
    });
  });
});

describe("describeSlice", () => {
  it("keeps spacer arithmetic in sync with the prefix", () => {
    expect(describeSlice(rows(10, 100), 3, 6)).toEqual({
      start: 3,
      end: 6,
      top: 300,
      bottom: 400,
      total: 1_000,
    });
  });

  it("clamps a range rather than returning a negative spacer", () => {
    expect(describeSlice(rows(5, 100), -2, 99)).toMatchObject({
      start: 0,
      end: 5,
      top: 0,
      bottom: 0,
    });
  });
});

describe("estimateMessageHeight", () => {
  it("grows with the number of lines and code blocks", () => {
    const short = estimateMessageHeight({ content: { text: "one line" } });
    const long = estimateMessageHeight({
      content: { text: "line\n".repeat(40) + "```ts\nconst a = 1;\n```" },
    });
    expect(long).toBeGreaterThan(short);
    expect(long).toBeLessThanOrEqual(3_200);
  });

  it("counts attachments and the leading gap", () => {
    const plain = estimateMessageHeight({ content: { text: "hello" } });
    const withImage = estimateMessageHeight({
      content: { text: "hello" },
      attachments: [{ data: "x", mime_type: "image/png" }],
    });
    expect(withImage).toBeGreaterThan(plain);
    expect(
      estimateMessageHeight({ content: { text: "hello" } }, 24),
    ).toBe(estimateMessageHeight({ content: { text: "hello" } }) + 24);
  });

  it("never returns a negative or tiny row", () => {
    expect(estimateMessageHeight(null)).toBeGreaterThanOrEqual(64);
    expect(estimateMessageHeight({ content: { text: "" } })).toBeGreaterThanOrEqual(64);
  });
});
