// Fleet activity state machine (OpenBot SidebarAgentIndicator): derives live
// per-bot activity from the global agent-stream feed — independent of the
// persisted bot.status field.
//   working  → run_started / delta / tool_started
//   attention→ approval_requested / question_asked / paused
//   responded→ done (holds ~3s, then back to idle)
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Activity = "working" | "attention" | "responded" | "idle";

const RESPONDED_HOLD_MS = 3000;

class FleetActivity {
  states = $state<Record<string, Activity>>({});
  private timers = new Map<string, ReturnType<typeof setTimeout>>();
  private unlisten: UnlistenFn | null = null;
  private refs = 0;

  get(botId: string): Activity {
    return this.states[botId] ?? "idle";
  }

  /** Subscribe once; refcounted so HMR / re-mounts don't double-listen. */
  async start(): Promise<void> {
    this.refs++;
    if (this.unlisten || this.listening) return;
    this.listening = true;
    const fn = await listen<any>("agent-stream", (e) => this.handle(e.payload));
    this.listening = false;
    if (this.refs === 0) {
      fn();
      return;
    }
    this.unlisten = fn;
  }

  private listening = false;

  stop(): void {
    this.refs = Math.max(0, this.refs - 1);
    if (this.refs > 0) return;
    this.unlisten?.();
    this.unlisten = null;
    for (const t of this.timers.values()) clearTimeout(t);
    this.timers.clear();
    this.states = {};
  }

  handle(p: any): void {
    const botId = p?.bot_id;
    if (!botId) return;
    switch (p.kind) {
      case "run_started":
      case "delta":
      case "tool_started":
        this.hold(botId);
        this.states[botId] = "working";
        break;
      case "approval_requested":
      case "question_asked":
      case "paused":
        this.hold(botId);
        this.states[botId] = "attention";
        break;
      case "approval_decided":
      case "question_answered":
        this.hold(botId);
        this.states[botId] = "working";
        break;
      case "done":
      case "error":
        this.hold(botId);
        this.states[botId] = "responded";
        this.timers.set(
          botId,
          setTimeout(() => {
            this.timers.delete(botId);
            this.states[botId] = "idle";
          }, RESPONDED_HOLD_MS),
        );
        break;
    }
  }

  private hold(botId: string) {
    const t = this.timers.get(botId);
    if (t) {
      clearTimeout(t);
      this.timers.delete(botId);
    }
  }
}

export const fleetActivity = new FleetActivity();
