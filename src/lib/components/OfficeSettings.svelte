<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import * as Dialog from "$lib/components/ui/dialog";
  import { notify } from "$lib/toast";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import * as Card from "$lib/components/ui/card";
  import * as Tabs from "$lib/components/ui/tabs";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import * as Avatar from "$lib/components/ui/avatar";
  import { Separator } from "$lib/components/ui/separator";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import ConnectorIcon from "$lib/components/ConnectorIcon.svelte";
  import OfficeMemoryPanel from "$lib/components/OfficeMemoryPanel.svelte";
  import { getDiceBearUrl, OFFICE_TEMPLATES, dicebearStyles } from "$lib/utils";
  import { officeTemplateName, officeTemplateDesc } from "$lib/catalogI18n";
  import { cn } from "$lib/utils.js";
  import {
    Save,
    Users,
    Shield,
    FileText,
    DollarSign,
    Target,
    Trash2,
    Plus,
    FolderOpen,
    X,
    Brain,
    Settings,
    Layers,
    BookOpen,
    Laptop,
    TrendingUp,
    Briefcase,
    Palette,
    Building2,
    Check,
    Search,
    Sparkles,
    UserPlus,
    Key,
    Bot as BotIcon,
    Coins,
    Sliders,
    ArrowRight,
    Lock,
    ExternalLink,
    Clock,
    AlertTriangle,
    AlertCircle,
  } from "@lucide/svelte";

  interface Props {
    room: any;
    bots: any[];
    open: boolean;
    onClose: () => void;
    onUpdated: (room: any) => void;
    onDeleted?: () => void;
  }

  let { room, bots = [], open, onClose, onUpdated, onDeleted }: Props = $props();

  let activeTab = $state("general");
  let name = $state("");
  let description = $state("");
  let officeTemplate = $state("custom");
  let avatarUrl = $state("");
  let avatarStyle = $state("bottts");
  let goal = $state("");
  let policy = $state("");
  let terms = $state("");
  let budget = $state("");
  let projectFolders = $state<string[]>([]);
  let newFolder = $state("");
  let showAvatarPicker = $state(false);
  let members = $state<any[]>([]);
  let searchQuery = $state("");
  let newBotName = $state("");
  let newBotRank = $state("Member");
  let newBotSpecialty = $state("Generalist");
  let isSaving = $state(false);
  let saveSuccess = $state(false);
  let showDeleteConfirm = $state(false);
  let isDeleting = $state(false);

  // Office Tools Matrix State
  let botServersMap = $state<Record<string, Set<string>>>({});
  let allServers = $state<any[]>([]);
  let toolsLoading = $state(false);

  $effect(() => {
    if (room) {
      name = room.name || "";
      description = room.description || "";
      officeTemplate = room.office_template || "custom";
      avatarUrl = room.avatar_url || "";
      avatarStyle = room.avatar_style || "bottts";
      goal = room.goal || "";
      policy = room.policy || "";
      terms = room.terms || "";
      budget = room.budget?.toString() || "";
      projectFolders = Array.isArray(room.project_folders) ? [...room.project_folders] : [];
    }
  });

  $effect(() => {
    if (open && room?.id) {
      loadMembers();
      loadOfficeTools();
    }
  });

  // Pre-fill rank choices whenever officeTemplate changes
  $effect(() => {
    const tmpl = OFFICE_TEMPLATES[officeTemplate as keyof typeof OFFICE_TEMPLATES];
    if (tmpl && tmpl.ranks && tmpl.ranks.length > 0) {
      const unused = tmpl.ranks.find((r: any) => !members.some((m) => m.rank === r.rank));
      const fallback = tmpl.ranks[0];
      if (unused) {
        newBotRank = unused.rank;
        newBotSpecialty = unused.specialty;
      } else if (fallback) {
        newBotRank = fallback.rank;
        newBotSpecialty = fallback.specialty;
      }
    }
  });

  async function loadMembers() {
    try {
      const mems = await invoke("list_chatroom_members", { chatroomId: room.id });
      members = (mems as any[]).map((m: any) => ({
        ...m,
        bot: bots.find((b: any) => b.id === m.bot_id),
      }));
    } catch (e) {
      console.error("Failed to load members:", e);
    }
  }

  async function loadOfficeTools() {
    toolsLoading = true;
    try {
      allServers = await invoke("list_mcp_servers", { category: null });
      const map: Record<string, Set<string>> = {};
      for (const b of bots) {
        try {
          const assigned: string[] = await invoke("list_bot_mcp_servers", { botId: b.id });
          map[b.id] = new Set(assigned);
        } catch {
          map[b.id] = new Set();
        }
      }
      botServersMap = map;
    } catch (e) {
      console.error("Failed to load office tools:", e);
    } finally {
      toolsLoading = false;
    }
  }

  async function toggleBotTool(botId: string, serverId: string) {
    const isAssigned = botServersMap[botId]?.has(serverId);
    const nextState = !isAssigned;
    try {
      await invoke("toggle_bot_mcp_server", {
        botId,
        serverId,
        enabled: nextState,
      });
      const set = new Set(botServersMap[botId] || []);
      if (nextState) set.add(serverId);
      else set.delete(serverId);
      botServersMap = { ...botServersMap, [botId]: set };
    } catch (e) {
      alert("Failed to toggle connector: " + String(e));
    }
  }

  async function applyPresetToOffice(serverIds: string[]) {
    if (members.length === 0) return;
    try {
      for (const m of members) {
        if (!m.bot_id) continue;
        const currentList = Array.from(botServersMap[m.bot_id] || []);
        const combined = Array.from(new Set([...currentList, ...serverIds]));
        await invoke("batch_set_bot_mcp", { botId: m.bot_id, serverIds: combined });
      }
      await loadOfficeTools();
      alert("Preset stack successfully assigned to all office members!");
    } catch (e) {
      alert("Failed to assign stack to office: " + String(e));
    }
  }

  let filteredBots = $derived(
    bots.filter(
      (b: any) =>
        !members.some((m) => m.bot_id === b.id) &&
        b.name.toLowerCase().includes(searchQuery.toLowerCase())
    )
  );

  async function browseFolders() {
    try {
      const picked = await openDialog({
        directory: true,
        multiple: true,
        title: "Select project folder(s)",
        parent: true,
      });
      if (!picked) return;
      const list = Array.isArray(picked) ? picked : [picked];
      const next = [...projectFolders];
      for (const dir of list) if (dir && !next.includes(dir)) next.push(dir);
      projectFolders = next;
    } catch (e) {
      notify(`Could not open folder picker: ${String(e)}`, "error");
    }
  }

  function addFolder() {
    const value = newFolder.trim();
    if (!value || projectFolders.includes(value)) return;
    projectFolders = [...projectFolders, value];
    newFolder = "";
  }

  function removeFolder(idx: number) {
    projectFolders = projectFolders.filter((_, i) => i !== idx);
  }

  async function saveAll() {
    isSaving = true;
    try {
      const updated = {
        ...room,
        name: name.trim() || room.name,
        description: description.trim(),
        office_template: officeTemplate,
        avatar_url: avatarUrl || getDiceBearUrl(name || room.name, avatarStyle),
        avatar_style: avatarStyle,
        goal,
        policy,
        terms,
        budget: budget ? parseFloat(budget) : null,
        project_folders: projectFolders.filter((f) => f.trim()),
      };
      await invoke("update_chatroom", { room: updated });
      onUpdated(updated);
      notify("Office details saved", "success");
      saveSuccess = true;
      setTimeout(() => {
        saveSuccess = false;
      }, 2000);
    } catch (e) {
      notify(`Failed to save office settings: ${String(e)}`, "error");
    } finally {
      isSaving = false;
    }
  }

  async function addExistingBot(bot: any) {
    const tmpl = OFFICE_TEMPLATES[officeTemplate as keyof typeof OFFICE_TEMPLATES];
    const rankInfo =
      tmpl?.ranks.find((r: any) => !members.some((m) => m.rank === r.rank)) ||
      tmpl?.ranks[0] || { rank: "Member", specialty: "Generalist" };
    await invoke("add_member_to_chatroom", {
      chatroomId: room.id,
      botId: bot.id,
      rank: rankInfo.rank,
      specialty: rankInfo.specialty,
    });
    await loadMembers();
    await loadOfficeTools();
  }

  async function createAndAddBot() {
    if (!newBotName.trim()) return;
    const url = getDiceBearUrl(newBotName, avatarStyle);
    await invoke("create_bot_for_office", {
      name: newBotName.trim(),
      description: `Office member: ${newBotSpecialty}`,
      rank: newBotRank.trim() || "Member",
      specialty: newBotSpecialty.trim() || "Generalist",
      avatarUrl: url,
      avatarStyle,
      chatroomId: room.id,
    });
    newBotName = "";
    await loadMembers();
    await loadOfficeTools();
  }

  async function removeMember(botId: string) {
    await invoke("remove_chatroom_member", { chatroomId: room.id, botId });
    await loadMembers();
    await loadOfficeTools();
  }

  async function updateMemberRank(botId: string, rank: string, specialty: string) {
    await invoke("update_chatroom_member", { chatroomId: room.id, botId, rank, specialty });
    await loadMembers();
  }

  function handleDelete() {
    showDeleteConfirm = true;
  }

  async function executeDelete() {
    isDeleting = true;
    try {
      await invoke("delete_chatroom", { chatroomId: room.id });
      showDeleteConfirm = false;
      onDeleted?.();
      onClose();
    } catch (e) {
      alert("Failed to delete office: " + String(e));
    } finally {
      isDeleting = false;
    }
  }

  let previewUrl = $derived(avatarUrl || getDiceBearUrl(name || room?.name || "office", avatarStyle));

  // Template icon helper
  function getTemplateIcon(key: string) {
    switch (key) {
      case "rot-archive":
        return BookOpen;
      case "it-office":
        return Laptop;
      case "marketing":
        return TrendingUp;
      case "sales":
        return Briefcase;
      case "design":
        return Palette;
      default:
        return Building2;
    }
  }

  function getTemplateTint(key: string) {
    switch (key) {
      case "rot-archive":
        return "bg-destructive/10 border-destructive/40 text-destructive";
      case "it-office":
        return "bg-[var(--surface-2)] border-[var(--brand)]/40 text-[var(--brand-text)]";
      case "marketing":
        return "bg-info/10 border-info/40 text-info";
      case "sales":
        return "bg-warning/10 border-warning/40 text-warning";
      case "design":
        return "bg-success/10 border-success/40 text-success";
      default:
        return "bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]";
    }
  }

  const POLICY_TEMPLATES = [
    {
      title: "Autonomous Dev Protocol",
      content: "1. All code changes require automated compilation check\n2. Architecture changes must be agreed by Tech Lead\n3. Security and credentials must never be committed\n4. Daily milestone review at 09:00 UTC",
    },
    {
      title: "Marketing & Growth Rules",
      content: "1. Respect brand tone of voice in all copy\n2. Run A/B hypothesis test before scaling campaigns\n3. Check compliance and opt-out rules for messaging\n4. Monitor weekly conversion & engagement metrics",
    },
    {
      title: "Strict Budget Guardrails",
      content: "1. Hard-stop agent generation if office budget exceeds 95%\n2. Cache search queries and avoid redundant LLM calls\n3. Require operator confirmation for high-token external tasks",
    },
  ];

  const TERMS_TEMPLATES = [
    {
      title: "Confidential Workspace Terms",
      content: "All documents, codebases, memories, and task execution graphs generated within this office remain local to this machine. No proprietary context is broadcast to third-party tracking services.",
    },
    {
      title: "Multi-Agent Sovereign Boundary",
      content: "Agents in this office operate under distributed consensus. Task handoffs occur across the blackboard memory layer. Final release execution requires human-in-the-loop approval.",
    },
  ];
