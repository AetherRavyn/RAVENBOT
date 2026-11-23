<script lang="ts">
  import { t, type TranslationKey } from "$lib/i18n";
  import { summarizeRun, type RunEntry, type RunPhase } from "$lib/chat/runTimeline.svelte";
  import { Play, Wrench, Check, ShieldAlert, MessageCircleQuestion, Pause, Flag } from "@lucide/svelte";

  interface Props {
    events: RunEntry[];
    /** Resolve a bot id to its display name (room roster lives in the view). */
    nameFor: (botId: string) => string;
  }

  let { events, nameFor }: Props = $props();
  const summaries = $derived(summarizeRun(events));

  let stripEl = $state<HTMLDivElement | null>(null);

  const phaseIcon: Record<RunPhase, any> = {
    start: Play,
    tool: Wrench,
    "tool-done": Check,
    approval: ShieldAlert,
    question: MessageCircleQuestion,
    pause: Pause,
    done: Flag,
  };

  const phaseKey: Record<RunPhase, TranslationKey> = {
    start: "room.tlStart",
    tool: "room.tlTool",
    "tool-done": "room.tlToolDone",
    approval: "room.tlApproval",
    question: "room.tlQuestion",
    pause: "room.tlPause",
    done: "room.tlDone",
  };

  const phaseTone: Record<RunPhase, string> = {
    start: "text-[var(--brand-text)]",
    tool: "text-[var(--brand-text)]",
    "tool-done": "text-success",
    approval: "text-warning",
    question: "text-warning",
    pause: "text-warning",
    done: "text-success",
  };

  function clock(at: number): string {
    return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  // Newest event should always be visible while a run streams in.
  $effect(() => {
    events.length;
    requestAnimationFrame(() => {
      stripEl?.scrollTo({ left: stripEl.scrollWidth, behavior: "smooth" });
    });
  });
</script>

<div bind:this={stripEl} class="flex items-center gap-1.5 overflow-x-auto no-scrollbar px-4 py-1.5 border-b border-[var(--hairline)] bg-[var(--surface-1)] shrink-0">
  {#each summaries as summary (summary.botId)}
    {@const Icon = phaseIcon[summary.phase]}
    <span
      class="inline-flex items-center gap-1 h-6 pl-1.5 pr-2 rounded-full border border-[var(--hairline)] bg-[var(--surface-2)] text-[11px] whitespace-nowrap shrink-0"
      title={`${clock(summary.at)} · ${nameFor(summary.botId)} ${t(phaseKey[summary.phase])}${summary.detail ? ` · ${summary.detail}` : ""}`}
    >
      <Icon class="size-3 shrink-0 {phaseTone[summary.phase]}" />
      <span class="font-semibold text-[var(--text-secondary)]">{nameFor(summary.botId)}</span>
      <span class="text-[var(--text-muted)]">{t(phaseKey[summary.phase])}</span>
      {#if summary.detail}
        <span class="max-w-28 truncate font-mono text-[var(--text-tertiary)]">{summary.detail}</span>
      {/if}
      {#if summary.toolCalls > 0}
        <span class="rounded-full bg-[var(--surface-3)] px-1.5 tabular-nums text-[var(--text-muted)]" title={`${summary.toolsDone}/${summary.toolCalls} tool calls finished`}>
          {summary.toolsDone}/{summary.toolCalls}
        </span>
      {/if}
    </span>
  {/each}
</div>
