/**
 * Built-in workflow templates.
 *
 * These are starting points, not opaque automations: loading one fills the
 * normal Plan editor, where every label, dependency, schema, retry and loop is
 * editable before dispatch. Task content stays English because it becomes the
 * actual brief sent to the agents; the gallery chrome is localizable.
 */

export interface WorkflowTemplateOutput {
  type: "object";
  required?: string[];
  properties?: Record<string, unknown>;
  additionalProperties?: boolean;
}

export interface WorkflowTemplateStop {
  operator: "contains" | "not_contains";
  value: string;
}

export interface WorkflowTemplateTask {
  /** Role hint used to pick an office member; falls back by position. */
  role: string;
  label: string;
  dependsOn: number[];
  retries?: number;
  outputSchema?: WorkflowTemplateOutput;
  loopIterations?: number;
  loopUntil?: WorkflowTemplateStop;
}

export interface WorkflowTemplate {
  id: string;
  name: string;
  description: string;
  goal: string;
  tasks: WorkflowTemplateTask[];
}

export interface TemplateMember {
  bot_id?: string;
  specialty?: string;
  rank?: string;
  bot?: { name?: string };
}

export interface MaterializedTask {
  id: string;
  botId: string;
  label: string;
  dependsOn: number[];
  condition: null;
  dataFrom: null;
  retries: number;
  outputSchema: string;
  loopIterations: number;
  loopUntil: WorkflowTemplateStop | null;
  loopGroup: string;
  loopGroupTerminal: boolean;
  loopGroupIterations: number;
  loopGroupStop: WorkflowTemplateStop | null;
  nodeType: string;
}

const taskOutput = (
  properties: Record<string, unknown>,
  required: string[],
): WorkflowTemplateOutput => ({
  type: "object",
  properties,
  required,
  additionalProperties: false,
});

const stringArray = { type: "array", items: { type: "string" } };

