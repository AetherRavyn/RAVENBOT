/**
 * Safe decoders for the Tauri IPC boundary.
 *
 * `invoke<T>` is a lie the type system cannot check: the Rust side can add,
 * rename, or omit a field and the frontend keeps compiling until something
 * reads a missing property. These decoders are the cheap half of that contract
 * — they are not strict validators. They accept whatever arrived, keep every
 * field they recognise, and fall back to a safe value for everything else so a
 * malformed row cannot throw while a list is rendering.
 *
 * Row identity is the one thing treated as non-negotiable: a thread, message,
 * or attention item with no usable `id` is dropped, because every consumer
 * keys on it and an unusable entry would poison lists and notifications.
 */
import type {
  Attachment,
  AttentionItem,
  AttentionKind,
  ChecklistItem,
  ChecklistStatus,
  Message,
  MessageContent,
  MessageRole,
  Thread,
  ThreadKind,
  ToolTrace,
  TraceSpan,
} from "$lib/types";

type JsonRecord = Record<string, unknown>;

const THREAD_KINDS: ReadonlySet<string> = new Set<ThreadKind>([
  "chat",
  "office",
  "delegation",
  "routine",
  "node",
  "channel",
  "goal",
]);

const MESSAGE_ROLES: ReadonlySet<string> = new Set<MessageRole>([
  "user",
  "assistant",
  "system",
  "tool",
]);

const CHECKLIST_STATUSES: ReadonlySet<string> = new Set<ChecklistStatus>([
  "Pending",
  "InProgress",
  "Completed",
  "Failed",
  "Skipped",
]);

const ATTENTION_KINDS: ReadonlySet<string> = new Set<AttentionKind>([
  "approval",
  "question",
  "queue",
  "alert",
]);

function asRecord(value: unknown): JsonRecord | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  return value as JsonRecord;
}

function text(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function nullableText(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function boolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function number(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function nullableNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function records(value: unknown): JsonRecord[] {
  if (!Array.isArray(value)) return [];
  const rows: JsonRecord[] = [];
  for (const entry of value) {
    const row = asRecord(entry);
    if (row) rows.push(row);
  }
  return rows;
}

function strings(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is string => typeof entry === "string");
}

// ── Threads ─────────────────────────────────────────────────────────────────

function threadKind(value: unknown): ThreadKind {
  return typeof value === "string" && THREAD_KINDS.has(value)
    ? (value as ThreadKind)
    : "chat";
}

function decodeThreadRow(value: unknown): Thread | null {
  const row = asRecord(value);
  if (!row) return null;
  const id = text(row.id).trim();
  if (!id) return null;

  return {
    id,
    bot_id: text(row.bot_id),
    title: text(row.title),
    is_active: boolean(row.is_active, true),
    ephemeral: boolean(row.ephemeral, false),
    channel_id: nullableText(row.channel_id),
    kind: threadKind(row.kind),
    visible: boolean(row.visible, true),
    created_at: text(row.created_at),
    updated_at: text(row.updated_at),
  };
}

/** Decode `list_threads` / thread event payloads. Never throws; bad rows drop. */
export function decodeThreads(value: unknown): Thread[] {
  if (!Array.isArray(value)) return [];
  const threads: Thread[] = [];
  for (const entry of value) {
    const thread = decodeThreadRow(entry);
    if (thread) threads.push(thread);
  }
  return threads;
}

// ── Messages ────────────────────────────────────────────────────────────────

function messageRole(value: unknown): MessageRole {
  const role = typeof value === "string" ? value.toLowerCase() : "";
  return MESSAGE_ROLES.has(role) ? (role as MessageRole) : "assistant";
}

function decodeAttachments(value: unknown): Attachment[] {
  const attachments: Attachment[] = [];
  for (const row of records(value)) {
    const id = text(row.id).trim();
    if (!id) continue;

    const attachment: Attachment = {
      id,
      name: text(row.name),
      mime_type: text(row.mime_type, text(row.mimeType)),
      size: number(row.size, 0),
      path: text(row.path),
    };
    if (typeof row.data === "string") attachment.data = row.data;
    if (typeof row.is_image === "boolean") attachment.is_image = row.is_image;
    attachments.push(attachment);
  }
  return attachments;
}

function decodeToolTraces(value: unknown): ToolTrace[] {
  const traces: ToolTrace[] = [];
  for (const row of records(value)) {
    const name = text(row.name).trim();
    if (!name) continue;

    const trace: ToolTrace = { name };
    if (row.arguments !== undefined) trace.arguments = row.arguments;
    if (row.result !== undefined) trace.result = row.result;
    if (typeof row.is_error === "boolean") trace.is_error = row.is_error;
    if (typeof row.duration_ms === "number" && Number.isFinite(row.duration_ms)) {
      trace.duration_ms = row.duration_ms;
    }
    if (typeof row.cost === "number" && Number.isFinite(row.cost)) {
      trace.cost = row.cost;
    }
    traces.push(trace);
  }
  return traces;
}

function decodeSpans(value: unknown): TraceSpan[] {
  const spans: TraceSpan[] = [];
  for (const row of records(value)) {
    const id = text(row.id).trim();
    if (!id) continue;

    const span: TraceSpan = {
      id,
      kind: text(row.kind, "model"),
      label: text(row.label),
      started_ms: number(row.started_ms, 0),
      duration_ms: number(row.duration_ms, 0),
    };
    if (typeof row.failed === "boolean") span.failed = row.failed;
    if (typeof row.detail === "string") span.detail = row.detail;
    spans.push(span);
  }
  return spans;
}

function decodeChecklistItems(value: unknown): ChecklistItem[] {
  const items: ChecklistItem[] = [];
  for (const row of records(value)) {
    const rawStatus = text(row.status, "Pending");
    const status = CHECKLIST_STATUSES.has(rawStatus)
      ? (rawStatus as ChecklistStatus)
      : "Pending";
    items.push({
      label: text(row.label),
      status,
      result: nullableText(row.result),
      thread_id: nullableText(row.thread_id),
      bot_id: nullableText(row.bot_id),
    });
  }
  return items;
}

type TextContent = Extract<MessageContent, { type: "Text" }>;

function textContent(body: string, row?: JsonRecord): TextContent {
  const content: TextContent = { type: "Text", text: body };
  if (!row) return content;

  if (Array.isArray(row.sources)) content.sources = row.sources;
  if (typeof row.reasoning === "string") content.reasoning = row.reasoning;

  const tools = decodeToolTraces(row.tools);
  if (tools.length > 0) content.tools = tools;

  const spans = decodeSpans(row.spans);
  if (spans.length > 0) content.spans = spans;

  return content;
}

function primitiveText(value: unknown): string {
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value);
  }
  return "";
}

