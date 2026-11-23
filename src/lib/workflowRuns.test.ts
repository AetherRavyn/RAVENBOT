import { describe, expect, it } from "vitest";
import { workflowRunOutcome } from "./workflowRuns";

describe("workflowRunOutcome", () => {
  it("marks a clean graph completed", () => {
    expect(workflowRunOutcome({ status: "completed", failures: [] })).toEqual({
      stateName: "completed",
      detail: null,
    });
  });

  it("marks failed and skipped tasks as a failed run", () => {
    expect(
      workflowRunOutcome({
        status: "failed",
        failures: ["Audit: provider error", "Ship: dependency did not complete"],
      }),
    ).toEqual({
      stateName: "failed",
      detail: "Audit: provider error; Ship: dependency did not complete",
    });
  });

  it("falls back to a readable detail when the backend omits failures", () => {
    expect(workflowRunOutcome({ status: "failed" })).toEqual({
      stateName: "failed",
      detail: "One or more tasks did not complete.",
    });
    expect(workflowRunOutcome({ status: "failed", failures: ["  ", 3] })).toEqual({
      stateName: "failed",
      detail: "One or more tasks did not complete.",
    });
  });

  it("treats an older result without status as completed", () => {
    expect(workflowRunOutcome({})).toEqual({
      stateName: "completed",
      detail: null,
    });
    expect(workflowRunOutcome(null)).toEqual({
      stateName: "completed",
      detail: null,
    });
  });
});
