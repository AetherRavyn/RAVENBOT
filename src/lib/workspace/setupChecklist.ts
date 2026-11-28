/**
 * First-run setup facts.
 *
 * Onboarding asks for a provider and The Raven; this checklist answers the
 * quieter question on Home: which of those is actually done, and is there an
 * office to work in? It is deliberately pure so the three booleans can be
 * tested without a Tauri host.
 */

export type SetupStepId = "provider" | "agent" | "office";

export interface SetupFacts {
  /** Providers with a saved key, from `get_configured_providers`. */
  providers: readonly string[] | null | undefined;
  /** The default agent, from `get_default_agent`. */
  agent: { id?: string; config?: { model_provider?: string | null } | null } | null | undefined;
  /** Models returned by `probe_ollama`; an empty list means no local model. */
  ollamaModels: readonly unknown[] | null | undefined;
  officeCount: number;
}

export interface SetupStep {
  id: SetupStepId;
  done: boolean;
}

/**
 * Whether a model path exists.
 *
 * A cloud key is not the only valid answer, and a local agent is not done just
 * because some unrelated provider key is saved. When an agent already names
 * Ollama/local, require a local model; otherwise either a key or a local model
 * is enough for the setup step.
 */
export function hasModelPath(facts: SetupFacts): boolean {
  const providers = Array.isArray(facts.providers) ? facts.providers : [];
  const localModels = Array.isArray(facts.ollamaModels) ? facts.ollamaModels : [];
  const provider = facts.agent?.config?.model_provider ?? "";
  const localAgent = provider === "ollama" || provider === "local";
  if (localAgent) return localModels.length > 0;
  return providers.length > 0 || localModels.length > 0;
}

export function setupSteps(facts: SetupFacts): SetupStep[] {
  return [
    { id: "provider", done: hasModelPath(facts) },
    { id: "agent", done: Boolean(facts.agent?.id) },
    { id: "office", done: facts.officeCount > 0 },
  ];
}

/** The first incomplete step, in the order a person should do them. */
export function nextSetupStep(steps: readonly SetupStep[]): SetupStepId | null {
  return steps.find((step) => !step.done)?.id ?? null;
}

export function setupComplete(steps: readonly SetupStep[]): boolean {
  return steps.every((step) => step.done);
}
