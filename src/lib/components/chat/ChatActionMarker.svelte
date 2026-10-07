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
  import { summarizeArgs } from "$lib/chat/toolSummary";

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
   */
  let { name, done = false, args = null }: { name: string; done?: boolean; args?: unknown } = $props();

  const Icon = $derived(ICONS[name] ?? Terminal);
  const display = $derived(name.replace(/_/g, " "));
  const detail = $derived(summarizeArgs(args));
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
</div>
