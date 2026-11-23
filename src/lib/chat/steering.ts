/**
 * Whether the composer can steer the active run.
 *
 * Steering is intentionally narrow: the run must be executing a tool, so the
 * message has a guaranteed next model call to join. Text-only avoids silently
 * dropping an attachment the running turn cannot consume.
 */
export interface Steerability {
  running: boolean;
  streamingTool: string | null;
  activeRunId: string | null;
  text: string;
  attachmentCount: number;
}

export function canSteerRun(input: Steerability): boolean {
  return (
    input.running &&
    !!input.streamingTool &&
    !!input.activeRunId &&
    input.text.trim().length > 0 &&
    input.attachmentCount === 0
  );
}
