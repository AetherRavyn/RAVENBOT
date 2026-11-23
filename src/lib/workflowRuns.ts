export interface WorkflowRunOutcome {
  stateName: "completed" | "failed";
  detail: string | null;
}

/**
 * Turn an `execute_graph` result into workflow-run history semantics.
 *
 * The graph executor always returns a blackboard, even when nodes failed or
 * were skipped. Treating that as success hid failed automation runs in the
 * history. Missing `status` is an older backend and stays completed.
 */
export function workflowRunOutcome(result: unknown): WorkflowRunOutcome {
  const value = (result ?? {}) as {
    status?: unknown;
    failures?: unknown;
  };
  const failed = value.status === "failed";
  if (!failed) return { stateName: "completed", detail: null };
  const details = Array.isArray(value.failures)
    ? value.failures.filter(
        (item): item is string => typeof item === "string" && item.trim().length > 0,
      )
    : [];
  return {
    stateName: "failed",
    detail:
      details.length > 0
        ? details.join("; ")
        : "One or more tasks did not complete.",
  };
}
