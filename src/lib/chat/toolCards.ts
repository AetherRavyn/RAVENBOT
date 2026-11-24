/**
 * Turn a persisted tool trace into the data a first-class card needs.
 *
 * The trace is deliberately small (results are clipped at 4k characters) and
 * has two historical shapes: `{success, output}` from the skill envelope, and
 * the bare output for older rows. A clipped result may also arrive as a string
 * because the core truncates the rendered JSON. These helpers accept all three
 * so a card never disappears just because a row is a little older or larger.
 */
import type { ToolTrace } from "$lib/types";

export type ToolCardKind = "command" | "diff" | "read" | "search";

export interface CommandCard {
  command: string;
  cwd: string;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  success: boolean;
}

export interface DiffFileCard {
  path: string;
  absolutePath: string;
  added: number;
  deleted: number;
  diff: string;
}

export interface DiffCard {
  files: DiffFileCard[];
  added: number;
  deleted: number;
  dryRun: boolean;
}

export interface ReadCard {
  path: string;
  content: string;
  offset: number;
  limit: number;
  returnedLines: number;
  totalLines: number;
  nextOffset: number | null;
  truncated: boolean;
}

export interface SearchCard {
  pattern: string;
  path: string;
  matches: string[];
  count: number;
  truncated: boolean;
  entries: boolean;
}

function asRecord(value: unknown): Record<string, any> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, any>)
    : null;
}

/** Unwrap the skill envelope when present. */
export function toolOutput(tool: ToolTrace): unknown {
  const result = asRecord(tool.result);
  return result && "output" in result ? result.output : tool.result;
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function numberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** The card family for a tool, or null for the generic trace row. */
export function toolCardKind(name: string): ToolCardKind | null {
  const id = name.trim().toLowerCase();
  if (id === "shell_exec") return "command";
  if (["file_edit", "apply_patch", "code_edit", "file_write"].includes(id)) {
    return "diff";
  }
  if (id === "file_read") return "read";
  if (["code_search", "file_tree"].includes(id)) return "search";
  return null;
}

/** True when the tool has a specialised card with something to show. */
export function hasToolCard(tool: ToolTrace): boolean {
  switch (toolCardKind(tool.name)) {
    case "command":
      return commandCard(tool) !== null;
    case "diff":
      return diffCard(tool) !== null;
    case "read":
      return readCard(tool) !== null;
    case "search":
      return searchCard(tool) !== null;
    default:
      return false;
  }
}

export function commandCard(tool: ToolTrace): CommandCard | null {
  if (toolCardKind(tool.name) !== "command") return null;
  const args = asRecord(tool.arguments) ?? {};
  const output = asRecord(toolOutput(tool));
  if (!output) {
    const raw = typeof toolOutput(tool) === "string" ? String(toolOutput(tool)) : "";
    const command = stringValue(args.command);
    if (!command && !raw) return null;
    return {
      command,
      cwd: stringValue(args.cwd),
      stdout: raw,
      stderr: "",
      exitCode: null,
      success: !tool.is_error,
    };
  }
  return {
    command: stringValue(output.command) || stringValue(args.command),
    cwd: stringValue(args.cwd) || stringValue(output.cwd),
    stdout: stringValue(output.stdout),
    stderr: stringValue(output.stderr) || stringValue(output.error),
    exitCode: numberValue(output.exit_code),
    success:
      typeof output.success === "boolean" ? output.success : !tool.is_error,
  };
}

export function readCard(tool: ToolTrace): ReadCard | null {
  if (toolCardKind(tool.name) !== "read") return null;
  const args = asRecord(tool.arguments) ?? {};
  const output = asRecord(toolOutput(tool));
  if (!output) {
    const content = typeof toolOutput(tool) === "string" ? String(toolOutput(tool)) : "";
    if (!content) return null;
    return {
      path: stringValue(args.path),
      content,
      offset: 1,
      limit: 0,
      returnedLines: content.split("\n").length,
      totalLines: content.split("\n").length,
      nextOffset: null,
      truncated: false,
    };
  }
  return {
    path: stringValue(output.path) || stringValue(args.path),
    content: stringValue(output.content),
    offset: numberValue(output.offset) ?? 1,
    limit: numberValue(output.limit) ?? 0,
    returnedLines: numberValue(output.returned_lines) ?? 0,
    totalLines: numberValue(output.total_lines) ?? 0,
    nextOffset: numberValue(output.next_offset),
    truncated: output.truncated === true,
  };
}

function diffFile(entry: unknown): DiffFileCard | null {
  const record = asRecord(entry);
  if (!record) return null;
  const diff = stringValue(record.diff);
  const path = stringValue(record.path);
  if (!path && !diff) return null;
  return {
    path,
    absolutePath: stringValue(record.absolute_path),
    added: numberValue(record.lines_added) ?? 0,
    deleted: numberValue(record.lines_deleted) ?? 0,
    diff,
  };
}

export function diffCard(tool: ToolTrace): DiffCard | null {
  if (toolCardKind(tool.name) !== "diff") return null;
  const output = asRecord(toolOutput(tool));
  const raw = toolOutput(tool);
  let entries: unknown = null;
  if (output) {
    entries = output.file_changes ?? output.files;
  }
  let files = Array.isArray(entries)
    ? entries.map(diffFile).filter((file): file is DiffFileCard => file !== null)
    : [];
  if (files.length === 0 && output) {
    const single = diffFile(output);
    if (single) files = [single];
  }
  if (files.length === 0 && typeof raw === "string" && /(^|\n)(@@|--- |\+\+\+ )/.test(raw)) {
    const args = asRecord(tool.arguments) ?? {};
    files = [
      {
        path: stringValue(args.path),
        absolutePath: "",
        added: (raw.match(/^\+/gm) ?? []).length,
        deleted: (raw.match(/^-/gm) ?? []).length,
        diff: raw,
      },
    ];
  }
  if (files.length === 0) return null;
  return {
    files,
    added: files.reduce((sum, file) => sum + file.added, 0),
    deleted: files.reduce((sum, file) => sum + file.deleted, 0),
    dryRun: output?.dry_run === true,
  };
}

export function searchCard(tool: ToolTrace): SearchCard | null {
  if (toolCardKind(tool.name) !== "search") return null;
  const args = asRecord(tool.arguments) ?? {};
  const output = asRecord(toolOutput(tool));
  if (!output) return null;
  const entries = Array.isArray(output.entries);
  const rawMatches = entries ? output.entries : output.matches;
  const matches = Array.isArray(rawMatches)
    ? rawMatches
        .map((match) => {
          if (typeof match === "string") return match;
          const record = asRecord(match);
          if (!record) return "";
          return stringValue(record.path) || JSON.stringify(record);
        })
        .filter(Boolean)
    : [];
  return {
    pattern: stringValue(output.pattern) || stringValue(args.pattern),
    path: stringValue(output.path) || stringValue(args.path),
    matches,
    count: numberValue(output.count) ?? matches.length,
    truncated: output.truncated === true,
    entries,
  };
}
