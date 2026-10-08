// Pure DAG layout for the office Plan Mode graph: longest-path layering with a
// cycle guard. Coordinates are derived from the constants so the renderer needs
// no measurement pass.
export const DAG_NODE_W = 190;
export const DAG_NODE_H = 46;
export const DAG_COL_GAP = 64;
export const DAG_ROW_GAP = 14;

export interface DagTask {
  label: string;
  botId: string;
  dependsOn: number[];
}

/**
 * Layer index per task (0 = no deps). Cycles degrade to the task's own index
 * layer so the layout always terminates.
 */
export function dagLayers(tasks: DagTask[]): number[] {
  const layers = new Array<number>(tasks.length).fill(0);
  const computing = new Set<number>();

  function resolve(i: number): number {
    if (layers[i] > 0 || computing.has(i)) return layers[i];
    computing.add(i);
    let layer = 0;
    for (const d of tasks[i]?.dependsOn ?? []) {
      if (d === i || d < 0 || d >= tasks.length) continue;
      layer = Math.max(layer, resolve(d) + 1);
    }
    computing.delete(i);
    layers[i] = layer;
    return layer;
  }

  for (let i = 0; i < tasks.length; i++) resolve(i);
  return layers;
}

export interface DagLayout {
  /** x,y of each task's node box (top-left), indexed like tasks. */
  positions: { x: number; y: number }[];
  width: number;
  height: number;
}

export function dagLayout(tasks: DagTask[]): DagLayout {
  if (tasks.length === 0) return { positions: [], width: 0, height: 0 };
  const layers = dagLayers(tasks);
  const columns = new Map<number, number[]>();
  tasks.forEach((_, i) => {
    const l = layers[i];
    (columns.get(l) ?? columns.set(l, []).get(l)!).push(i);
  });

  const colKeys = [...columns.keys()].sort((a, b) => a - b);
  const rows = Math.max(1, ...colKeys.map((k) => columns.get(k)!.length));
  const positions = new Array<{ x: number; y: number }>(tasks.length);

  for (const k of colKeys) {
    const idxs = columns.get(k)!;
    const stackHeight = idxs.length * DAG_NODE_H + (idxs.length - 1) * DAG_ROW_GAP;
    const offset = (rows * (DAG_NODE_H + DAG_ROW_GAP) - DAG_ROW_GAP - stackHeight) / 2;
    idxs.forEach((taskIdx, row) => {
      positions[taskIdx] = {
        x: k * (DAG_NODE_W + DAG_COL_GAP),
        y: offset + row * (DAG_NODE_H + DAG_ROW_GAP),
      };
    });
  }

  const cols = colKeys.length ? Math.max(...colKeys) + 1 : 0;
  return {
    positions,
    width: cols ? cols * DAG_NODE_W + (cols - 1) * DAG_COL_GAP : 0,
    height: rows ? rows * DAG_NODE_H + (rows - 1) * DAG_ROW_GAP : 0,
  };
}

/** Edges as (from, to) index pairs, deduped and clamped to the task list. */
export function dagEdges(tasks: DagTask[]): { from: number; to: number }[] {
  const seen = new Set<string>();
  const edges: { from: number; to: number }[] = [];
  tasks.forEach((t, to) => {
    for (const from of t.dependsOn) {
      if (from === to || from < 0 || from >= tasks.length) continue;
      const key = `${from}->${to}`;
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push({ from, to });
    }
  });
  return edges;
}
