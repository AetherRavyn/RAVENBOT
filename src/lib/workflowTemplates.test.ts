import { describe, expect, it } from "vitest";
import {
  WORKFLOW_TEMPLATES,
  materializeWorkflowTemplate,
} from "./workflowTemplates";

const members = [
  {
    bot_id: "lead-1",
    rank: "lead",
    specialty: "strategy",
    bot: { name: "Chief of Staff" },
  },
  {
    bot_id: "engineer-1",
    rank: "specialist",
    specialty: "engineering",
    bot: { name: "Staff Engineer" },
  },
  {
    bot_id: "writer-1",
    rank: "specialist",
    specialty: "content",
    bot: { name: "Writer" },
  },
];

describe("workflow templates", () => {
  it("are internally valid starting points", () => {
    const ids = new Set<string>();
    for (const template of WORKFLOW_TEMPLATES) {
      expect(ids.has(template.id)).toBe(false);
      ids.add(template.id);
      expect(template.tasks.length).toBeGreaterThan(0);
      template.tasks.forEach((task, index) => {
        expect(task.label.trim().length).toBeGreaterThan(0);
        for (const dependency of task.dependsOn) {
          expect(dependency).toBeGreaterThanOrEqual(0);
          expect(dependency).toBeLessThan(index);
        }
        const iterations = task.loopIterations ?? 1;
        expect(iterations).toBeGreaterThanOrEqual(1);
        expect(iterations).toBeLessThanOrEqual(10);
        if (task.loopUntil) {
          expect(["contains", "not_contains"]).toContain(task.loopUntil.operator);
          expect(task.loopUntil.value.trim().length).toBeGreaterThan(0);
        }
        if (task.outputSchema) {
          expect(task.outputSchema.type).toBe("object");
        }
      });
    }
  });

  it("maps role hints onto office members", () => {
    const template = WORKFLOW_TEMPLATES.find((item) => item.id === "release-audit");
    expect(template).toBeTruthy();
    const applied = materializeWorkflowTemplate(template!, members);
    expect(applied.goal).toBe(template!.goal);
    expect(applied.tasks[0].botId).toBe("lead-1");
    expect(applied.tasks[1].botId).toBe("engineer-1");
    expect(applied.tasks[3].botId).toBe("lead-1");
  });

  it("falls back by position when a role is missing", () => {
    const template = WORKFLOW_TEMPLATES.find((item) => item.id === "research-brief");
    const applied = materializeWorkflowTemplate(template!, [
      { bot_id: "only-1", rank: "lead", specialty: "general", bot: { name: "Generalist" } },
    ]);
    expect(applied.tasks.every((task) => task.botId === "only-1")).toBe(true);
  });

  it("preserves dependencies, retries, schemas, and loops for the Plan editor", () => {
    const template = WORKFLOW_TEMPLATES.find((item) => item.id === "content-pipeline");
    const applied = materializeWorkflowTemplate(template!, members);
    const revise = applied.tasks[3];
    expect(revise.dependsOn).toEqual([2]);
    expect(revise.loopIterations).toBe(3);
    expect(revise.loopUntil).toEqual({ operator: "contains", value: "APPROVED" });
    expect(revise.outputSchema).toContain('"body"');
    expect(applied.tasks[4].dependsOn).toEqual([3]);
    expect(applied.tasks[4].retries).toBe(0);
  });
});
