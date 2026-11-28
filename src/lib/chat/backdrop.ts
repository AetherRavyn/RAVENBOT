/**
 * Conversation backdrops.
 *
 * A backdrop is a presentation choice, not content. It can be one of four
 * bundled images or a local file copied into `~/RAVENBOT/backdrops/`; a stored
 * setting can never name a remote URL. The parser below is the boundary the
 * rest of the UI trusts, because the value comes back from SQLite and may have
 * been written by an older build.
 */
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type BackdropKind = "builtin" | "image" | "video";

export interface ThreadBackdrop {
  kind: BackdropKind;
  /** Present for `builtin`. */
  id?: string | null;
  /** Absolute local path for `image`/`video`. */
  path?: string | null;
  /** Original file name, for the picker's label. */
  name?: string | null;
}

export interface ThreadBackdropState {
  current: ThreadBackdrop | null;
  previous: ThreadBackdrop | null;
}

export interface BuiltinBackdrop {
  id: string;
  name: string;
  path: string;
  /** Which theme tone it was designed against; shown as a hint. */
  tone: "dark" | "light";
}

export const BUILTIN_BACKDROPS: readonly BuiltinBackdrop[] = Object.freeze([
  { id: "aurora", name: "Aurora", path: "/backdrops/aurora.svg", tone: "dark" },
  { id: "dunes", name: "Dunes", path: "/backdrops/dunes.svg", tone: "dark" },
  { id: "grid", name: "Grid", path: "/backdrops/grid.svg", tone: "dark" },
  { id: "bloom", name: "Bloom", path: "/backdrops/bloom.svg", tone: "dark" },
]);

const BUILTIN_BY_ID = new Map(BUILTIN_BACKDROPS.map((item) => [item.id, item]));

export function builtinBackdrop(id: string | null | undefined): BuiltinBackdrop | null {
  if (!id) return null;
  return BUILTIN_BY_ID.get(id.trim().toLowerCase()) ?? null;
}

/** Parse one value from the backend, rejecting anything the UI cannot render. */
export function parseThreadBackdrop(value: unknown): ThreadBackdrop | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Record<string, unknown>;
  const kind = record.kind;
  if (kind === "builtin") {
    const id = typeof record.id === "string" ? record.id : "";
    return builtinBackdrop(id) ? { kind, id } : null;
  }
  if (kind === "image" || kind === "video") {
    const path = typeof record.path === "string" ? record.path.trim() : "";
    if (!path) return null;
    const name = typeof record.name === "string" ? record.name.trim() : "";
    return { kind, path, name: name || null };
  }
  return null;
}

/** Parse the current/undo pair; malformed entries count as absent. */
export function parseThreadBackdropState(value: unknown): ThreadBackdropState {
  if (!value || typeof value !== "object") return { current: null, previous: null };
  const record = value as Record<string, unknown>;
  return {
    current: parseThreadBackdrop(record.current),
    previous: parseThreadBackdrop(record.previous),
  };
}

/**
 * The URL to put in `src`/`background-image`.
 *
 * A bundled image is a static path. An uploaded file lives outside the bundle,
 * so the native shell's asset protocol resolves it; in a browser preview or a
 * test there is no protocol, and returning the path is the honest fallback.
 */
export function backdropSource(backdrop: ThreadBackdrop | null): string | null {
  if (!backdrop) return null;
  if (backdrop.kind === "builtin") return builtinBackdrop(backdrop.id)?.path ?? null;
  if (!backdrop.path) return null;
  try {
    return convertFileSrc(backdrop.path);
  } catch {
    return backdrop.path;
  }
}

/** A human label for the current backdrop. */
export function backdropLabel(backdrop: ThreadBackdrop | null): string {
  if (!backdrop) return "";
  if (backdrop.kind === "builtin") return builtinBackdrop(backdrop.id)?.name ?? "";
  return backdrop.name?.trim() || (backdrop.kind === "video" ? "Video" : "Image");
}

/** The file-dialog filters for the two upload buttons. */
export const BACKDROP_IMAGE_FILTER = {
  name: "Images",
  extensions: ["png", "jpg", "jpeg", "webp"],
};
export const BACKDROP_VIDEO_FILTER = {
  name: "Videos",
  extensions: ["mp4", "webm"],
};

async function backdropCommand(
  command: string,
  threadId: string,
  extra: Record<string, unknown> = {},
): Promise<ThreadBackdropState> {
  const raw = await invoke(command, { threadId, ...extra });
  return parseThreadBackdropState(raw);
}

export function loadThreadBackdrop(threadId: string): Promise<ThreadBackdropState> {
  return backdropCommand("get_thread_backdrop", threadId);
}

export function chooseBuiltinBackdrop(
  threadId: string,
  builtinId: string,
): Promise<ThreadBackdropState> {
  return backdropCommand("set_thread_backdrop_builtin", threadId, { builtinId });
}

export function importThreadBackdrop(
  threadId: string,
  sourcePath: string,
): Promise<ThreadBackdropState> {
  return backdropCommand("import_thread_backdrop", threadId, { sourcePath });
}

export function clearThreadBackdrop(threadId: string): Promise<ThreadBackdropState> {
  return backdropCommand("clear_thread_backdrop", threadId);
}

export function undoThreadBackdrop(threadId: string): Promise<ThreadBackdropState> {
  return backdropCommand("undo_thread_backdrop", threadId);
}