</script>

{#if open && room}
  <Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
    <Dialog.Content class="sm:max-w-5xl max-w-5xl w-[96vw] h-[88vh] flex flex-col p-0 overflow-hidden bg-[var(--surface-1)] border border-[var(--hairline-strong)] rounded-xl text-[var(--text-primary)] font-sans">
      <!-- Executive Header -->
      <Tabs.Root bind:value={activeTab} class="flex flex-col flex-1 min-h-0 w-full">
      <Dialog.Header class="px-6 pt-5 pb-4 border-b border-[var(--hairline)] shrink-0 bg-[var(--surface-2)]/80 ">
        <div class="flex items-center justify-between gap-4">
          <!-- Left: Avatar & Identity Meta -->
          <div class="flex items-center gap-3.5">
            <div class="relative group">
              <Avatar.Root class="size-13 rounded-2xl ring-2 ring-[var(--brand)]/40 overflow-hidden bg-[var(--surface-3)]">
                <Avatar.Image src={previewUrl} alt={name || room.name} class="size-full object-cover" />
                <Avatar.Fallback class="bg-[var(--brand-soft)] text-[var(--brand-text)] font-bold text-sm">
                  {(name || room.name).slice(0, 2).toUpperCase()}
                </Avatar.Fallback>
              </Avatar.Root>
              <button
                type="button"
                class="absolute inset-0 bg-black/60 rounded-2xl opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-[10px] font-bold text-[var(--text-primary)] cursor-pointer"
                aria-pressed={showAvatarPicker}
                onclick={() => (showAvatarPicker = !showAvatarPicker)}
              >
                {t("office.change")}
              </button>
            </div>

            <div>
              <div class="flex items-center gap-2 flex-wrap">
                <Dialog.Title class="text-base font-extrabold text-[var(--text-primary)] tracking-wide">
                  {t("room.settings")} — {name || room.name}
                </Dialog.Title>
                <Badge variant="outline" class="bg-[var(--brand-soft)] border-[var(--brand)]/40 text-[var(--brand-text)] text-[10px] font-mono font-semibold px-2 py-0.5">
                  {officeTemplateName(officeTemplate)}
                </Badge>
                <Badge variant="outline" class="bg-[var(--surface-2)] border-[var(--hairline-strong)] text-[var(--text-secondary)] text-[10px] font-mono px-2 py-0.5">
                  {members.length} {members.length === 1 ? t("office.agent1") : t("office.agentN")}
                </Badge>
              </div>
              <Dialog.Description class="text-xs text-[var(--text-tertiary)] mt-0.5">
                {t("office.domainDesc")}
              </Dialog.Description>
            </div>
          </div>
        </div>

        <!-- Navigation Tabs Bar with Crisp Lucide Icons -->
          <Tabs.List class="grid w-full grid-cols-8 bg-[var(--surface-2)] border border-[var(--hairline)] p-1 rounded-xl">
            <Tabs.Trigger value="general" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <Settings class="size-3.5" />
              <span>{t("office.tabGeneral")}</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="members" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <Users class="size-3.5" />
              <span>{t("office.tabMembers")} ({members.length})</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="memory" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <Brain class="size-3.5 text-info" />
              <span>{t("office.tabMemory")}</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="tools" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <Layers class="size-3.5 text-[var(--brand-text)]" />
              <span>{t("office.tabTools")}</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="policy" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <Shield class="size-3.5 text-[var(--brand-text)]" />
              <span>{t("office.tabPolicy")}</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="terms" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <FileText class="size-3.5 text-warning" />
              <span>{t("office.tabTerms")}</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="budget" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <Coins class="size-3.5 text-success" />
              <span>{t("office.tabBudget")}</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="goal" class="gap-1.5 text-xs data-[state=active]:bg-[var(--brand)]/40 data-[state=active]:text-[var(--text-on-light)] data-[state=active]:font-bold text-[var(--text-tertiary)]">
              <Target class="size-3.5 text-danger" />
              <span>{t("office.tabGoal")}</span>
            </Tabs.Trigger>
          </Tabs.List>
      </Dialog.Header>

      <!-- Scrollable Tab Content Viewport -->
      <div class="flex-1 min-h-0 overflow-y-auto no-scrollbar px-6 py-4 overscroll-contain scroll-smooth">
          <!-- TAB 1: GENERAL & TEMPLATES -->
          <Tabs.Content value="general" class="space-y-4">
            <!-- Identity Details -->
            <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3.5 ">
              <div class="flex items-center justify-between">
                <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                  <Settings class="size-3.5 text-[var(--brand-text)]" />
                  <span>{t("office.identity")}</span>
                </h4>
                <Button
                  size="sm"
                  variant="outline"
                  class="h-7 text-xs bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer"
                  aria-pressed={showAvatarPicker}
                  onclick={() => (showAvatarPicker = !showAvatarPicker)}
                >
                  <Palette class="size-3.5 mr-1" />
                  <span>{showAvatarPicker ? t("office.avatarClose") : t("office.avatarCustomize")}</span>
                </Button>
              </div>

              <div class="grid grid-cols-1 md:grid-cols-3 gap-4 items-start">
                <div class="flex flex-col items-center justify-center p-4 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] gap-2">
                  <img
                    src={previewUrl}
                    alt={name}
                    class="size-20 rounded-2xl object-cover ring-2 ring-[var(--brand)]/40 shadow-lg"
                  />
                  <span class="text-[11px] text-[var(--text-tertiary)] font-mono">Style: {avatarStyle}</span>
                </div>

                <div class="md:col-span-2 space-y-3">
                  <div class="space-y-1.5">
                    <Label for="office-name" class="text-xs font-bold text-[var(--text-secondary)]">{t("office.name")}</Label>
                    <Input
                      id="office-name"
                      bind:value={name}
                      placeholder={t("office.namePh")}
                      class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)] focus-visible:border-[var(--brand)]"
                    />
                  </div>

                  <div class="space-y-1.5">
                    <Label for="office-desc" class="text-xs font-bold text-[var(--text-secondary)]">{t("office.mission")}</Label>
                    <Textarea
                      id="office-desc"
                      bind:value={description}
                      placeholder={t("office.missionPh")}
                      rows={2}
                      class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)] focus-visible:border-[var(--brand)] min-h-[60px]"
                    />
                  </div>
                </div>
              </div>

              <!-- Project folders: where this office's work happens -->
              <div class="pt-4 mt-1 border-t border-[var(--hairline)] space-y-2.5">
                <div class="flex items-center justify-between">
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <FolderOpen class="size-3.5 text-[var(--brand-text)]" />
                    <span>{t("office.folders")}</span>
                  </h4>
                  <span class="text-[10px] text-[var(--text-muted)]">{projectFolders.length} configured</span>
                </div>
                <p class="text-[11px] text-[var(--text-muted)] leading-relaxed">
                  {t("office.foldersConfinement")}
                </p>

                <div class="space-y-1.5">
                  {#each projectFolders as folder, idx}
                    <div class="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-[var(--surface-2)] border border-[var(--hairline)]">
                      <FolderOpen class="size-3.5 text-[var(--text-muted)] shrink-0" />
                      <span class="flex-1 min-w-0 text-[11px] font-mono text-[var(--text-secondary)] truncate" title={folder}>{folder}</span>
                      <button type="button" class="text-[var(--text-muted)] hover:text-danger cursor-pointer shrink-0" aria-label="Remove folder" onclick={() => removeFolder(idx)} title={t("office.removeFolder")}>✕</button>
                    </div>
                  {:else}
                    <div class="text-[11px] text-[var(--text-muted)] italic px-1">{t("office.foldersEmpty")}</div>
                  {/each}
                </div>

                <div class="flex items-center gap-2">
                  <Input
                    bind:value={newFolder}
                    aria-label="Project folder path"
                    placeholder="/home/you/projects/my-app"
                    class="h-8 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
                    onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); addFolder(); } }}
                  />
                  <Button size="sm" variant="outline" class="h-8 shrink-0 gap-1.5 border-[var(--hairline)]" onclick={addFolder}>
                    <Plus class="size-3.5" /> Add
                  </Button>
                  <Button size="sm" variant="outline" class="h-8 shrink-0 gap-1.5 border-[var(--hairline)]" onclick={browseFolders} title={t("office.browse")}>
                    <FolderOpen class="size-3.5" /> Browse…
                  </Button>
                </div>
              </div>

              {#if showAvatarPicker}
                <div class="pt-3 border-t border-[var(--hairline)]">
                  <AvatarPicker
                    seed={name || room.name}
                    style={avatarStyle}
                    customUrl={avatarUrl && !avatarUrl.includes("dicebear") ? avatarUrl : null}
                    onSelect={(url, style) => {
                      avatarUrl = url;
                      avatarStyle = style;
                      showAvatarPicker = false;
                    }}
                  />
                </div>
              {/if}
            </div>

            <!-- Office Template Selection with Vector Icons -->
            <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Building2 class="size-3.5 text-[var(--brand-text)]" />
                    <span>{t("office.domain")}</span>
                  </h4>
                  <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
                    {t("office.domainDesc")}
                  </p>
                </div>
              </div>

              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                {#each Object.entries(OFFICE_TEMPLATES) as [key, tmpl]}
                  {@const isSelected = officeTemplate === key}
                  {@const IconComponent = getTemplateIcon(key)}
                  {@const tintClass = getTemplateTint(key)}
                  <button
                    type="button"
                    aria-pressed={isSelected}
                    onclick={() => (officeTemplate = key)}
                    class={cn(
 "text-left rounded-2xl border p-3.5 transition-all flex flex-col justify-between gap-2.5 cursor-pointer relative group",
                      isSelected
                        ? "border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]"
                        : "border-[var(--hairline)] bg-[var(--surface-2)]/80 hover:border-[var(--brand)]/40 hover:bg-[var(--surface-2)]"
                    )}
                  >
                    <div class="flex items-start justify-between gap-2">
                      <div class="flex items-center gap-2.5">
                        <div class={cn("size-9 rounded-xl border flex items-center justify-center shadow-md", tintClass)}>
                          <IconComponent class="size-5" />
                        </div>
                        <div>
                          <span class="font-bold text-xs text-[var(--text-primary)] block">{officeTemplateName(key)}</span>
                          <span class="text-[10px] text-[var(--brand-text)]/90 font-mono">{t("office.ranksN", { n: tmpl.ranks.length })}</span>
                        </div>
                      </div>

                      <div class={cn("size-5 rounded-md border flex items-center justify-center shrink-0 transition-colors", isSelected ? "bg-[var(--brand)] border-[var(--brand)] text-[var(--text-on-light)]" : "border-[var(--hairline-strong)] bg-[var(--surface-2)]")}>
                        {#if isSelected}
                          <Check class="size-3.5 font-bold" />
                        {/if}
                      </div>
                    </div>

                    <p class="text-[11px] text-[var(--text-tertiary)] line-clamp-2 leading-relaxed">
                      {officeTemplateDesc(key)}
                    </p>

                    <!-- Preview of rank chips -->
                    {#if tmpl.ranks.length > 0}
                      <div class="flex flex-wrap gap-1 pt-1.5 border-t border-[var(--hairline)]">
                        {#each tmpl.ranks.slice(0, 3) as r}
                          <span class="text-[9px] px-1.5 py-[2px] rounded bg-[var(--surface-2)] text-[var(--text-secondary)] font-mono border border-[var(--hairline)]">
                            {r.rank}
                          </span>
                        {/each}
                        {#if tmpl.ranks.length > 3}
                          <span class="text-[9px] px-1 py-[2px] rounded text-[var(--text-muted)] font-mono">
                            +{tmpl.ranks.length - 3}
                          </span>
                        {/if}
                      </div>
                    {/if}
                  </button>
                {/each}
              </div>
            </div>
          </Tabs.Content>

          <!-- TAB 2: MEMBERS ROSTER -->
          <Tabs.Content value="members" class="space-y-4">
            <div class="grid grid-cols-1 lg:grid-cols-3 gap-4 items-start">
              <!-- Left 2 Cols: Active Roster -->
              <div class="lg:col-span-2 space-y-3">
                <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
                  <div class="flex items-center justify-between">
                    <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                      <Users class="size-3.5 text-[var(--brand-text)]" />
                      <span>{t("office.roster")} ({members.length})</span>
                    </h4>
                    <span class="text-[11px] text-[var(--text-tertiary)]">{t("office.fleetTag")}</span>
                  </div>

                  <div class="space-y-2 max-h-[380px] overflow-y-auto pr-1">
                    {#each members as m (m.bot_id)}
                      <div class="flex items-center justify-between p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] gap-3 hover:border-[var(--hairline-strong)] transition-all">
                        <div class="flex items-center gap-3 min-w-0">
                          <Avatar.Root class="size-9 rounded-xl ring-1 ring-[var(--brand)]/30 shrink-0">
                            <Avatar.Image
                              src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "bottts")}
                              alt={m.bot?.name || m.rank}
                            />
                            <Avatar.Fallback class="bg-[var(--brand-soft)] text-[var(--brand-text)] text-xs">
                              {(m.bot?.name || m.rank).slice(0, 2)}
                            </Avatar.Fallback>
                          </Avatar.Root>

                          <div class="min-w-0">
                            <div class="flex items-center gap-2">
                              <span class="font-bold text-xs text-[var(--text-primary)] truncate">{m.bot?.name || m.rank}</span>
                              <Badge variant="outline" class="text-[9px] px-1.5 py-0 bg-[var(--brand-soft)] border-[var(--brand)]/30 text-[var(--brand-text)] font-mono">
                                {m.rank}
                              </Badge>
                            </div>
                            <p class="text-[11px] text-[var(--text-tertiary)] truncate mt-0.5">{m.specialty}</p>
                          </div>
                        </div>

                        <div class="flex items-center gap-2 shrink-0">
                          <Button
                            variant="ghost"
                            size="sm"
                            class="size-7 p-0 text-[var(--text-tertiary)] hover:text-danger hover:bg-danger/10 cursor-pointer rounded-lg"
                            aria-label="Remove agent"
                            title={t("office.removeAgent")}
                            onclick={() => removeMember(m.bot_id)}
                          >
                            <Trash2 class="size-3.5" />
                          </Button>
                        </div>
                      </div>
                    {:else}
                      <div class="py-12 px-4 text-center border border-dashed border-[var(--hairline)] rounded-xl text-[var(--text-muted)] space-y-2">
                        <Users class="size-8 mx-auto opacity-30 text-[var(--brand-text)]" />
                        <p class="text-xs">{t("office.noAgents")}</p>
                        <p class="text-[11px] text-[var(--text-muted)]">{t("office.noAgentsHint")}</p>
                      </div>
                    {/each}
                  </div>
                </div>
              </div>

              <!-- Right 1 Col: Provision / Add Agents -->
              <div class="space-y-3">
                <!-- Add Existing -->
                <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Search class="size-3.5 text-[var(--brand-text)]" />
                    <span>{t("office.addExisting")}</span>
                  </h4>

                  <div class="relative">
                    <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
                    <Input
                      bind:value={searchQuery}
                      aria-label="Search fleet"
                      placeholder={t("office.searchFleet")}
                      class="pl-8 h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)]"
                    />
                  </div>

                  <div class="space-y-1.5 max-h-36 overflow-y-auto">
                    {#each filteredBots as b (b.id)}
                      <button
                        type="button"
                        class="w-full flex items-center justify-between p-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--brand)]/40 hover:bg-[var(--surface-3)] text-left cursor-pointer transition-all"
                        onclick={() => addExistingBot(b)}
                      >
                        <div class="flex items-center gap-2 min-w-0">
                          <Avatar.Root class="size-6.5 rounded-lg shrink-0">
                            <Avatar.Image src={b.avatar_url || getDiceBearUrl(b.name, b.avatar_style || "bottts")} />
                            <Avatar.Fallback class="text-[10px]">{b.name.slice(0, 2)}</Avatar.Fallback>
                          </Avatar.Root>
                          <span class="text-xs font-bold text-[var(--text-primary)] truncate">{b.name}</span>
                        </div>
                        <span class="text-[10px] font-mono text-[var(--brand-text)] shrink-0 font-semibold">+ Assign</span>
                      </button>
                    {:else}
                      <p class="text-[11px] text-[var(--text-muted)] text-center py-3">
                        {searchQuery ? "No matching available bots" : "All fleet bots already in office"}
                      </p>
                    {/each}
                  </div>
                </div>

                <!-- Create New Inline -->
                <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <UserPlus class="size-3.5 text-[var(--brand-text)]" />
                    <span>{t("office.provisionNew")}</span>
                  </h4>

                  <div class="space-y-2">
                    <Input
                      bind:value={newBotName}
                      aria-label="Agent name"
                      placeholder={t("office.phName")}
                      class="h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)]"
                    />
                    <Input
                      bind:value={newBotRank}
                      aria-label="Agent rank"
                      placeholder={t("office.phRank")}
                      class="h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)]"
                    />
                    <Input
                      bind:value={newBotSpecialty}
                      aria-label="Agent specialty"
                      placeholder={t("office.phSpecialty")}
                      class="h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)]"
                    />

                    <Button
                      size="sm"
                      class="w-full h-8 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium gap-1.5 cursor-pointer shadow-sm"
                      disabled={!newBotName.trim()}
                      onclick={createAndAddBot}
                    >
                      <Plus class="size-3.5" />
                      <span>{t("office.provisionAdd")}</span>
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          </Tabs.Content>

          <!-- TAB 3: SHARED MEMORY -->
          <Tabs.Content value="memory" class="space-y-4">
            <OfficeMemoryPanel chatroomId={room.id} />
          </Tabs.Content>

          <!-- TAB 4: OFFICE CONNECTORS & TOOLS MATRIX -->
          <Tabs.Content value="tools" class="space-y-4">
            <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3.5 ">
              <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-[var(--hairline)]">
                <div>
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Layers class="size-3.5 text-[var(--brand-text)]" />
                    <span>{t("office.matrix")}</span>
                  </h4>
                  <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
                    Assign and synchronize 135+ Model Context Protocol tools across all member agents in this office.
                  </p>
                </div>

                <!-- 1-Click Preset Stacks for Office -->
                <div class="flex items-center gap-1.5 flex-wrap">
                  <Button
                    size="sm"
                    variant="outline"
                    class="h-7 text-[11px] bg-[var(--surface-2)] border-[var(--brand)]/30 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer"
                    onclick={() => applyPresetToOffice(["github", "postgres", "redis", "docker", "sentry", "shell", "filesystem"])}
                  >
                    <span>+ Full-Stack Dev Stack</span>
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    class="h-7 text-[11px] bg-[var(--surface-2)] border-[var(--brand)]/30 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer"
                    onclick={() => applyPresetToOffice(["duckdb", "arxiv", "wikipedia", "wolfram_alpha", "brave_search", "openai"])}
                  >
                    <span>+ Data/AI Research Stack</span>
                  </Button>
                </div>
              </div>

              <!-- Members Tool Matrix Table -->
              {#if members.length > 0}
                <div class="space-y-3">
                  {#each members as m (m.bot_id)}
                    {@const botAssigned = botServersMap[m.bot_id] || new Set()}
                    <div class="p-3.5 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] space-y-2.5">
                      <div class="flex items-center justify-between">
                        <div class="flex items-center gap-2.5">
                          <Avatar.Root class="size-7 rounded-lg ring-1 ring-[var(--brand)]/30">
                            <Avatar.Image src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, "bottts")} />
                            <Avatar.Fallback>{(m.bot?.name || m.rank).slice(0, 2)}</Avatar.Fallback>
                          </Avatar.Root>
                          <div>
                            <span class="font-bold text-xs text-[var(--text-primary)]">{m.bot?.name || m.rank}</span>
                            <span class="text-[10px] text-[var(--brand-text)] font-mono ml-1.5">({m.rank})</span>
                          </div>
                        </div>
                        <Badge variant="outline" class="text-[10px] font-mono bg-[var(--brand-soft)] border-[var(--brand)]/30 text-[var(--brand-text)]">
                          {botAssigned.size} tools active
                        </Badge>
                      </div>

                      <!-- Tool Chips for this Bot -->
                      <div class="flex flex-wrap gap-1.5">
                        {#each allServers.slice(0, 16) as s (s.id)}
                          {@const isEnabled = botAssigned.has(s.id)}
                          <button
                            type="button"
                            aria-pressed={isEnabled}
                            class={cn(
 "px-2 py-1 rounded-lg text-[10px] font-mono transition-all flex items-center gap-1.5 cursor-pointer border",
                              isEnabled
                                ? "bg-[var(--brand)] text-[var(--text-on-light)] border-[var(--brand)] font-bold shadow-sm"
                                : "bg-[var(--surface-3)] text-[var(--text-tertiary)] border-[var(--hairline)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)]"
                            )}
                            onclick={() => toggleBotTool(m.bot_id, s.id)}
                            title={isEnabled ? `Click to disable ${s.name}` : `Click to enable ${s.name}`}
                          >
                            <ConnectorIcon id={s.id} name={s.name} size="sm" class="!size-3.5 border-none shadow-none" />
                            <span>{s.id}</span>
                          </button>
                        {/each}
                      </div>
                    </div>
                  {/each}
                </div>
              {:else}
                <div class="py-10 text-center text-[var(--text-muted)] border border-dashed border-[var(--hairline)] rounded-xl">
                  <p class="text-xs">{t("office.matrixEmpty")}</p>
                </div>
              {/if}
            </div>
          </Tabs.Content>

          <!-- TAB 5: OFFICE POLICY & STANDARDS -->
          <Tabs.Content value="policy" class="space-y-4">
            <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Shield class="size-3.5 text-[var(--brand-text)]" />
                    <span>{t("office.policyTitle")}</span>
                  </h4>
                  <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
                    {t("office.policyDesc")}
                  </p>
                </div>
              </div>

              <!-- Quick Templates -->
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-[10px] font-mono text-[var(--text-muted)] uppercase">{t("office.presets")}</span>
                {#each POLICY_TEMPLATES as pt}
                  <button
                    type="button"
                    class="text-[10px] font-mono bg-[var(--surface-3)] hover:bg-[var(--brand-soft)] border border-[var(--hairline)] hover:border-[var(--brand)]/40 text-[var(--brand-text)] px-2 py-0.5 rounded-md cursor-pointer transition-colors"
                    onclick={() => (policy = pt.content)}
                  >
                    + {pt.title}
                  </button>
                {/each}
              </div>

              <Textarea
                bind:value={policy}
                aria-label="Office policy"
                placeholder={t("office.policyPh")}
                rows={9}
                class="font-mono text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] focus-visible:border-[var(--brand)] leading-relaxed"
              />
            </div>
          </Tabs.Content>

          <!-- TAB 6: TERMS & CONDITIONS -->
          <Tabs.Content value="terms" class="space-y-4">
            <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <FileText class="size-3.5 text-warning" />
                    <span>{t("office.termsTitle")}</span>
                  </h4>
                  <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
                    {t("office.termsDesc")}
                  </p>
                </div>
              </div>

              <!-- Quick Templates -->
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-[10px] font-mono text-[var(--text-muted)] uppercase">{t("office.presets")}</span>
                {#each TERMS_TEMPLATES as tt}
                  <button
                    type="button"
                    class="text-[10px] font-mono bg-[var(--surface-3)] hover:bg-warning/40 border border-[var(--hairline)] hover:border-warning/40 text-warning px-2 py-0.5 rounded-md cursor-pointer transition-colors"
                    onclick={() => (terms = tt.content)}
                  >
                    + {tt.title}
                  </button>
                {/each}
              </div>

              <Textarea
                bind:value={terms}
                aria-label="Office terms and conditions"
                placeholder={t("office.termsPh")}
                rows={9}
                class="font-mono text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] focus-visible:border-[var(--brand)] leading-relaxed"
              />
            </div>
          </Tabs.Content>

          <!-- TAB 7: BUDGET & TOKEN ALLOCATION -->
          <Tabs.Content value="budget" class="space-y-4">
            <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Coins class="size-3.5 text-success" />
                    <span>{t("office.budgetTitle")}</span>
                  </h4>
                  <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
                    {t("office.budgetDesc")}
                  </p>
                </div>
              </div>

              <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 items-center">
                <div class="space-y-1.5">
                  <Label for="office-budget" class="text-xs font-bold text-[var(--text-secondary)]">{t("office.budgetLabel")}</Label>
                  <div class="relative">
                    <span class="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)] font-mono text-xs">$</span>
                    <Input
                      id="office-budget"
                      type="number"
                      bind:value={budget}
                      placeholder="e.g. 500"
                      class="pl-7 h-9 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-primary)]"
                    />
                  </div>
                </div>

                <div class="p-3 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] text-xs text-[var(--text-tertiary)] space-y-1">
                  <div class="flex justify-between">
                    <span>{t("office.ceiling")}</span>
                    <span class="font-bold text-success">{budget ? t("office.hardStop", { v: `$${budget}` }) : t("office.uncapped")}</span>
                  </div>
                  <div class="flex justify-between text-[11px] text-[var(--text-muted)]">
                    <span>{t("office.share")}</span>
                    <span>{members.length > 0 ? t("office.perAgent", { v: `$${((budget ? parseFloat(budget) : 0) / members.length).toFixed(2)}` }) : "N/A"}</span>
                  </div>
                </div>
              </div>

              {#if members.length > 0}
                <div class="space-y-2 pt-2 border-t border-[var(--hairline)]">
                  <Label class="text-xs font-bold text-[var(--text-secondary)]">{t("office.roleDist")}</Label>
                  {#each members as m}
                    <div class="flex items-center justify-between p-2.5 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)]">
                      <div class="flex items-center gap-2.5">
                        <Avatar.Root class="size-6.5 rounded-lg">
                          <Avatar.Image src={m.bot?.avatar_url || getDiceBearUrl(m.rank, "bottts")} />
                          <Avatar.Fallback class="text-[10px]">{m.rank.slice(0, 2)}</Avatar.Fallback>
                        </Avatar.Root>
                        <div>
                          <span class="font-bold text-xs text-[var(--text-primary)]">{m.bot?.name || m.rank}</span>
                          <span class="text-[10px] text-[var(--text-tertiary)] font-mono ml-1.5">— {m.rank}</span>
                        </div>
                      </div>
                      <Badge variant="outline" class="text-[10px] font-mono text-success border-success/30 bg-success/40">
                        {m.specialty}
                      </Badge>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </Tabs.Content>

          <!-- TAB 8: QUARTERLY GOAL -->
          <Tabs.Content value="goal" class="space-y-4">
            <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3 ">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-[var(--text-primary)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Target class="size-3.5 text-danger" />
                    <span>{t("office.goalTitle")}</span>
                  </h4>
                  <p class="text-[11px] text-[var(--text-tertiary)] mt-0.5">
                    {t("office.goalDesc")}
                  </p>
                </div>
              </div>

              <Textarea
                bind:value={goal}
                aria-label="Quarterly goal"
                placeholder={t("office.goalPh")}
                rows={7}
                class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] focus-visible:border-[var(--brand)] leading-relaxed"
              />

              <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/30 flex items-center gap-3">
                <Sparkles class="size-4 text-[var(--brand-text)] shrink-0" />
                <p class="text-[11px] text-[var(--brand-text)] leading-relaxed">
                  {t("office.goalHint")}
                </p>
              </div>
            </div>
          </Tabs.Content>
      </div>

      <!-- Unified Executive Footer (Single Save Changes Bar) -->
      <div class="px-6 py-3.5 border-t border-[var(--hairline)] flex items-center justify-between bg-[var(--surface-2)]/90 shrink-0">
        <Button
          variant="outline"
          class="h-8.5 text-xs bg-danger/10 border-danger/40 text-danger hover:bg-danger/20 hover:text-danger cursor-pointer gap-1.5 transition-colors"
          onclick={handleDelete}
        >
          <Trash2 class="size-3.5" />
          <span>{t("office.del")}</span>
        </Button>

        <div class="flex items-center gap-2.5">
          <Button
            variant="outline"
            class="h-8.5 text-xs border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--hairline)] cursor-pointer"
            onclick={onClose}
          >
            {t("ui.cancel")}
          </Button>

          <Button
            class={cn(
 "h-8.5 text-xs font-medium gap-1.5 cursor-pointer transition-all",
              saveSuccess
                ? "bg-success/15 border border-success/40 text-success hover:bg-success/25"
                : "bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)]"
            )}
            disabled={isSaving}
            onclick={saveAll}
          >
            {#if saveSuccess}
              <Check class="size-3.5" />
              <span>{t("office.saved")}</span>
            {:else}
              <Save class="size-3.5" />
              <span>{isSaving ? t("ui.saving") : t("office.saveChanges")}</span>
            {/if}
          </Button>
        </div>
      </div>
      </Tabs.Root>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Custom Executive Delete Confirmation Modal -->
{#if showDeleteConfirm}
  <Dialog.Root open={showDeleteConfirm} onOpenChange={(o) => !o && (showDeleteConfirm = false)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[var(--surface-1)] border border-danger/40 rounded-xl p-6 text-[var(--text-primary)] flex flex-col gap-4 font-sans select-none z-50">
      <div class="flex items-start gap-3.5">
        <div class="size-11 rounded-2xl bg-danger/15 border border-danger/40 flex items-center justify-center text-danger shrink-0 ">
          <AlertTriangle class="size-6 animate-pulse" />
        </div>
        <div class="space-y-1">
          <Dialog.Title class="text-base font-extrabold text-[var(--text-primary)]">
            {t("office.delConfirmTitle")}
          </Dialog.Title>
          <Dialog.Description class="text-xs text-[var(--text-tertiary)] leading-relaxed">
            {t("office.delConfirmDesc", { name: name || room?.name })}
          </Dialog.Description>
        </div>
      </div>

      <div class="p-3 rounded-xl bg-danger/10 border border-danger/30 text-[11px] text-danger font-mono flex items-center gap-2">
        <AlertCircle class="size-4 text-danger shrink-0" />
        <span>{t("office.permanent")}</span>
      </div>

      <div class="flex items-center justify-end gap-2.5 pt-2 border-t border-[var(--hairline)]">
        <Button
          variant="outline"
          class="h-8.5 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] hover:text-[var(--text-primary)] cursor-pointer"
          onclick={() => (showDeleteConfirm = false)}
          disabled={isDeleting}
        >
          {t("office.keepOffice")}
        </Button>

        <Button
          class="h-8.5 text-xs bg-danger/15 border border-danger/40 text-danger hover:bg-danger/25 font-bold gap-1.5 cursor-pointer"
          disabled={isDeleting}
          onclick={executeDelete}
        >
          <Trash2 class="size-3.5" />
          <span>{isDeleting ? t("office.deleting") : t("office.permDelete")}</span>
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}
