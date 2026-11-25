/**
 * Pending human-in-the-loop requests surfaced by the runtime.
 *
 * A run parks server-side until an approval is decided or a question is
 * answered, so these rows are read and resolved by the owning view.
 */

import type { InteractionBlock, QuestionKind } from "$lib/chat/blocks";

export interface PendingApproval {
  id: string;
  bot_id: string;
  thread_id: string;
  run_id: string;
  tool_name: string;
  tool_label: string;
  arguments: any;
  risk: string;
  status: string;
  created_at: string;
}

export interface PendingQuestion {
  id: string;
  bot_id: string;
  thread_id: string;
  run_id: string;
  header: string;
  question: string;
  options: string[];
  allow_custom: boolean;
  kind?: QuestionKind | null;
  payload?: InteractionBlock | null;
  status: string;
  created_at: string;
}
