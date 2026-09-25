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

  let { name, done = false }: { name: string; done?: boolean } = $props();

  const Icon = $derived(ICONS[name] ?? Terminal);
  const display = $derived(name.replace(/_/g, " "));
</script>

<div class="chat-action-marker {done ? 'opacity-60' : 'text-[var(--text-secondary)]'}">
  {#if done}
    <Icon class="size-3.5 shrink-0" strokeWidth={1.75} />
  {:else}
    <Loader2 class="size-3.5 shrink-0 animate-spin" strokeWidth={2} />
  {/if}
  <span class="truncate">{t(done ? "tool.done" : "tool.running", { name: display })}</span>
</div>
