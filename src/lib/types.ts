// RAVENBOT TypeScript types (mirrors Rust types from crates/core)

export type BotStatus = "idle" | "thinking" | "running_tool" | "waiting_on_user" | "paused";

export type SandboxTier = "os_level" | "docker" | "host";

export type Permission =
  | { FileSystem: { paths: string[] } }
  | { Network: { domains: string[] } }
  | { Shell: null }
  | { Screenshot: null }
  | { InputControl: null }
  | { AudioCapture: null }
  | { AudioPlayback: null }
  | { Clipboard: null }
  | { Delegation: null };

export interface BotConfig {
  model_provider: string;
  model_id: string;
  fallback_provider: string | null;
  fallback_model: string | null;
  sandbox_tier: SandboxTier;
  max_tokens: number | null;
  temperature: number | null;
  custom_prompt: string | null;
}

export interface Bot {
  id: string;
  name: string;
  description: string;
  avatar_color: string;
  status: BotStatus;
  config: BotConfig;
  permissions: Permission[];
  is_orchestrator: boolean;
  delegate_to: string[];
  created_at: string;
  updated_at: string;
  last_active_at: string | null;
}

export interface Thread {
  id: string;
  bot_id: string;
  title: string;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export type MessageRole = "user" | "assistant" | "system" | "tool";

export type ChecklistStatus = "pending" | "in_progress" | "completed" | "failed" | "skipped";

export interface Attachment {
  id: string;
  name: string;
  mime_type: string;
  size: number;
  path: string;
}

export interface ChecklistItem {
  label: string;
  status: ChecklistStatus;
  result: string | null;
  thread_id: string | null;
  bot_id: string | null;
}

export type MessageContent =
  | { type: "text"; text: string }
  | { type: "checklist"; text: string | null; items: ChecklistItem[] }
  | { type: "tool_call"; tool_name: string; arguments: any }
  | { type: "tool_result"; tool_name: string; result: any; is_error: boolean };

export interface Message {
  id: string;
  thread_id: string;
  role: MessageRole;
  content: MessageContent;
  attachments: Attachment[];
  created_at: string;
}

export type RunState = 
  | "planning"
  | "acting"
  | "observing"
  | "reflecting"
  | "waiting_on_user"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface Run {
  id: string;
  bot_id: string;
  thread_id: string;
  parent_run_id: string | null;
  state: RunState;
  checkpoint: any | null;
  outcome: any | null;
  tokens_consumed: number;
  cost_estimate: number;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export type BudgetLimit =
  | { Unlimited: null }
  | { Tokens: { max: number } }
  | { Cost: { max: number } };

export type BudgetBehavior = "hard_stop" | "warn" | "ask_user";

export type BudgetPeriod = "hourly" | "daily" | "weekly" | "monthly" | "total";

export interface Budget {
  id: string;
  bot_id: string;
  limit: BudgetLimit;
  behavior: BudgetBehavior;
  period: BudgetPeriod;
  created_at: string;
  updated_at: string;
}

// IPC Command types
export type Command =
  | { command: "CreateBot"; name: string; description: string }
  | { command: "ListBots" }
  | { command: "GetBot"; bot_id: string }
  | { command: "UpdateBot"; bot: Bot }
  | { command: "DeleteBot"; bot_id: string }
  | { command: "CreateThread"; bot_id: string; title: string }
  | { command: "ListThreads"; bot_id: string }
  | { command: "GetThread"; thread_id: string }
  | { command: "ListMessages"; thread_id: string }
  | { command: "SendMessage"; thread_id: string; content: string }
  | { command: "PauseAll" }
  | { command: "ResumeAll" }
  | { command: "GetStatus" };

// IPC Event types
export type Event =
  | { event: "BotCreated"; bot: Bot }
  | { event: "BotUpdated"; bot: Bot }
  | { event: "BotDeleted"; bot_id: string }
  | { event: "BotStatusChanged"; bot_id: string; status: BotStatus }
  | { event: "ThreadCreated"; thread: Thread }
  | { event: "MessageAdded"; message: Message }
  | { event: "MessageUpdated"; message: Message }
  | { event: "RunStarted"; run: Run }
  | { event: "RunStateChanged"; run: Run }
  | { event: "RunCompleted"; run: Run }
  | { event: "Error"; message: string; code: string | null };
