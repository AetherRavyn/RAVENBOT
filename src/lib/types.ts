// RAVENBOT TypeScript types (mirrors Rust types from crates/core)

export type BotStatus = "idle" | "thinking" | "running_tool" | "waiting_on_user" | "paused";

export type SandboxTier = "os_level" | "docker" | "host";

/** Rust `ApprovalMode`, serialized with `#[serde(rename_all = "snake_case")]`. */
export type ApprovalMode = "ask" | "auto" | "full";

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
  /** Maximum model↔tool rounds per run (null = runtime default). */
  max_tool_rounds: number | null;
  /** Preferred text-to-speech voice id (null = engine default). */
  voice_id: string | null;
  /** Read this bot's replies aloud automatically as they land. */
  auto_read: boolean;
  /** Allow this agent to control the host computer (mouse/keyboard). */
  host_control: boolean;
  /** Project folder this agent works in (null = inherit office/channel). */
  working_folder: string | null;
  /** Job-library template this agent was installed from, if any. */
  job_id?: string | null;
}

export interface Bot {
  id: string;
  name: string;
  description: string;
  avatar_color: string;
  /** DiceBear or custom avatar URL; animated via CSS when DiceBear. */
  avatar_url: string | null;
  /** DiceBear style (avataaars, bottts, personas, lorelei, ...). */
  avatar_style: string | null;
  /** Rank in office (e.g. CTO, Tech Lead). */
  rank: string | null;
  /** Specialty / role lane. */
  specialty: string | null;
  status: BotStatus;
  config: BotConfig;
  permissions: Permission[];
  is_orchestrator: boolean;
  delegate_to: string[];
  /** Enabled skill ids (empty = all built-ins enabled). */
  skills: string[];
  approval_mode: ApprovalMode;
  created_at: string;
  updated_at: string;
  last_active_at: string | null;
}

/** Rust `ThreadKind`, serialized snake_case. */
export type ThreadKind =
  | "chat"
  | "office"
  | "delegation"
  | "routine"
  | "node"
  | "channel"
  | "goal"
  | "task";

export interface Thread {
  id: string;
  bot_id: string;
  title: string;
  is_active: boolean;
  /** Ephemeral thread: skip agent-memory persistence (RAG, self-review). */
  ephemeral: boolean;
  /** Channel (context) this thread is filed under, if any. */
  channel_id: string | null;
  /** What created this thread. */
  kind: ThreadKind;
  /** Whether the thread belongs in the user-facing history list. */
  visible: boolean;
  created_at: string;
  updated_at: string;
}

export type MessageRole = "user" | "assistant" | "system" | "tool";

export type ChecklistStatus = "Pending" | "InProgress" | "Completed" | "Failed" | "Skipped";

export interface Attachment {
  id: string;
  name: string;
  mime_type: string;
  size: number;
  path: string;
  /** Inline base64 payload for pasted/dropped images sent without disk IO. */
  data?: string | null;
  /** Whether this attachment is an image the model can see. */
  is_image?: boolean;
}

export interface ChecklistItem {
  label: string;
  status: ChecklistStatus;
  result: string | null;
  thread_id: string | null;
  bot_id: string | null;
}

// Rust serializes this enum with `#[serde(tag = "type")]` and no rename_all,
// so the discriminants are the PascalCase variant names.
export type MessageContent =
  /**
   * `reasoning` is the model's thinking, kept apart from `text` on purpose.
   *
   * It was previously prefixed into `text` as a `<think>` block, which meant the
   * streamed buffer could be cleared mid-thought, an interrupted stream left the
   * markers sitting in the user's paragraph, and any renderer that missed a
   * marker showed private notes as answer text. Optional because rows written
   * before the field existed simply do not have one.
   */
  | {
      type: "Text";
      text: string;
      sources?: unknown[];
      reasoning?: string | null;
      tools?: ToolTrace[];
      spans?: TraceSpan[];
    }
  | { type: "Checklist"; text: string | null; items: ChecklistItem[] }
  | { type: "ToolCall"; tool_name: string; arguments: any }
  | { type: "ToolResult"; tool_name: string; result: any; is_error: boolean };

/**
 * One tool an agent ran while producing the message it is attached to.
 *
 * These used to exist only as a transient SSE line in the browser: reload the
 * page and every tool call in the conversation was gone, leaving answers with no
 * account of how they were reached. The variants `ToolCall` / `ToolResult` had
 * existed in the schema since the beginning and nothing ever wrote one.
 *
 * Attached to the assistant message rather than as its own rows, because rows
 * would be fed back to the model as history and because a tool call belongs to
 * the turn that made it.
 */
export interface ToolTrace {
  /** The skill name as the model called it. */
  name: string;
  /** Arguments as given — the interesting part when an answer looks wrong. */
  arguments?: unknown;
  /** Truncated result. Full results are megabytes; the answer is a few lines. */
  result?: unknown;
  is_error?: boolean;
  /** Wall-clock milliseconds. A slow turn is usually one slow call. */
  duration_ms?: number | null;
  /** User-configured per-call price for a paid tool, when one exists. */
  cost?: number | null;
}

export interface TraceSpan {
  id: string;
  kind: "model" | "tool" | "memory" | "context" | "delegation" | string;
  label: string;
  started_ms: number;
  duration_ms: number;
  failed?: boolean;
  detail?: string | null;
}

