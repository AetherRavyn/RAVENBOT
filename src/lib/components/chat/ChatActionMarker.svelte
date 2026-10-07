<script lang="ts">
  import {
    Boxes,
    Calendar,
    Globe,
    History,
    Loader2,
    MessageSquare,
    Pencil,
    Search,
    Share2,
    Sparkles,
    Terminal,
    FileText,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { summarizeArgs, fmtElapsed } from "$lib/chat/toolSummary";

  // OpenBot ChatActionMarker: one quiet mini-row per tool call in the live run.
  // Icons are per-tool; the verb phrase is generic and translated (P7 i18n).
  const ICONS: Record<string, typeof Search> = {
    web_search: Search,
    http_request: Globe,
    browser_navigate: Globe,
    browser_click: Globe,
    browser_screenshot: Globe,
    shell_exec: Terminal,
    file_read: FileText,
    file_write: Pencil,
    file_tree: Search,
    search_files: Search,
    code_search: Search,
    code_edit: Pencil,
    git: Boxes,
    docker: Boxes,
    delegate: Share2,
    ask_user: MessageSquare,
    memory_recall: History,
    memory_save: Sparkles,
    image_gen: Sparkles,
    calendar: Calendar,
  };

  /**
   * `args` is what the call is being asked to do.
   *
   * The name alone says "running `file_write`", which tells a watcher nothing;
   * the argument says "src/lib/app.rs", which is the entire reason the line is
   * on screen. Absent on the engine path, where only an id is reported, and then
   * the row degrades to the name rather than showing a stray separator.
   *
   * `startedAt` is a wall-clock epoch for a call still in flight, `ms` its
   * final duration once it is not. Between them the line always says how long
   * this has been going on — which is the first question anyone watching a run
   * asks, and the one a bare spinner refuses to answer. When neither is known
   * (the engine path reports only an id) the time is simply absent rather than
   * shown as zero, because `0s` next to a spinner is a lie.
   */
  let {
    name,
    done = false,
    args = null,
    startedAt = null,
    ms = null,
  }: {
    name: string;
    done?: boolean;
    args?: unknown;
    startedAt?: number | null;
    ms?: number | null;
  } = $props();

  const Icon = $derived(ICONS[name] ?? Terminal);
  const display = $derived(name.replace(/_/g, " "));
  const detail = $derived(summarizeArgs(args));

  /**
   * Ticks once a second while the call runs, and only then.
   *
   * One interval for this row, cleared when it finishes or unmounts — an
   * interval per historical marker would leave the list running timers long
   * after the run that produced it is gone.
   */
  let now = $state(Date.now());
  $effect(() => {
    if (done || !startedAt) return;
    now = Date.now();
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const elapsed = $derived(done ? ms : startedAt ? Math.max(0, now - startedAt) : null);
  const elapsedText = $derived(fmtElapsed(elapsed));
</script>

<div class="chat-action-marker {done ? 'opacity-60' : 'text-[var(--text-secondary)]'}">
  {#if done}
    <Icon class="size-3.5 shrink-0" strokeWidth={1.75} />
  {:else}
    <Loader2 class="size-3.5 shrink-0 animate-spin" strokeWidth={2} />
  {/if}
  <span class="truncate">{t(done ? "tool.done" : "tool.running", { name: display })}</span>
  {#if detail}
    <!-- The point of the line, so it gets its own span rather than being
         appended to the label — otherwise a long path pushes the verb off. -->
    <span class="chat-action-marker__args truncate">{detail}</span>
  {/if}
  {#if elapsedText}
    <!--
      Right-aligned and monospaced so a second's tick does not shove the
      arguments sideways. Tabular figures are the whole trick: without them
      every tick reflows the row and the line twitches once a second forever.
    -->
    <span class="chat-action-marker__time" data-done={done}>{elapsedText}</span>
  {/if}
</div>