export const WORKFLOW_TEMPLATES: WorkflowTemplate[] = [
  {
    id: "release-audit",
    name: "Release audit",
    description:
      "Inventory the release, audit it, fix blockers, then make a grounded ship/no-ship call.",
    goal: "Audit the upcoming release and decide whether it is safe to ship.",
    tasks: [
      {
        role: "lead",
        label:
          "Inventory what changed since the last release: user-visible changes, risky areas, and open risks.",
        dependsOn: [],
        outputSchema: taskOutput(
          {
            summary: { type: "string" },
            changes: stringArray,
            risks: stringArray,
          },
          ["summary", "changes", "risks"],
        ),
      },
      {
        role: "engineer",
        label:
          "Audit the release for regressions, security issues, and missing tests. Report concrete findings.",
        dependsOn: [0],
        retries: 1,
        outputSchema: taskOutput(
          {
            findings: {
              type: "array",
              items: {
                type: "object",
                properties: {
                  severity: { type: "string", enum: ["high", "medium", "low"] },
                  issue: { type: "string" },
                  evidence: { type: "string" },
                },
                required: ["severity", "issue", "evidence"],
              },
            },
          },
          ["findings"],
        ),
      },
      {
        role: "engineer",
        label:
          "Fix every high-severity finding. Re-run the relevant checks and report what remains.",
        dependsOn: [1],
        retries: 1,
        loopIterations: 3,
        loopUntil: { operator: "contains", value: "NO BLOCKERS" },
        outputSchema: taskOutput(
          {
            fixed: stringArray,
            remaining: stringArray,
            status: { type: "string" },
          },
          ["fixed", "remaining", "status"],
        ),
      },
      {
        role: "lead",
        label:
          "Verify the fixes against the audit and issue a ship/no-ship decision with evidence.",
        dependsOn: [1, 2],
        outputSchema: taskOutput(
          {
            decision: { type: "string", enum: ["ship", "hold"] },
            rationale: { type: "string" },
            blockers: stringArray,
          },
          ["decision", "rationale", "blockers"],
        ),
      },
    ],
  },
  {
    id: "research-brief",
    name: "Research brief",
    description:
      "Scope a question, gather sources, challenge the findings, and produce a cited brief.",
    goal: "Produce a decision-ready research brief with evidence and open questions.",
    tasks: [
      {
        role: "lead",
        label:
          "Turn the request into 3–5 answerable research questions and define what evidence would settle each one.",
        dependsOn: [],
        outputSchema: taskOutput(
          { questions: stringArray, success_criteria: stringArray },
          ["questions", "success_criteria"],
        ),
      },
      {
        role: "researcher",
        label:
          "Gather evidence for each question. Prefer primary sources and record exact links and dates.",
        dependsOn: [0],
        retries: 1,
        outputSchema: taskOutput(
          {
            findings: {
              type: "array",
              items: {
                type: "object",
                properties: {
                  question: { type: "string" },
                  finding: { type: "string" },
                  source: { type: "string" },
                  confidence: { type: "string", enum: ["high", "medium", "low"] },
                },
                required: ["question", "finding", "source", "confidence"],
              },
            },
          },
          ["findings"],
        ),
      },
      {
        role: "analyst",
        label:
          "Challenge the findings: look for contradictory evidence, stale data, and unsupported claims. State NO GAPS when the evidence is sound.",
        dependsOn: [1],
        loopIterations: 2,
        loopUntil: { operator: "contains", value: "NO GAPS" },
        outputSchema: taskOutput(
          { gaps: stringArray, contradictions: stringArray, status: { type: "string" } },
          ["gaps", "contradictions", "status"],
        ),
      },
      {
        role: "lead",
        label:
          "Write the final brief: answer each question, cite sources, and separate facts from recommendations.",
        dependsOn: [1, 2],
        outputSchema: taskOutput(
          {
            brief: { type: "string" },
            sources: stringArray,
            open_questions: stringArray,
          },
          ["brief", "sources", "open_questions"],
        ),
      },
    ],
  },
  {
    id: "content-pipeline",
    name: "Content pipeline",
    description:
      "Outline, draft, review, and revise until the editor approves, then prepare publication.",
    goal: "Ship a reviewed, publication-ready piece of content.",
    tasks: [
      {
        role: "writer",
        label:
          "Write the outline: audience, promise, section order, and the evidence each section needs.",
        dependsOn: [],
        outputSchema: taskOutput(
          { audience: { type: "string" }, sections: stringArray, evidence: stringArray },
          ["audience", "sections", "evidence"],
        ),
      },
      {
        role: "writer",
        label:
          "Draft the piece from the outline. Keep claims concrete and mark anything that still needs a source.",
        dependsOn: [0],
        retries: 1,
        outputSchema: taskOutput(
          {
            title: { type: "string" },
            body: { type: "string" },
            unsourced_claims: stringArray,
          },
          ["title", "body", "unsourced_claims"],
        ),
      },
      {
        role: "editor",
        label:
          "Review the draft for accuracy, structure, and voice. List required changes and verdict.",
        dependsOn: [1],
        outputSchema: taskOutput(
          {
            verdict: { type: "string", enum: ["approve", "revise"] },
            required_changes: stringArray,
            notes: { type: "string" },
          },
          ["verdict", "required_changes", "notes"],
        ),
      },
      {
        role: "writer",
        label:
          "Revise the draft against the editor's notes. Reply APPROVED only when every required change is addressed.",
        dependsOn: [2],
        loopIterations: 3,
        loopUntil: { operator: "contains", value: "APPROVED" },
        outputSchema: taskOutput(
          {
            body: { type: "string" },
            changes_made: stringArray,
            status: { type: "string" },
          },
          ["body", "changes_made", "status"],
        ),
      },
      {
        role: "lead",
        label:
          "Prepare the publication checklist: title, summary, links, assets, and final approval.",
        dependsOn: [3],
        outputSchema: taskOutput(
          {
            title: { type: "string" },
            summary: { type: "string" },
            checklist: stringArray,
            approved: { type: "boolean" },
          },
          ["title", "summary", "checklist", "approved"],
        ),
      },
    ],
  },
  {
    id: "bug-triage",
    name: "Bug triage",
    description:
      "Reproduce a bug, find the root cause, fix it, and verify the fix with evidence.",
    goal: "Find, fix, and verify the reported bug without guessing.",
    tasks: [
      {
        role: "engineer",
        label:
          "Reproduce the bug from the report. Record exact steps, environment, and observed vs expected behavior.",
        dependsOn: [],
        retries: 1,
        outputSchema: taskOutput(
          {
            reproduced: { type: "boolean" },
            steps: stringArray,
            observed: { type: "string" },
            expected: { type: "string" },
          },
          ["reproduced", "steps", "observed", "expected"],
        ),
      },
      {
        role: "engineer",
        label:
          "Diagnose the failure. Trace it to a root cause and cite the files, lines, or logs that prove it. State ROOT CAUSE when confirmed.",
        dependsOn: [0],
        loopIterations: 2,
        loopUntil: { operator: "contains", value: "ROOT CAUSE" },
        outputSchema: taskOutput(
          {
            root_cause: { type: "string" },
            evidence: stringArray,
            affected_areas: stringArray,
          },
          ["root_cause", "evidence", "affected_areas"],
        ),
      },
      {
        role: "engineer",
        label:
          "Implement the smallest correct fix and add a regression test. Report the commands you ran.",
        dependsOn: [1],
        retries: 1,
        outputSchema: taskOutput(
          {
            fix: { type: "string" },
            files_changed: stringArray,
            tests: stringArray,
          },
          ["fix", "files_changed", "tests"],
        ),
      },
      {
        role: "lead",
        label:
          "Verify the fix against the original report and decision on whether the bug is closed.",
        dependsOn: [2],
        outputSchema: taskOutput(
          {
            fixed: { type: "boolean" },
            verification: { type: "string" },
            follow_ups: stringArray,
          },
          ["fixed", "verification", "follow_ups"],
        ),
      },
    ],
  },
];

function memberLabel(member: TemplateMember): string {
  return [member.bot?.name, member.specialty, member.rank]
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .join(" ")
    .toLowerCase();
}

function resolveMember(
  members: TemplateMember[],
  task: WorkflowTemplateTask,
  index: number,
): string {
  const role = task.role.trim().toLowerCase();
  if (role) {
    const match = members.find((member) => memberLabel(member).includes(role));
    if (match?.bot_id) return match.bot_id;
  }
  return members[index % Math.max(1, members.length)]?.bot_id ?? "";
}

/** Fill a template with this office's members and the Plan editor's shape. */
export function materializeWorkflowTemplate(
  template: WorkflowTemplate,
  members: TemplateMember[],
): { goal: string; tasks: MaterializedTask[] } {
  return {
    goal: template.goal,
    tasks: template.tasks.map((task, index) => ({
      id: String(index),
      botId: resolveMember(members, task, index),
      label: task.label,
      dependsOn: [...task.dependsOn],
      condition: null,
      dataFrom: null,
      retries: task.retries ?? 0,
      outputSchema: task.outputSchema
        ? JSON.stringify(task.outputSchema, null, 2)
        : "",
      loopIterations: task.loopIterations ?? 1,
      loopUntil: task.loopUntil ?? null,
      loopGroup: "",
      loopGroupTerminal: false,
      loopGroupIterations: 2,
      loopGroupStop: null,
      nodeType: "agent",
    })),
  };
}
