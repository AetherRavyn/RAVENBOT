/**
 * The live actions an agent performs on the browser or the real desktop.
 *
 * Events are ephemeral UI state, capped so a long run cannot grow the array
 * forever. The computer panel uses the pointer coordinates to draw a click
 * marker; the browser dock uses the latest browser event as its activity strip.
 */

export interface AgentActivityEvent {
  surface: "browser" | "computer" | "window" | string;
  bot_id: string;
  action: string;
  detail: string;
  ok: boolean;
  at: string;
  x?: number | null;
  y?: number | null;
}

const MAX_EVENTS = 40;

export class AgentActivityStore {
  events = $state<AgentActivityEvent[]>([]);
  /** Synced from the Tauri command; agent desktop input is paused while true. */
  computerPaused = $state(false);

  record(event: AgentActivityEvent): void {
    this.events = [event, ...this.events].slice(0, MAX_EVENTS);
  }

  latest(surface: "browser" | "computer"): AgentActivityEvent | null {
    if (surface === "browser") {
      return this.events.find((event) => event.surface === "browser") ?? null;
    }
    return (
      this.events.find(
        (event) => event.surface === "computer" || event.surface === "window",
      ) ?? null
    );
  }

  recent(surface: "browser" | "computer"): AgentActivityEvent[] {
    const matches =
      surface === "browser"
        ? (event: AgentActivityEvent) => event.surface === "browser"
        : (event: AgentActivityEvent) =>
            event.surface === "computer" || event.surface === "window";
    return this.events.filter(matches).slice(0, 8);
  }

  clear(): void {
    this.events = [];
  }
}

export const agentActivity = new AgentActivityStore();
