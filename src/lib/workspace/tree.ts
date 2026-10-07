/**
 * The workspace file tree, shared by the browser and the projects IDE.
 *
 * Extracted because there were going to be two of them. `browse_workspace`
 * returns a flat list with relative paths — cheap to build, cheap to keep
 * sorted — and the hierarchy is reconstructed by whoever renders it. Doing that
 * twice is two depth caps, two orderings, and two subtly different answers to
 * "is this folder empty".
 */

export interface Entry {
  /** Path relative to the listed root, so the UI never does path arithmetic. */
  path: string;
  name: string;
  isDir: boolean;
  isLink: boolean;
  sizeBytes: number;
  /** `Some` when the file is small enough to preview inline. */
  preview: string | null;
  /** Never render this as text, whatever its extension claims. */
  isBinary: boolean;
}

export interface Tree {
  root: string;
  entries: Entry[];
  /** The root does not exist — distinct from an empty root. */
  missing: boolean;
  truncated: boolean;
  note: string | null;
  fileCount: number;
  dirCount: number;
  totalBytes: number;
}

export interface TreeNode {
  entry: Entry;
  depth: number;
  children: TreeNode[];
}

/**
 * A tree from a flat list.
 *
 * Two passes, because a single pass would make the result depend on the order
 * the backend happened to sort in. Every node exists before any is linked, so a
 * child listed before its parent still finds it, and a parent listed after its
 * children does not replace the node they attached to.
 */
export function buildTree(entries: Entry[]): TreeNode[] {
  const roots: TreeNode[] = [];
  const byPath = new Map<string, TreeNode>();

  for (const entry of entries) {
    const depth = entry.path.split("/").length - 1;
    byPath.set(entry.path, { entry, depth, children: [] });
  }

  /**
   * The folder for `path`, created if the listing did not include it.
   *
   * A directory the backend capped out of, but whose children came through.
   * Without this those children render at the top level with no folder above
   * them, which reads as "this file is in the workspace root" — the most
   * misleading thing a file tree can say, and it is exactly what happens on a
   * large codebase, which is the case this screen exists for.
   *
   * Recursive rather than one level: a cap that bites twice leaves two missing
   * ancestors, and synthesising only the nearer one parks a folder called
   * `src/lib` at the top level — still wrong, just less obviously so.
   */
  const ensureAncestor = (path: string): TreeNode => {
    const existing = byPath.get(path);
    if (existing) return existing;
    const parts = path.split("/");
    const made: TreeNode = {
      entry: {
        path,
        name: parts[parts.length - 1],
        isDir: true,
        isLink: false,
        sizeBytes: 0,
        preview: null,
        isBinary: false,
      },
      depth: parts.length - 1,
      children: [],
    };
    byPath.set(path, made);
    const parentPath = parts.slice(0, -1).join("/");
    if (parentPath) ensureAncestor(parentPath).children.push(made);
    else roots.push(made);
    return made;
  };

  for (const entry of entries) {
    const node = byPath.get(entry.path);
    if (!node) continue;
    const parentPath = entry.path.split("/").slice(0, -1).join("/");
    if (!parentPath) {
      roots.push(node);
      continue;
    }
    ensureAncestor(parentPath).children.push(node);
  }

  const order = (list: TreeNode[]) => {
    // Folders first, then alphabetical. Folders first because a codebase read
    // top-down is read as `src/`, `tests/`, `README` — interleaving them makes
    // every file look like it is at the same level as the folder above it.
    list.sort(
      (a, b) =>
        Number(b.entry.isDir) - Number(a.entry.isDir) ||
        a.entry.name.toLowerCase().localeCompare(b.entry.name.toLowerCase()),
    );
    for (const n of list) order(n.children);
  };
  order(roots);
  return roots;
}

/** Flatten a tree depth-first, for keyboard navigation over what is visible. */
export function flattenTree(nodes: TreeNode[], open: (path: string) => boolean): TreeNode[] {
  const out: TreeNode[] = [];
  const walk = (list: TreeNode[]) => {
    for (const n of list) {
      out.push(n);
      if (n.entry.isDir && open(n.entry.path)) walk(n.children);
    }
  };
  walk(nodes);
  return out;
}

/**
 * A byte count a person would write down.
 *
 * The backend sends raw bytes because a preformatted string per row is a string
 * the UI cannot sort or compare.
 */
export function fmtBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}