/**
 * Normalise one message `content` value.
 *
 * Rust stores `MessageContent` as a tagged object (`Text`, `Checklist`,
 * `ToolCall`, `ToolResult`), but older rows and optimistic UI rows also carry a
 * bare string or `{ text }`. Anything unrecognised becomes a `Text` block with
 * the best available string, never an exception.
 */
export function decodeMessageContent(value: unknown): MessageContent {
  if (typeof value === "string") return textContent(value);

  const row = asRecord(value);
  if (!row) return textContent(primitiveText(value));

  // Accept `Text`, `text`, `tool_call`, `ToolResult`, … from mixed producers.
  const tag = text(row.type).toLowerCase().replace(/[\s_-]/g, "");
  switch (tag) {
    case "text":
      return textContent(text(row.text), row);

    case "checklist":
      return {
        type: "Checklist",
        text: nullableText(row.text),
        items: decodeChecklistItems(row.items),
      };

    case "toolcall":
      return {
        type: "ToolCall",
        tool_name: text(row.tool_name, text(row.toolName)),
        arguments: row.arguments ?? null,
      };

    case "toolresult":
      return {
        type: "ToolResult",
        tool_name: text(row.tool_name, text(row.toolName)),
        result: row.result ?? null,
        is_error: boolean(row.is_error, false),
      };

    default:
      // A bare object with prose is a legacy/optimistic Text block.
      return textContent(text(row.text), row);
  }
}

function decodeMessageRow(value: unknown): Message | null {
  const row = asRecord(value);
  if (!row) return null;
  const id = text(row.id).trim();
  if (!id) return null;

  return {
    id,
    thread_id: text(row.thread_id),
    role: messageRole(row.role),
    content: decodeMessageContent(row.content),
    attachments: decodeAttachments(row.attachments),
    sender_bot_id: nullableText(row.sender_bot_id),
    sender_name: nullableText(row.sender_name),
    reply_to_id: nullableText(row.reply_to_id),
    reactions: strings(row.reactions),
    context_used_tokens: nullableNumber(row.context_used_tokens),
    context_window_tokens: nullableNumber(row.context_window_tokens),
    context_compacted: boolean(row.context_compacted, false),
    context_system_tokens: nullableNumber(row.context_system_tokens),
    context_tool_tokens: nullableNumber(row.context_tool_tokens),
    context_memory_tokens: nullableNumber(row.context_memory_tokens),
    context_message_tokens: nullableNumber(row.context_message_tokens),
    created_at: text(row.created_at),
  };
}

/** Decode `list_messages` / `list_messages_page.messages`. Never throws. */
export function decodeMessages(value: unknown): Message[] {
  if (!Array.isArray(value)) return [];
  const messages: Message[] = [];
  for (const entry of value) {
    const message = decodeMessageRow(entry);
    if (message) messages.push(message);
  }
  return messages;
}

// ── Attention inbox ─────────────────────────────────────────────────────────

function attentionKind(value: unknown): AttentionKind {
  return typeof value === "string" && ATTENTION_KINDS.has(value)
    ? (value as AttentionKind)
    : "queue";
}

function decodeAttentionRow(value: unknown): AttentionItem | null {
  const row = asRecord(value);
  if (!row) return null;
  const id = text(row.id).trim();
  if (!id) return null;

  const item: AttentionItem = {
    id,
    kind: attentionKind(row.kind),
    bot_id: nullableText(row.bot_id),
    bot_name: text(row.bot_name),
    thread_id: nullableText(row.thread_id),
    office_id: nullableText(row.office_id),
    office_name: nullableText(row.office_name),
    title: text(row.title),
    detail: text(row.detail),
    risk: nullableText(row.risk),
    state: text(row.state, "pending"),
    question_kind: nullableText(row.question_kind),
    created_at: text(row.created_at),
  };

  // Alert fields are optional. Preserve absence so a non-alert item keeps the
  // same shape it was serialized with (and notifications compare cleanly).
  if (row.alert_kind !== undefined) item.alert_kind = nullableText(row.alert_kind);
  if (row.percent !== undefined) item.percent = nullableNumber(row.percent);
  if (row.budget !== undefined) item.budget = nullableNumber(row.budget);
  if (row.cost !== undefined) item.cost = nullableNumber(row.cost);

  return item;
}

/** Decode `list_attention_items`. Never throws; rows without an id drop. */
export function decodeAttentionItems(value: unknown): AttentionItem[] {
  if (!Array.isArray(value)) return [];
  const items: AttentionItem[] = [];
  for (const entry of value) {
    const item = decodeAttentionRow(entry);
    if (item) items.push(item);
  }
  return items;
}
