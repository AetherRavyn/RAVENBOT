/**
 * A short, readable form of a tool call's arguments, for a permission card.
 *
 * A JSON dump is accurate and unreadable. The cases below are the arguments a
 * person actually needs to decide with — the command, the path, the URL — and
 * the JSON is the fallback rather than the default.
 */
const SUMMARY_LIMIT = 280;

export function approvalSummary(args: unknown): string {
  try {
    if (args == null) return "";
    if (typeof args === "string") return clip(args);
    if (Array.isArray(args)) return clip(JSON.stringify(args));
    if (typeof args !== "object") return clip(String(args));
    const record = args as Record<string, unknown>;
    if (typeof record.command === "string") return clip(record.command);
    if (typeof record.path === "string" && typeof record.content === "string") {
      return clip(`${record.path} (+${record.content.length} chars)`);
    }
    for (const key of ["path", "instruction", "query", "url"]) {
      if (typeof record[key] === "string") return clip(record[key] as string);
    }
    return clip(JSON.stringify(record));
  } catch {
    return "";
  }
}

function clip(value: string): string {
  const trimmed = value.trim();
  return trimmed.length <= SUMMARY_LIMIT
    ? trimmed
    : `${trimmed.slice(0, SUMMARY_LIMIT - 1)}…`;
}
