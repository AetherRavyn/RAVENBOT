<script lang="ts">
  /**
   * The compact view of an office plan above the composer.
   *
   * The full board is a rail (or the top of the column on a narrow window).
   * This is the one-line answer while the reader is typing: how much is done,
   * what is running now, and whether anything failed. Opening it lists the
   * nodes, and a node with a live thread opens that thread.
   */
  import { t } from "$lib/i18n";
  import {
    CheckCircle2,
    ChevronDown,
    Circle,
    Loader2,
    MinusCircle,
    XCircle,
  } from "@lucide/svelte";
  import type { BoardNode } from "./OfficeBoard.svelte";

  interface Props {
    nodes: BoardNode[];
    nameFor: (botId: string) => string;
    onOpen?: (node: BoardNode) => void;
  }

  let { nodes, nameFor, onOpen }: Props = $props();
  let open = $state(false);

  const done = $derived(nodes.filter((node) => node.state === "done" || node.state === "skipped").length);
  const failed = $derived(nodes.filter((node) => node.state === "failed").length);
  const active = $derived(
    nodes.find((node) => node.state === "running") ??
      nodes.find((node) => node.state === "pending"),
  );
  const complete = $derived(nodes.length > 0 && done === nodes.length);
  const ringRadius = 7.25;
  const ringLength = 2 * Math.PI * ringRadius;

  function iconFor(node: BoardNode) {
    if (node.state === "running") return Loader2;
    if (node.state === "done") return CheckCircle2;
    if (node.state === "failed") return XCircle;
    if (node.state === "skipped") return MinusCircle;
    return Circle;
  }
</script>

{#if nodes.length > 0}
  <section class="office-task-card" data-open={open ? "" : undefined} aria-label={t("room.tasks")}>
    <button
      type="button"
      class="office-task-card__head"
      aria-expanded={open}
      onclick={() => (open = !open)}
    >
      <span class="office-task-card__ring" aria-hidden="true">
        {#if complete}
          <CheckCircle2 class="size-4 text-success" strokeWidth={1.8} />
        {:else}
          <svg viewBox="0 0 16 16" fill="none" width="16" height="16">
            <circle cx="8" cy="8" r={ringRadius} class="office-task-card__ring-track" />
            <circle
              cx="8"
              cy="8"
              r={ringRadius}
              class="office-task-card__ring-value"
              stroke-dasharray={ringLength}
              stroke-dashoffset={ringLength * (1 - done / Math.max(nodes.length, 1))}
              transform="rotate(-90 8 8)"
            />
          </svg>
        {/if}
      </span>
      <span class="office-task-card__title">{t("room.tasks")}</span>
      <span class="office-task-card__count">{t("room.board.progress", { done, total: nodes.length })}</span>
      {#if failed > 0}
        <span class="office-task-card__failed">{failed}</span>
      {:else if active && !open}
        <span class="office-task-card__active">{active.label}</span>
      {/if}
      <ChevronDown class={"office-task-card__chev " + (open ? "office-task-card__chev--open" : "")} />
    </button>

    {#if open}
      <ol class="office-task-card__list">
        {#each nodes as node (node.nodeId)}
          {@const Icon = iconFor(node)}
          <li>
            <button
              type="button"
              class="office-task-card__item"
              data-state={node.state}
              title={node.nodeThreadId ? t("room.openThread") : undefined}
              onclick={() => {
                if (node.nodeThreadId) onOpen?.(node);
              }}
            >
              <Icon
                class={"size-3.5 shrink-0 office-task-card__icon " + (node.state === "running" ? "animate-spin" : "")}
                strokeWidth={1.9}
              />
              <span class="office-task-card__label">{node.label}</span>
              <span class="office-task-card__agent">{nameFor(node.botId)}</span>
            </button>
          </li>
        {/each}
      </ol>
    {/if}
  </section>
{/if}
