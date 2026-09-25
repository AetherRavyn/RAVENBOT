<script lang="ts">
  // Plan Mode run-graph preview: layers the task DAG left→right with SVG
  // edges. Positions come from dagLayout (pure, tested) so this stays a
  // dumb renderer.
  import { dagLayout, dagEdges, DAG_NODE_W, DAG_NODE_H, type DagTask } from "$lib/chat/dag";
  import { getDiceBearUrl } from "$lib/utils";

  interface MemberLite {
    bot_id: string;
    rank: string;
    bot?: { name?: string; avatar_url?: string; avatar_style?: string } | null;
  }

  interface Props {
    tasks: DagTask[];
    members: MemberLite[];
  }

  let { tasks, members }: Props = $props();

  let layout = $derived(dagLayout(tasks));
  let edges = $derived(dagEdges(tasks));

  function memberFor(botId: string) {
    return members.find((m) => m.bot_id === botId);
  }

  function nameFor(botId: string): string {
    const m = memberFor(botId);
    return m?.bot?.name || m?.rank || "?";
  }

  function avatarFor(botId: string): string {
    const m = memberFor(botId);
    return m?.bot?.avatar_url || getDiceBearUrl(nameFor(botId), m?.bot?.avatar_style || "bottts");
  }

  function edgePath(from: number, to: number): string {
    const a = layout.positions[from];
    const b = layout.positions[to];
    if (!a || !b) return "";
    const x1 = a.x + DAG_NODE_W;
    const y1 = a.y + DAG_NODE_H / 2;
    const x2 = b.x;
    const y2 = b.y + DAG_NODE_H / 2;
    const mid = (x2 - x1) / 2;
    return `M ${x1} ${y1} C ${x1 + mid} ${y1}, ${x2 - mid} ${y2}, ${x2} ${y2}`;
  }
</script>

{#if tasks.length > 0 && layout.width > 0}
  <div class="overflow-x-auto no-scrollbar pb-1">
    <div class="relative shrink-0" style="width: {layout.width}px; height: {layout.height}px">
      <svg
        class="absolute inset-0 pointer-events-none"
        width={layout.width}
        height={layout.height}
        aria-hidden="true"
      >
        <defs>
          <marker id="plan-dag-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 0 L 8 4 L 0 8 z" style="fill: var(--brand)" opacity="0.7" />
          </marker>
        </defs>
        {#each edges as e (e.from + "->" + e.to)}
          <path
            d={edgePath(e.from, e.to)}
            fill="none"
            style="stroke: var(--brand)"
            stroke-width="1.5"
            opacity="0.5"
            marker-end="url(#plan-dag-arrow)"
          />
        {/each}
      </svg>

      {#each tasks as task, i (i)}
        {@const pos = layout.positions[i]}
        {#if pos}
          <div
            class="absolute rounded-xl border bg-[var(--surface-2)] px-2 py-1.5 flex items-center gap-1.5"
            style="left: {pos.x}px; top: {pos.y}px; width: {DAG_NODE_W}px; height: {DAG_NODE_H}px; border-color: color-mix(in srgb, var(--brand) 35%, transparent)"
            title={task.label}
          >
            <img src={avatarFor(task.botId)} alt="" class="size-5 rounded-full object-cover shrink-0 border border-[var(--hairline)]" />
            <div class="min-w-0">
              <div class="text-[10px] font-bold text-[var(--text-primary)] truncate leading-tight">{nameFor(task.botId)}</div>
              <div class="text-[9px] text-[var(--text-muted)] truncate leading-tight">{task.label}</div>
            </div>
          </div>
        {/if}
      {/each}
    </div>
  </div>
{/if}
