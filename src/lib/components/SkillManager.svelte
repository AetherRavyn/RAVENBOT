<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { getDiceBearUrl } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import PluginsStore from "$lib/components/PluginsStore.svelte";
  import McpManager from "$lib/components/McpManager.svelte";
  import {
    Wrench,
    Globe,
    FileText,
    FileEdit,
    Terminal,
    Share2,
    Save,
    Lock,
    FolderTree,
    Code2,
    Globe2,
    ListTodo,
    Video,
    BookOpen,
    Calendar,
    Container,
    Plug,
    Layers,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    open: boolean;
    onClose: () => void;
    onUpdated: (bot: any) => void;
  }

  let { bot, open, onClose, onUpdated }: Props = $props();

  // Dynamic — 24 native tools (9 original + 15 lane closers). Fetches from Rust so new skills appear without UI code change.
  let availableSkills = $state<any[]>([
    { id: "web_search", name: "Web Search", description: "Search the web for real-time information", icon: Globe, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Network access"] },
    { id: "file_read", name: "File Read", description: "Inspect workspace files", icon: FileText, color: "text-success bg-success/40 border-success/40", permissions: ["Read filesystem"] },
    { id: "file_write", name: "File Write", description: "Create and modify files", icon: FileEdit, color: "text-warning bg-warning/40 border-warning/40", permissions: ["Write filesystem"] },
    { id: "shell_exec", name: "Shell Execute", description: "Execute bash commands", icon: Terminal, color: "text-red-400 bg-red-950/40 border-red-800/40", permissions: ["Shell execution"] },
    { id: "delegate", name: "Bot Delegation", description: "Delegate to peer bots", icon: Share2, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Fleet IPC"] },
    { id: "code_search", name: "Code Search", description: "Ripgrep-like codebase search", icon: Wrench, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Read filesystem"] },
    { id: "git", name: "Git", description: "commit/branch/pr/diff/log", icon: FileEdit, color: "text-orange-400 bg-orange-950/40 border-orange-800/40", permissions: ["Shell", "Git"] },
    { id: "browser_navigate", name: "Browser Navigate", description: "Navigate/click/fill/screenshot", icon: Globe, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Screenshot", "Network"] },
    { id: "db_query", name: "DB Query", description: "SELECT on local SQLite", icon: FileText, color: "text-success bg-success/40 border-success/40", permissions: ["Read filesystem"] },
    { id: "tavily_search", name: "Tavily Search", description: "Research-grade search", icon: Globe2, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Network"] },
    { id: "memory_save", name: "Memory Save", description: "Save fact/voice", icon: Save, color: "text-pink-400 bg-pink-950/40 border-pink-800/40", permissions: [] },
    { id: "memory_recall", name: "Memory Recall", description: "Recall relevant memories", icon: Save, color: "text-fuchsia-400 bg-[var(--surface-2)]uchsia-950/40 border-fuchsia-800/40", permissions: [] },
    { id: "file_tree", name: "File Tree", description: "List directory tree", icon: FolderTree, color: "text-warning bg-warning/40 border-warning/40", permissions: ["Read filesystem"] },
    { id: "code_edit", name: "Code Edit", description: "Apply unified diff patch", icon: Code2, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Write filesystem","Shell"] },
    { id: "http_request", name: "HTTP Request", description: "GET/POST any URL", icon: Globe2, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Network"] },
    { id: "todo", name: "Todo", description: "Local todo per bot", icon: ListTodo, color: "text-yellow-400 bg-yellow-950/40 border-yellow-800/40", permissions: [] },
    { id: "youtube_transcript", name: "YouTube", description: "Get video transcript", icon: Video, color: "text-red-400 bg-red-950/40 border-red-800/40", permissions: ["Network"] },
    { id: "arxiv_search", name: "ArXiv", description: "Search research papers", icon: BookOpen, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Network"] },
    { id: "calendar", name: "Calendar", description: "Create/list local events", icon: Calendar, color: "text-green-400 bg-green-950/40 border-green-800/40", permissions: [] },
    { id: "docker", name: "Docker", description: "ps/images/build/run", icon: Container, color: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", permissions: ["Shell"] },
  ]);

  $effect(() => {
    if (open) {
      invoke("list_all_skills").then((skills: any) => {
        if (Array.isArray(skills) && skills.length > 0) {
          const iconMap: Record<string, any> = { web_search: Globe, file_read: FileText, file_write: FileEdit, shell_exec: Terminal, delegate: Share2, screenshot: Globe, analyze_image: FileText, voice_input: Terminal, voice_output: Terminal, code_search: Wrench, git: FileEdit, browser_navigate: Globe, db_query: FileText, tavily_search: Globe2, memory_save: Save, memory_recall: Save, file_tree: FolderTree, code_edit: Code2, http_request: Globe2, todo: ListTodo, youtube_transcript: Video, arxiv_search: BookOpen, calendar: Calendar, docker: Container };
          const colorMap: Record<string, string> = { code_search: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", git: "text-orange-400 bg-orange-950/40 border-orange-800/40", browser_navigate: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", db_query: "text-success bg-success/40 border-success/40", tavily_search: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", file_tree: "text-warning bg-warning/40 border-warning/40", code_edit: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", http_request: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", todo: "text-yellow-400 bg-yellow-950/40 border-yellow-800/40", youtube_transcript: "text-red-400 bg-red-950/40 border-red-800/40", arxiv_search: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", calendar: "text-green-400 bg-green-950/40 border-green-800/40", docker: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40" };
          availableSkills = skills.map((s: any) => ({
            id: s.id,
            name: s.name,
            description: s.description,
            icon: iconMap[s.id] || Wrench,
            color: colorMap[s.id] || "text-[var(--text-tertiary)] bg-[var(--surface-2)] border-[var(--hairline)]",
            permissions: s.permissions || [],
          }));
        }
      }).catch(() => {});
    }
  });

  let enabledSkills = $state<Set<string>>(new Set());
  let isSaving = $state(false);
  let showPlugins = $state(false);
  let showMcp = $state(false);

  $effect(() => {
    if (bot) {
      enabledSkills = new Set(bot.skills || []);
    }
  });

  function toggleSkill(skillId: string) {
    const next = new Set(enabledSkills);
    if (next.has(skillId)) {
      next.delete(skillId);
    } else {
      next.add(skillId);
    }
    enabledSkills = next;
  }

  async function save() {
    if (!bot) return;
    isSaving = true;

    const updatedBot = {
      ...bot,
      skills: Array.from(enabledSkills),
      updated_at: new Date().toISOString(),
    };

    try {
      await invoke("update_bot", { bot: updatedBot });
      onUpdated(updatedBot);
      onClose();
    } catch (e) {
      console.error("Failed to update skills:", e);
    } finally {
      isSaving = false;
    }
  }
</script>

{#if open && bot}
  <Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
    <Dialog.Content class="sm:max-w-xl max-h-[85vh] overflow-y-auto bg-[var(--surface-1)] border border-[var(--brand)]/30 ]  rounded-xl">
      <Dialog.Header class="flex flex-row items-center gap-3 pb-3 border-[var(--hairline)] border-[var(--hairline)]">
        <div class="size-10 rounded-full overflow-hidden bg-[var(--surface-3)] border-[var(--hairline)] border-[var(--brand)]/40 p-0.5 shadow-md">
          <img
            src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "avataaars")}
            alt={bot.name}
            class="size-full rounded-full object-cover"
          />
        </div>
        <div class="flex-1">
          <Dialog.Title class="text-base font-bold flex items-center gap-2 text-white">
            <span>Tool Capabilities: {bot.name}</span>
          </Dialog.Title>
          <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
            9 built-ins + 1000+ plugins — every plugin appears native to every model
          </Dialog.Description>
        </div>
        <div class="flex gap-1.5 flex-wrap">
          <Button size="sm" variant="outline" class="shrink-0 gap-1.5 border-[var(--brand)]/40 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer" onclick={() => (showPlugins = true)}>
            <Plug class="size-3.5" />
            <span>1000+ Plugins</span>
          </Button>
          <Button size="sm" variant="outline" class="shrink-0 gap-1.5 border-success/40 text-success hover:bg-success/30 cursor-pointer" onclick={() => (showMcp = true)}>
            <span>🔧</span>
            <span>100+ MCP</span>
          </Button>
          <Button size="sm" class="shrink-0 gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-white cursor-pointer shadow-sm" onclick={() => { onClose(); window.dispatchEvent(new CustomEvent("open-connectors")); }}>
            <Layers class="size-3.5" />
            <span>Connectors Hub</span>
          </Button>
        </div>
      </Dialog.Header>

      <div class="space-y-3 py-3">
        {#each availableSkills as skill}
          {@const isEnabled = enabledSkills.has(skill.id)}
          {@const IconComponent = skill.icon}
          <div
            class={cn(
 "rounded-2xl border p-3.5 transition-all flex flex-col gap-2.5",
              isEnabled
                ? "border-[var(--brand)]/60 bg-[var(--brand-soft)] ]"
                : "border-[var(--hairline)] bg-[var(--surface-1)]/80 hover:border-[var(--hairline-strong)]"
            )}
          >
            <div class="flex items-start justify-between gap-3">
              <div class="flex items-start gap-3">
                <div class={cn("size-9 rounded-xl flex items-center justify-center shrink-0 border", skill.color)}>
                  <IconComponent class="size-4.5" />
                </div>
                <div>
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-bold text-white">{skill.name}</span>
                    {#if isEnabled}
                      <span class="text-[9px] bg-[var(--brand-soft)] text-[var(--brand-text)] border border-[var(--brand)]/40 py-0.2 px-1.5 rounded font-mono font-bold">
                        ENABLED
                      </span>
                    {/if}
                  </div>
                  <p class="text-xs text-[var(--text-tertiary)] mt-0.5 leading-relaxed">
                    {skill.description}
                  </p>
                </div>
              </div>

              <!-- Toggle switch button -->
              <button
                type="button"
                role="switch"
                aria-checked={isEnabled}
                aria-label={`Toggle ${skill.name}`}
                onclick={() => toggleSkill(skill.id)}
                class={cn(
 "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-[var(--hairline)] border-transparent transition-colors duration-200 ease-in-out focus:outline-none mt-1",
                  isEnabled ? "bg-[var(--brand)] " : "bg-[var(--hairline)]"
                )}
              >
                <span
                  class={cn(
 "pointer-events-none inline-block size-5 transform rounded-full bg-white shadow-lg ring-0 transition duration-200 ease-in-out",
                    isEnabled ? "translate-x-5" : "translate-x-0"
                  )}
                ></span>
              </button>
            </div>

            <!-- Permissions list -->
            <div class="flex items-center gap-1.5 flex-wrap pt-1.5 border-t border-[var(--hairline)] text-[11px]">
              <span class="text-[var(--text-muted)] flex items-center gap-1 text-[10px]">
                <Lock class="size-3 text-[var(--brand-text)]" />
                Security scopes:
              </span>
              {#each skill.permissions as p}
                <span class="text-[10px] bg-[var(--surface-2)] text-[var(--text-tertiary)] border border-[var(--hairline)] px-1.5 py-0.2 rounded font-mono">
                  {p}
                </span>
              {/each}
            </div>
          </div>
        {/each}
      </div>

      <div class="flex justify-end gap-2 pt-3 border-t border-[var(--hairline)]">
        <Button variant="outline" size="sm" class="bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={onClose} disabled={isSaving}>
          Cancel
        </Button>
        <Button size="sm" class="gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-white font-medium shadow-md " onclick={save} disabled={isSaving}>
          <Save class="size-3.5" />
          {isSaving ? "Saving..." : "Save Tool Permissions"}
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
  <PluginsStore bot={bot} open={showPlugins} onClose={() => (showPlugins = false)} />
  <McpManager bot={bot} open={showMcp} onClose={() => (showMcp = false)} />
{/if}
