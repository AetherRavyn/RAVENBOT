<script lang="ts">
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { getDiceBearUrl } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import PluginsStore from "$lib/components/PluginsStore.svelte";
  import { t, type TranslationKey } from "$lib/i18n";
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

  // Native-tool catalog: product built-ins ship translated names/descriptions
  // under `skills.catalog.<id>.*`. Skills absent from the locale files (the
  // ~1,500 "awesome" community entries, future additions) fall through to the
  // raw backend strings by design.
  function catalogText(id: string, field: "name" | "desc", fallback: string): string {
    const key = `skills.catalog.${id}.${field}`;
    const v = t(key as TranslationKey);
    return v === key ? fallback : v;
  }

  // The backend sends the real `Permission` wire shape — one object per variant,
  // e.g. `{ FileSystem: { paths: ["/"] } }` — so the variant is read straight
  // off the key. It used to send `format!("{:?}", p)` and this had to pull the
  // variant name back out of a Debug string, which is what a structured
  // transport exists to avoid.
  function permLabel(p: any): string {
    const variant = Object.keys(p ?? {})[0] ?? "";
    const key = `skills.perm.${variant}`;
    const v = t(key as TranslationKey);
    return v === key ? (variant || String(p)) : v;
  }

  let availableSkills = $state<any[]>([]);

  $effect(() => {
    if (open) {
      invoke("list_all_skills").then((skills: any) => {
        if (Array.isArray(skills) && skills.length > 0) {
          const iconMap: Record<string, any> = { web_search: Globe, file_read: FileText, file_write: FileEdit, shell_exec: Terminal, delegate: Share2, screenshot: Globe, analyze_image: FileText, voice_input: Terminal, voice_output: Terminal, code_search: Wrench, git: FileEdit, browser_navigate: Globe, db_query: FileText, tavily_search: Globe2, memory_save: Save, memory_recall: Save, file_tree: FolderTree, code_edit: Code2, http_request: Globe2, todo: ListTodo, youtube_transcript: Video, arxiv_search: BookOpen, calendar: Calendar, docker: Container };
          const colorMap: Record<string, string> = { web_search: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", file_read: "text-success bg-success/10 border-success/40", file_write: "text-warning bg-warning/10 border-warning/40", shell_exec: "text-danger bg-danger/10 border-danger/40", delegate: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", code_search: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", git: "text-warning bg-warning/10 border-warning/40", browser_navigate: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", db_query: "text-success bg-success/10 border-success/40", tavily_search: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", file_tree: "text-warning bg-warning/10 border-warning/40", code_edit: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", http_request: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", todo: "text-warning bg-warning/10 border-warning/40", youtube_transcript: "text-danger bg-danger/10 border-danger/40", arxiv_search: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40", calendar: "text-success bg-success/10 border-success/40", docker: "text-[var(--brand-text)] bg-[var(--brand-soft)] border-[var(--brand)]/40" };
          availableSkills = skills.map((s: any) => ({
            id: s.id,
            rawName: s.name,
            rawDesc: s.description,
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
    <Dialog.Content class="sm:max-w-xl max-h-[85vh] overflow-y-auto no-scrollbar">
      <Dialog.Header class="flex flex-row items-center gap-3 pb-3 pr-8 border-b border-[var(--hairline)]">
        <div class="size-10 shrink-0 rounded-full overflow-hidden border border-[var(--hairline)] bg-[var(--surface-2)] p-0.5">
          <RavenAvatar name={bot.name} imageUrl={bot.avatar_url} />
        </div>
        <div class="flex-1 min-w-0">
          <Dialog.Title class="text-sm font-semibold text-[var(--text-primary)] truncate">
            {t("skills.capabilitiesTitle")} — {bot.name}
          </Dialog.Title>
          <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
            {t("skills.description2")}
          </Dialog.Description>
        </div>
        <div class="flex gap-1.5 flex-wrap shrink-0">
          <button
            type="button"
            class="h-7 px-2.5 rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] text-xs font-medium text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] flex items-center gap-1.5 cursor-pointer"
            onclick={() => (showPlugins = true)}
          >
            <Plug class="size-3.5" />
            <span>{t("ui.plugins")}</span>
          </button>
          <button
            type="button"
            class="h-7 px-2.5 rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] text-xs font-medium text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] flex items-center gap-1.5 cursor-pointer"
            onclick={() => { onClose(); window.dispatchEvent(new CustomEvent("open-connectors", { detail: { botId: bot.id } })); }}
          >
            <Layers class="size-3.5" />
            <span>{t("ui.mcp")}</span>
          </button>
        </div>
      </Dialog.Header>

      <div class="space-y-2 py-3">
        {#if availableSkills.length === 0}
          <p class="py-6 text-center text-xs text-[var(--text-muted)]">{t("ui.loading")}</p>
        {/if}
        {#each availableSkills as skill (skill.id)}
          {@const isEnabled = enabledSkills.has(skill.id)}
          {@const IconComponent = skill.icon}
          {@const skillName = catalogText(skill.id, "name", skill.rawName)}
          <div
            class={cn(
              "rounded-xl border p-3 flex items-center gap-3",
              isEnabled
                ? "border-[var(--brand-strong)] bg-[var(--brand-soft)]"
                : "border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--hairline-strong)]"
            )}
          >
            <div class={cn("size-9 shrink-0 rounded-lg flex items-center justify-center border", skill.color)}>
              <IconComponent class="size-[18px]" />
            </div>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2 min-w-0">
                <span class="text-sm font-medium text-[var(--text-primary)] truncate">{skillName}</span>
                {#if isEnabled}
                  <span class="text-[10px] font-mono px-1.5 py-px rounded border border-[var(--brand-strong)] bg-[var(--brand-soft)] text-[var(--brand-text)] shrink-0">
                    {t("ui.on")}
                  </span>
                {/if}
              </div>
              <p class="text-xs text-[var(--text-tertiary)] mt-0.5 truncate">{catalogText(skill.id, "desc", skill.rawDesc)}</p>
              {#if skill.permissions.length}
                <div class="flex items-center gap-1.5 flex-wrap pt-1">
                  {#each skill.permissions as p, i (i)}
                    <span class="text-[10px] font-mono bg-[var(--surface-3)] text-[var(--text-tertiary)] border border-[var(--hairline)] px-1.5 py-px rounded">
                      {permLabel(p)}
                    </span>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Toggle switch -->
            <button
              type="button"
              role="switch"
              aria-checked={isEnabled}
              aria-label={t("skills.toggleSkill", { name: skillName })}
              onclick={() => toggleSkill(skill.id)}
              class={cn(
                "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full transition-colors duration-200 ease-in-out focus:outline-none",
                isEnabled ? "bg-[var(--brand)]" : "bg-[var(--surface-4)]"
              )}
            >
              <span
                class={cn(
                  "pointer-events-none inline-block size-5 transform rounded-full bg-[var(--surface-light)] shadow ring-0 transition duration-200 ease-in-out",
                  isEnabled ? "translate-x-5" : "translate-x-0.5"
                )}
              ></span>
            </button>
          </div>
        {/each}
      </div>

      <div class="flex justify-end gap-2 pt-3 border-t border-[var(--hairline)]">
        <Button variant="outline" size="sm" class="border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] cursor-pointer" onclick={onClose} disabled={isSaving}>
          {t("ui.cancel")}
        </Button>
        <Button size="sm" class="gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium cursor-pointer" onclick={save} disabled={isSaving}>
          <Save class="size-3.5" />
          {isSaving ? t("ui.saving") : t("ui.save")}
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
  <PluginsStore bot={bot} open={showPlugins} onClose={() => (showPlugins = false)} />
{/if}
