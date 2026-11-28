import { describe, expect, it } from "vitest";
import { AGENT_TEMPLATES, agentTemplate } from "./agentTemplates";

describe("agent templates", () => {
  it("offers four distinct roles with real prompts", () => {
    expect(AGENT_TEMPLATES.map((template) => template.id)).toEqual([
      "billing",
      "research",
      "support",
      "release",
    ]);
    expect(new Set(AGENT_TEMPLATES.map((template) => template.id)).size).toBe(4);
    for (const template of AGENT_TEMPLATES) {
      expect(template.prompt.length).toBeGreaterThan(120);
      expect(template.skills.length).toBeGreaterThan(3);
      expect(new Set(template.skills).size).toBe(template.skills.length);
      expect(template.nameKey.startsWith("agentTemplates.")).toBe(true);
    }
  });

  it("uses built-in skill ids, not invented tools", () => {
    const known = new Set([
      "file_read",
      "file_write",
      "make_document",
      "web_search",
      "http_request",
      "memory_recall",
      "memory_save",
      "todo",
      "ask_user",
      "code_search",
      "code_edit",
      "shell_exec",
      "task_runner",
      "git",
    ]);
    for (const template of AGENT_TEMPLATES) {
      for (const skill of template.skills) expect(known.has(skill), skill).toBe(true);
    }
  });

  it("resolves by id and rejects an unknown template", () => {
    expect(agentTemplate("research")?.id).toBe("research");
    expect(agentTemplate("nope")).toBeNull();
    expect(agentTemplate(null)).toBeNull();
  });
});
