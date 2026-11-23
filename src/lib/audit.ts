/** Shapes returned by the global audit query. Tool arguments are never sent. */
export interface AuditLogEntry {
  id: string;
  bot_id: string;
  bot_name: string | null;
  run_id: string | null;
  thread_id: string | null;
  thread_title: string | null;
  kind: string;
  timestamp: string;
  tool_name: string | null;
  path: string | null;
  url: string | null;
  method: string | null;
  provider: string | null;
  model: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
  cost: number | null;
  size: number | null;
  permission: string | null;
  reason: string | null;
  memory_scope: string | null;
  memory_action: string | null;
  memory_id: string | null;
  category: string | null;
  office_id: string | null;
}

export interface AuditLogPage {
  entries: AuditLogEntry[];
  total: number;
  limit: number;
  offset: number;
}

/** Canonical event kinds, matching the Rust `AUDIT_EVENT_KINDS` list. */
export const AUDIT_KINDS = [
  "ModelCall",
  "ToolCall",
  "NetworkRequest",
  "FileRead",
  "FileWrite",
  "PermissionGranted",
  "PermissionDenied",
  "BotStarted",
  "BotPaused",
  "BotStopped",
  "MemoryChanged",
  "MemoryEmbeddingsUpdated",
] as const;
