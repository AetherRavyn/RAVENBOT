/**
 * Structured interaction blocks.
 *
 * These types mirror the Rust contract in `crates/core/src/question.rs`. The
 * host validates a block before it is stored; the UI still treats every field
 * as plain text and never as HTML.
 */

export type QuestionKind = "question" | "confirm" | "choice" | "form" | "vault";

export interface BlockOption {
  id: string;
  label: string;
  meta?: string | null;
  selected?: boolean;
}

export interface BlockField {
  id: string;
  label: string;
  kind?: "text" | "textarea" | "select";
  required?: boolean;
  options?: string[];
  placeholder?: string | null;
  value?: string | null;
}

export interface BlockAction {
  id: string;
  label: string;
  style?: "primary" | "secondary" | "danger" | null;
  danger?: boolean;
  confirm?: boolean;
  hold_ms?: number | null;
}

export interface InteractionBlock {
  title?: string | null;
  description?: string | null;
  danger?: boolean;
  multiple?: boolean;
  allow_text?: boolean;
  submit_label?: string | null;
  preview?: string | null;
  site?: string | null;
  purpose?: string | null;
  item_kind?: "login" | "api_key" | "card" | "contact" | null;
  name?: string | null;
  placeholder?: string | null;
  options?: BlockOption[];
  fields?: BlockField[];
  actions?: BlockAction[];
}

export type BlockValue = string | string[];

/** What a structured card sends into the existing `answer_question` string slot. */
export interface BlockResponse {
  action_id: string;
  values?: Record<string, BlockValue>;
  text?: string;
}

/** Serialize a block response for the runtime, which parses it back to JSON. */
export function serializeBlockResponse(response: BlockResponse): string {
  return JSON.stringify(response);
}

/** The default submit label for a choice or form. */
export function submitLabel(payload: InteractionBlock, fallback: string): string {
  return payload.submit_label?.trim() || fallback;
}

/** Whether a field must have a non-empty value before submit. */
export function missingRequiredFields(
  fields: BlockField[],
  values: Record<string, string>,
): BlockField[] {
  return fields.filter((field) => field.required && !(values[field.id] ?? "").trim());
}
