import { describe, it, expect } from "vitest";
import { dagLayers, dagLayout, dagEdges, DAG_NODE_W, type DagTask } from "./dag";

const t = (dependsOn: number[]): DagTask => ({ label: "x", botId: "b", dependsOn });

describe("dag layout", () => {
  it("layers independent tasks at 0 and dependents after their deps", () => {
    // 0 -> 1 -> 2, and 3 depends on 0
    const tasks = [t([]), t([0]), t([1]), t([0])];
    expect(dagLayers(tasks)).toEqual([0, 1, 2, 1]);
  });

  it("uses the longest dependency path", () => {
    // 2 depends on both 0 (layer 0) and 1 (layer 1) → 2 is layer 2
    const tasks = [t([]), t([0]), t([0, 1])];
    expect(dagLayers(tasks)).toEqual([0, 1, 2]);
  });

  it("terminates on cycles and out-of-range self edges", () => {
    const tasks = [t([1]), t([0])];
    const layers = dagLayers(tasks);
    expect(layers.length).toBe(2);
    layers.forEach((l) => expect(Number.isFinite(l)).toBe(true));
    expect(dagLayers([t([0, 9, -1])])).toEqual([0]);
  });

  it("assigns distinct rows within a column and sizes the canvas", () => {
    const tasks = [t([]), t([]), t([0, 1])];
    const layout = dagLayout(tasks);
    // two roots share layer 0 at different y, the child sits one column right
    expect(layout.positions[0].x).toBe(0);
    expect(layout.positions[1].x).toBe(0);
    expect(layout.positions[0].y).not.toBe(layout.positions[1].y);
    expect(layout.positions[2].x).toBe(DAG_NODE_W + 64);
    expect(layout.width).toBeGreaterThan(0);
    expect(layout.height).toBeGreaterThan(0);
  });

  it("dedupes and clamps edges", () => {
    const edges = dagEdges([t([1, 1, 5, -2, 0]), t([0])]);
    expect(edges).toEqual([
      { from: 1, to: 0 },
      { from: 0, to: 1 },
    ]);
  });

  it("handles an empty plan", () => {
    expect(dagLayout([])).toEqual({ positions: [], width: 0, height: 0 });
    expect(dagEdges([])).toEqual([]);
  });
});