export interface Message {
  id: string;
  thread_id: string;
  role: MessageRole;
  content: MessageContent;
  attachments: Attachment[];
  sender_bot_id?: string | null;
  sender_name?: string | null;
  reply_to_id?: string | null;
  reactions?: string[];
  context_used_tokens?: number | null;
  context_window_tokens?: number | null;
  context_compacted?: boolean;
  context_system_tokens?: number | null;
  context_tool_tokens?: number | null;
  context_memory_tokens?: number | null;
  context_message_tokens?: number | null;
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
  /** Portion spent on interactive model calls. */
  model_cost: number;
  /** Portion spent on paid tool calls. */
  tool_cost: number;
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

// ── Cross-fleet inbox ───────────────────────────────────────────────────────

export type AttentionKind = "approval" | "question" | "queue" | "alert";
export type AttentionFilter = "all" | AttentionKind;

/**
 * One item in the global attention inbox.
 *
 * Mirrors the Rust `AttentionItem` projection (src-tauri/src/lib.rs): approvals,
 * questions, office queue rows, and budget alerts all share this shape. `state`
 * is intentionally open — the backend may grow new queue states — while `kind`
 * stays a closed union because the inbox groups and filters on it.
 */
export interface AttentionItem {
  id: string;
  kind: AttentionKind;
  bot_id: string | null;
  bot_name: string;
  /** The agent's stored avatar, resolved by the shared roster avatar. */
  avatar_url?: string | null;
  avatar_style?: string | null;
  thread_id: string | null;
  office_id: string | null;
  office_name: string | null;
  title: string;
  detail: string;
  risk: string | null;
  /** `pending`, `running`, or `failed`; intentionally open for future states. */
  state: string;
  question_kind: string | null;
  /** `budget` for an office health alert. */
  alert_kind?: string | null;
  percent?: number | null;
  budget?: number | null;
  cost?: number | null;
  created_at: string;
}

// ── Run liveness ────────────────────────────────────────────────────────────

/** Rust `RunProgress` (crates/db/src/queries.rs): liveness for one run. */
export interface RunProgress {
  run_id: string;
  thread_id: string;
  state: string;
  phase: string;
  last_tool: string | null;
  last_error: string | null;
  /** Answers "is it still moving?"; `updated_at` only changes on state. */
  heartbeat_at: string | null;
  lease_owner: string | null;
  lease_expires_at: string | null;
  updated_at: string;
}

// ── Office board ────────────────────────────────────────────────────────────

export interface BoardNode {
  nodeId: string;
  botId: string;
  label: string;
  /** Node ids this task waits on (resolved deps from `plan_ready`). */
  dependsOn: string[];
  state: "pending" | "running" | "done" | "failed" | "skipped";
  preview?: string;
  /** Thread this node streams on; used to re-check liveness after a reload. */
  nodeThreadId?: string;
}

export interface BotTodo {
  id: string;
  task: string;
  done: boolean;
}

/** One row from the durable work ledger (previous cycles included). */
export interface BoardWorkItem {
  id: string;
  title: string;
  status: string;
  owner_bot_id?: string | null;
  attempts?: number;
  max_attempts?: number;
  error?: string | null;
  result?: string | null;
}

/** Persisted office-board snapshot returned by `get_office_board`. */
export interface OfficeBoard {
  goal: string;
  nodes: BoardNode[];
  /** Node threads whose latest run is still non-terminal. */
  liveNodeThreads?: string[];
}

// ── Scheduler ───────────────────────────────────────────────────────────────

/** Rust `Routine` (crates/core/src/routine.rs). */
export interface Routine {
  id: string;
  bot_id: string;
  name: string;
  description: string;
  schedule: string;
  instruction: string;
  is_enabled: boolean;
  last_run_at: string | null;
  created_at: string;
  updated_at: string;
}

// ── Job library ─────────────────────────────────────────────────────────────

export type JobGroup =
  | "marketing"
  | "sales"
  | "engineering"
  | "operations"
  | "finance"
  | "people"
  | "research"
  | "personal";

export type JobCadence = "one_off" | "recurring" | "monitoring";

export type JobOversight = "low" | "review" | "approval";

/** Rust `JobTemplate` (crates/core/src/job.rs). */
export interface JobTemplate {
  id: string;
  name: string;
  summary: string;
  group: JobGroup;
  cadence: JobCadence;
  /** Five-field cron, or `@manual` for a one-off. */
  schedule: string;
  oversight: JobOversight;
  role_title: string;
  agent_prompt: string;
  routine_instruction: string;
  deliverable: string;
}

/** Result of the `install_job` command. */
export interface InstalledJob {
  job_id: string;
  bot: Bot;
  routine: Routine;
  thread_id: string;
  duplicate: boolean;
}

// ── Connectors (MCP servers) ────────────────────────────────────────────────

export type McpTransport = "stdio" | "http";

/** One BYOC credential input, mirrors Rust `EnvField`. */
export interface EnvField {
  key: string;
  label: string;
  description: string;
  required: boolean;
  secret?: boolean;
  placeholder?: string;
}

/** Rust `McpServerSummary` (crates/mcp/src/servers.rs). */
export interface McpServerSummary {
  id: string;
  name: string;
  description: string;
  category: string;
  icon: string;
  command: string;
  args: string[];
  env_keys: string[];
  env_fields: EnvField[];
  url: string | null;
  transport: McpTransport;
  enabled: boolean;
  is_custom: boolean;
  /** Whether the launcher package was found by the catalog audit. */
  verified: boolean;
  env_configured: boolean;
  assigned_bot_ids: string[];
  tools_count: number;
}

/** The app calls MCP servers "connectors"; same wire shape. */
export type Connector = McpServerSummary;

// ── App status ──────────────────────────────────────────────────────────────

/** Rust `StatusInfo` returned by `get_status`. */
export interface StatusInfo {
  active_bots: number;
  running_tasks: number;
  session_tokens: number;
  session_cost: number;
  kill_switch_active: boolean;
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
