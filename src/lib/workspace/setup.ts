/** Whether first-run setup must be shown after login. */
export function needsDefaultAgentSetup(
  providers: readonly string[] | null | undefined,
  agent: unknown,
): boolean {
  return !Array.isArray(providers) || providers.length === 0 || !agent;
}
