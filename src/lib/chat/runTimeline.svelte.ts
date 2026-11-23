// Office run timeline: derives a chronological activity trail from the same
// agent-stream payloads the room already receives. Backend-frozen — only
// existing event kinds are recognized; anything else is ignored.
export type RunPhase = "start" | "tool" | "tool-done" | "approval" | "question" | "pause" | "done";

export interface RunEntry {
  id: number;
  botId: string;
  phase: RunPhase;
  /** Extra data for the label (currently the tool name). */
  detail?: string;
  at: number;
}

const MAX_ENTRIES = 40;

/** Map one agent-stream payload to a timeline phase, or null to ignore it. */
export function describeRunEvent(p: any): { botId: string; phase: RunPhase; detail?: string } | null {
  const botId = p?.bot_id;
  if (!botId || typeof p?.kind !== "string") return null;
  switch (p.kind) {
    case "run_started":
      return { botId, phase: "start" };
    case "tool_started":
      return { botId, phase: "tool", detail: p.name || "tool" };
    case "tool_finished":
      return { botId, phase: "tool-done", detail: p.name || undefined };
    case "approval_requested":
      return { botId, phase: "approval" };
    case "question_asked":
      return { botId, phase: "question" };
    case "paused":
      return { botId, phase: "pause" };
    case "done":
      return { botId, phase: "done" };
    default:
      return null;
  }
}

export class RunTimeline {
  events = $state<RunEntry[]>([]);
  private nextId = 1;

  /** Fold one payload in; returns true when a visible entry was appended. */
  track(p: any, now = Date.now()): boolean {
    const d = describeRunEvent(p);
    if (!d) return false;
    this.events = [...this.events, { id: this.nextId++, ...d, at: now }].slice(-MAX_ENTRIES);
    return true;
  }

  reset(): void {
    this.events = [];
  }
}

export interface BotRunSummary {
  botId: string;
  phase: RunPhase;
  detail?: string;
  /** Last event time, so the strip can order the chips. */
  at: number;
  /** Tool calls started in this run. */
  toolCalls: number;
  /** Tool calls that finished. */
  toolsDone: number;
}

/**
 * Collapse a chronological trail into one chip per agent.
 *
 * The strip used to render every event. A long office run therefore produced a
 * horizontal wall of "Tester finished a tool" pills that pushed the useful
 * state off-screen. The per-agent latest phase plus a tool count carries the
 * same information in one glance.
 */
export function summarizeRun(events: RunEntry[]): BotRunSummary[] {
  const byBot = new Map<string, BotRunSummary>();
  for (const event of events) {
    const current = byBot.get(event.botId) ?? {
      botId: event.botId,
      phase: "start" as RunPhase,
      at: event.at,
      toolCalls: 0,
      toolsDone: 0,
    };
    current.phase = event.phase;
    current.detail = event.detail;
    current.at = event.at;
    if (event.phase === "tool") current.toolCalls += 1;
    if (event.phase === "tool-done") current.toolsDone += 1;
    byBot.set(event.botId, current);
  }
  return [...byBot.values()].sort((a, b) => a.at - b.at);
}
