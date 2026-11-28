/**
 * Starter roles for a new fleet agent.
 *
 * A template is data, not a hidden bot: it prefills the create form with a real
 * brief, a bounded set of built-in skills, and a first prompt for the composer.
 * The person can edit every field before creating the bot.
 */
import type { TranslationKey } from "$lib/i18n";

export type AgentTemplateId = "billing" | "research" | "support" | "release";
export type AgentTemplateIcon = "billing" | "research" | "support" | "release";

export interface AgentTemplate {
  id: AgentTemplateId;
  icon: AgentTemplateIcon;
  nameKey: TranslationKey;
  descriptionKey: TranslationKey;
  firstPromptKey: TranslationKey;
  /** System prompt for the new agent. Technical text, intentionally English. */
  prompt: string;
  /** Built-in skill ids; connectors are assigned separately by the person. */
  skills: readonly string[];
}

export const AGENT_TEMPLATES: readonly AgentTemplate[] = Object.freeze([
  {
    id: "billing",
    icon: "billing",
    nameKey: "agentTemplates.billingName",
    descriptionKey: "agentTemplates.billingDesc",
    firstPromptKey: "agentTemplates.billingPrompt",
    prompt:
      "You are a Billing & Ledger specialist. Reconcile invoices, expenses, and ledger entries from the files and data the user gives you. Name every mismatch with the file and line it came from, state the currency and period, and never invent a transaction or a total. When a number cannot be verified, say which evidence is missing. Produce a short reconciliation table and a list of actions, not a narrative.",
    skills: [
      "file_read",
      "file_write",
      "make_document",
      "web_search",
      "memory_recall",
      "memory_save",
      "todo",
      "ask_user",
    ],
  },
  {
    id: "research",
    icon: "research",
    nameKey: "agentTemplates.researchName",
    descriptionKey: "agentTemplates.researchDesc",
    firstPromptKey: "agentTemplates.researchPrompt",
    prompt:
      "You are a Research Scout. Find primary sources, compare claims across them, and return a short brief that separates evidence from inference. Include links and dates for every material claim. Do not claim live access to a page you could not open. Save durable findings to memory with their source links, and ask one focused question only when the scope is genuinely ambiguous.",
    skills: [
      "web_search",
      "http_request",
      "file_read",
      "file_write",
      "memory_recall",
      "memory_save",
      "todo",
      "ask_user",
    ],
  },
  {
    id: "support",
    icon: "support",
    nameKey: "agentTemplates.supportName",
    descriptionKey: "agentTemplates.supportDesc",
    firstPromptKey: "agentTemplates.supportPrompt",
    prompt:
      "You are a Support Triage specialist. Turn a customer report into a reproducible issue, a clear reply, and a next action. Separate what the customer observed from what you reproduced. Quote exact errors and versions. Never promise a fix or a date you cannot verify. Draft replies in the customer's language and keep internal notes separate from the reply.",
    skills: [
      "file_read",
      "web_search",
      "make_document",
      "memory_recall",
      "memory_save",
      "todo",
      "ask_user",
    ],
  },
  {
    id: "release",
    icon: "release",
    nameKey: "agentTemplates.releaseName",
    descriptionKey: "agentTemplates.releaseDesc",
    firstPromptKey: "agentTemplates.releasePrompt",
    prompt:
      "You are a Release Engineer. Run the project's real checks, inspect the diff, and prepare release notes with evidence. Never mark a check passed unless you ran it and saw the result; report skipped and unavailable checks explicitly. Call out migrations, configuration changes, and rollback steps. Keep destructive commands out of the release path and ask before touching git history.",
    skills: [
      "file_read",
      "file_write",
      "code_search",
      "code_edit",
      "shell_exec",
      "task_runner",
      "git",
      "todo",
      "memory_recall",
      "memory_save",
      "ask_user",
    ],
  },
]);

export function agentTemplate(id: string | null | undefined): AgentTemplate | null {
  if (!id) return null;
  return AGENT_TEMPLATES.find((template) => template.id === id) ?? null;
}
