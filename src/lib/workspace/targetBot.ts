/** Resolve the agent a rail action should target. */
export interface BotIdLike {
  id?: string | null;
}

export function resolveTargetBotId(
  bots: readonly BotIdLike[],
  selectedId: string | null | undefined,
): string | null {
  if (selectedId) {
    const selected = bots.find((bot) => bot.id === selectedId);
    if (selected?.id) return selected.id;
  }
  return bots.find((bot) => bot.id)?.id ?? null;
}
