<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
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
      };
      await invoke("update_chatroom", { room: updated });
      onUpdated(updated);
      saveSuccess = true;
      setTimeout(() => {
        saveSuccess = false;
      }, 2000);
    } catch (e) {
      alert("Failed to save office settings: " + String(e));
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

  function getTemplateGradient(key: string) {
    switch (key) {
      case "rot-archive":
        return "from-red-950/50 to-amber-950/30 border-red-500/40 text-red-300";
      case "it-office":
        return "from-purple-950/50 to-blue-950/30 border-purple-500/40 text-purple-300";
      case "marketing":
        return "from-pink-950/50 to-orange-950/30 border-pink-500/40 text-pink-300";
      case "sales":
        return "from-amber-950/50 to-yellow-950/30 border-amber-500/40 text-amber-300";
      case "design":
        return "from-fuchsia-950/50 to-indigo-950/30 border-fuchsia-500/40 text-fuchsia-300";
      default:
        return "from-emerald-950/50 to-teal-950/30 border-emerald-500/40 text-emerald-300";
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
    <Dialog.Content class="sm:max-w-5xl max-w-5xl w-[96vw] h-[88vh] flex flex-col p-0 overflow-hidden bg-[#0c0c16] border border-purple-500/30 rounded-3xl shadow-[0_0_60px_rgba(147,51,234,0.25)] text-zinc-100 font-sans">
      <!-- Executive Header -->
      <Dialog.Header class="px-6 pt-5 pb-4 border-b border-white/10 shrink-0 bg-[#10101f]/80 backdrop-blur-xl">
        <div class="flex items-center justify-between gap-4">
          <!-- Left: Avatar & Identity Meta -->
          <div class="flex items-center gap-3.5">
            <div class="relative group">
              <Avatar.Root class="size-13 rounded-2xl ring-2 ring-purple-500/40 shadow-[0_0_20px_rgba(168,85,247,0.25)] overflow-hidden bg-[#161628]">
                <Avatar.Image src={previewUrl} alt={name || room.name} class="size-full object-cover" />
                <Avatar.Fallback class="bg-purple-950 text-purple-200 font-bold text-sm">
                  {(name || room.name).slice(0, 2).toUpperCase()}
                </Avatar.Fallback>
              </Avatar.Root>
              <button
                type="button"
                class="absolute inset-0 bg-black/60 rounded-2xl opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-[10px] font-bold text-white cursor-pointer"
                onclick={() => (showAvatarPicker = !showAvatarPicker)}
              >
                Change
              </button>
            </div>

            <div>
              <div class="flex items-center gap-2 flex-wrap">
                <Dialog.Title class="text-base font-extrabold text-white tracking-wide">
                  Office Settings — {name || room.name}
                </Dialog.Title>
                <Badge variant="outline" class="bg-purple-950/70 border-purple-500/40 text-purple-300 text-[10px] font-mono font-semibold px-2 py-0.5">
                  {OFFICE_TEMPLATES[officeTemplate as keyof typeof OFFICE_TEMPLATES]?.name || "Custom Office"}
                </Badge>
                <Badge variant="outline" class="bg-black/40 border-zinc-700 text-zinc-300 text-[10px] font-mono px-2 py-0.5">
                  {members.length} {members.length === 1 ? "Agent" : "Agents"}
                </Badge>
              </div>
              <Dialog.Description class="text-xs text-zinc-400 mt-0.5">
                Configure autonomous workspace team roster, shared memory, tools & MCP dispatch, and governance standards.
              </Dialog.Description>
            </div>
          </div>
        </div>

        <!-- Navigation Tabs Bar with Crisp Lucide Icons -->
        <Tabs.Root bind:value={activeTab} class="w-full mt-3.5">
          <Tabs.List class="grid w-full grid-cols-8 bg-black/40 border border-[#202034] p-1 rounded-xl">
            <Tabs.Trigger value="general" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <Settings class="size-3.5" />
              <span>General</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="members" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <Users class="size-3.5" />
              <span>Members ({members.length})</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="memory" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <Brain class="size-3.5 text-pink-400" />
              <span>Memory</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="tools" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <Layers class="size-3.5 text-purple-400" />
              <span>Tools/MCP</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="policy" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <Shield class="size-3.5 text-cyan-400" />
              <span>Policy</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="terms" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <FileText class="size-3.5 text-amber-400" />
              <span>Terms</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="budget" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <Coins class="size-3.5 text-emerald-400" />
              <span>Budget</span>
            </Tabs.Trigger>
            <Tabs.Trigger value="goal" class="gap-1.5 text-xs data-[state=active]:bg-purple-600/40 data-[state=active]:text-white data-[state=active]:font-bold text-zinc-400">
              <Target class="size-3.5 text-red-400" />
              <span>Goal</span>
            </Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
      </Dialog.Header>

      <!-- Scrollable Tab Content Viewport -->
      <div class="flex-1 min-h-0 overflow-y-auto px-6 py-4 overscroll-contain scroll-smooth">
        <Tabs.Root bind:value={activeTab} class="w-full">
          <!-- TAB 1: GENERAL & TEMPLATES -->
          <Tabs.Content value="general" class="space-y-4">
            <!-- Identity Details -->
            <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3.5 backdrop-blur-md">
              <div class="flex items-center justify-between">
                <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                  <Settings class="size-3.5 text-purple-400" />
                  <span>Office Identity</span>
                </h4>
                <Button
                  size="sm"
                  variant="outline"
                  class="h-7 text-xs bg-[#161628] border-[#29293e] text-purple-300 hover:bg-purple-950/40 cursor-pointer"
                  onclick={() => (showAvatarPicker = !showAvatarPicker)}
                >
                  <Palette class="size-3.5 mr-1" />
                  <span>{showAvatarPicker ? "Close Avatar Picker" : "Customize Avatar"}</span>
                </Button>
              </div>

              <div class="grid grid-cols-1 md:grid-cols-3 gap-4 items-start">
                <div class="flex flex-col items-center justify-center p-4 rounded-xl bg-[#141424] border border-[#222238] gap-2">
                  <img
                    src={previewUrl}
                    alt={name}
                    class="size-20 rounded-2xl object-cover ring-2 ring-purple-500/40 shadow-lg"
                  />
                  <span class="text-[11px] text-zinc-400 font-mono">Style: {avatarStyle}</span>
                </div>

                <div class="md:col-span-2 space-y-3">
                  <div class="space-y-1.5">
                    <Label for="office-name" class="text-xs font-bold text-zinc-200">Office Name</Label>
                    <Input
                      id="office-name"
                      bind:value={name}
                      placeholder="e.g. Comic verse / Core Engineering"
                      class="h-9 text-xs bg-[#141424] border-[#25253c] text-white focus-visible:border-purple-500"
                    />
                  </div>

                  <div class="space-y-1.5">
                    <Label for="office-desc" class="text-xs font-bold text-zinc-200">Description & Mission</Label>
                    <Textarea
                      id="office-desc"
                      bind:value={description}
                      placeholder="What is this office team responsible for? (e.g. Full-stack product roadmap, branding, multi-agent automated testing)"
                      rows={2}
                      class="text-xs bg-[#141424] border-[#25253c] text-white focus-visible:border-purple-500 min-h-[60px]"
                    />
                  </div>
                </div>
              </div>

              {#if showAvatarPicker}
                <div class="pt-3 border-t border-white/5">
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
            <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Building2 class="size-3.5 text-purple-400" />
                    <span>Office Domain Template</span>
                  </h4>
                  <p class="text-[11px] text-zinc-400 mt-0.5">
                    Select a structured template to automatically configure team ranks, permissions, and specializations.
                  </p>
                </div>
              </div>

              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                {#each Object.entries(OFFICE_TEMPLATES) as [key, tmpl]}
                  {@const isSelected = officeTemplate === key}
                  {@const IconComponent = getTemplateIcon(key)}
                  {@const gradientClass = getTemplateGradient(key)}
                  <button
                    type="button"
                    onclick={() => (officeTemplate = key)}
                    class={cn(
                      "text-left rounded-2xl border p-3.5 transition-all flex flex-col justify-between gap-2.5 cursor-pointer relative group",
                      isSelected
                        ? "border-purple-400 bg-purple-950/40 ring-1 ring-purple-400 shadow-[0_0_20px_rgba(168,85,247,0.25)]"
                        : "border-[#202034] bg-[#121222]/80 hover:border-purple-500/40 hover:bg-[#16162c]"
                    )}
                  >
                    <div class="flex items-start justify-between gap-2">
                      <div class="flex items-center gap-2.5">
                        <div class={cn("size-9 rounded-xl border flex items-center justify-center shadow-md bg-gradient-to-br", gradientClass)}>
                          <IconComponent class="size-5" />
                        </div>
                        <div>
                          <span class="font-bold text-xs text-white block">{tmpl.name}</span>
                          <span class="text-[10px] text-purple-300/90 font-mono">{tmpl.ranks.length} Ranks Defined</span>
                        </div>
                      </div>

                      <div class={cn("size-5 rounded-md border flex items-center justify-center shrink-0 transition-colors", isSelected ? "bg-purple-600 border-purple-400 text-white" : "border-zinc-700 bg-black/40")}>
                        {#if isSelected}
                          <Check class="size-3.5 font-bold" />
                        {/if}
                      </div>
                    </div>

                    <p class="text-[11px] text-zinc-400 line-clamp-2 leading-relaxed">
                      {tmpl.description}
                    </p>

                    <!-- Preview of rank chips -->
                    {#if tmpl.ranks.length > 0}
                      <div class="flex flex-wrap gap-1 pt-1.5 border-t border-white/5">
                        {#each tmpl.ranks.slice(0, 3) as r}
                          <span class="text-[9px] px-1.5 py-0.2 rounded bg-black/40 text-zinc-300 font-mono border border-white/5">
                            {r.rank}
                          </span>
                        {/each}
                        {#if tmpl.ranks.length > 3}
                          <span class="text-[9px] px-1 py-0.2 rounded text-zinc-500 font-mono">
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
                <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
                  <div class="flex items-center justify-between">
                    <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                      <Users class="size-3.5 text-purple-400" />
                      <span>Office Team Roster ({members.length})</span>
                    </h4>
                    <span class="text-[11px] text-zinc-400">Autonomous Pair Programming Fleet</span>
                  </div>

                  <div class="space-y-2 max-h-[380px] overflow-y-auto pr-1">
                    {#each members as m (m.bot_id)}
                      <div class="flex items-center justify-between p-3 rounded-xl border border-[#202034] bg-[#121222] gap-3 hover:border-purple-500/30 transition-all">
                        <div class="flex items-center gap-3 min-w-0">
                          <Avatar.Root class="size-9 rounded-xl ring-1 ring-purple-400/30 shrink-0">
                            <Avatar.Image
                              src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "bottts")}
                              alt={m.bot?.name || m.rank}
                            />
                            <Avatar.Fallback class="bg-purple-950 text-purple-200 text-xs">
                              {(m.bot?.name || m.rank).slice(0, 2)}
                            </Avatar.Fallback>
                          </Avatar.Root>

                          <div class="min-w-0">
                            <div class="flex items-center gap-2">
                              <span class="font-bold text-xs text-white truncate">{m.bot?.name || m.rank}</span>
                              <Badge variant="outline" class="text-[9px] px-1.5 py-0 bg-purple-950/60 border-purple-500/30 text-purple-300 font-mono">
                                {m.rank}
                              </Badge>
                            </div>
                            <p class="text-[11px] text-zinc-400 truncate mt-0.5">{m.specialty}</p>
                          </div>
                        </div>

                        <div class="flex items-center gap-2 shrink-0">
                          <Button
                            variant="ghost"
                            size="sm"
                            class="size-7 p-0 text-zinc-400 hover:text-red-400 hover:bg-red-950/30 cursor-pointer rounded-lg"
                            title="Remove agent from office"
                            onclick={() => removeMember(m.bot_id)}
                          >
                            <Trash2 class="size-3.5" />
                          </Button>
                        </div>
                      </div>
                    {:else}
                      <div class="py-12 px-4 text-center border-2 border-dashed border-[#202034] rounded-xl text-zinc-500 space-y-2">
                        <Users class="size-8 mx-auto opacity-30 text-purple-400" />
                        <p class="text-xs">No agents assigned to this office yet.</p>
                        <p class="text-[11px] text-zinc-500">Search existing agents on the right or provision a new one inline.</p>
                      </div>
                    {/each}
                  </div>
                </div>
              </div>

              <!-- Right 1 Col: Provision / Add Agents -->
              <div class="space-y-3">
                <!-- Add Existing -->
                <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Search class="size-3.5 text-purple-400" />
                    <span>Add Existing Fleet Agent</span>
                  </h4>

                  <div class="relative">
                    <Search class="size-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
                    <Input
                      bind:value={searchQuery}
                      placeholder="Search fleet bots..."
                      class="pl-8 h-8 text-xs bg-[#141424] border-[#25253c] text-white"
                    />
                  </div>

                  <div class="space-y-1.5 max-h-36 overflow-y-auto">
                    {#each filteredBots as b (b.id)}
                      <button
                        type="button"
                        class="w-full flex items-center justify-between p-2 rounded-xl border border-[#202034] bg-[#121222] hover:border-purple-500/40 hover:bg-[#161628] text-left cursor-pointer transition-all"
                        onclick={() => addExistingBot(b)}
                      >
                        <div class="flex items-center gap-2 min-w-0">
                          <Avatar.Root class="size-6.5 rounded-lg shrink-0">
                            <Avatar.Image src={b.avatar_url || getDiceBearUrl(b.name, b.avatar_style || "bottts")} />
                            <Avatar.Fallback class="text-[10px]">{b.name.slice(0, 2)}</Avatar.Fallback>
                          </Avatar.Root>
                          <span class="text-xs font-bold text-white truncate">{b.name}</span>
                        </div>
                        <span class="text-[10px] font-mono text-purple-400 shrink-0 font-semibold">+ Assign</span>
                      </button>
                    {:else}
                      <p class="text-[11px] text-zinc-500 text-center py-3">
                        {searchQuery ? "No matching available bots" : "All fleet bots already in office"}
                      </p>
                    {/each}
                  </div>
                </div>

                <!-- Create New Inline -->
                <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <UserPlus class="size-3.5 text-purple-400" />
                    <span>Provision New Agent</span>
                  </h4>

                  <div class="space-y-2">
                    <Input
                      bind:value={newBotName}
                      placeholder="Agent Name (e.g. Sarah / RustArchitect)"
                      class="h-8 text-xs bg-[#141424] border-[#25253c] text-white"
                    />
                    <Input
                      bind:value={newBotRank}
                      placeholder="Rank (e.g. Tech Lead)"
                      class="h-8 text-xs bg-[#141424] border-[#25253c] text-white"
                    />
                    <Input
                      bind:value={newBotSpecialty}
                      placeholder="Specialty (e.g. Backend & Distributed Systems)"
                      class="h-8 text-xs bg-[#141424] border-[#25253c] text-white"
                    />

                    <Button
                      size="sm"
                      class="w-full h-8 text-xs bg-purple-600 hover:bg-purple-500 text-white font-medium gap-1.5 cursor-pointer shadow-sm"
                      disabled={!newBotName.trim()}
                      onclick={createAndAddBot}
                    >
                      <Plus class="size-3.5" />
                      <span>Provision & Add to Office</span>
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
            <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3.5 backdrop-blur-md">
              <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-white/5">
                <div>
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Layers class="size-3.5 text-purple-400" />
                    <span>Office Connectors & MCP Dispatch Matrix</span>
                  </h4>
                  <p class="text-[11px] text-zinc-400 mt-0.5">
                    Assign and synchronize 135+ Model Context Protocol tools across all member agents in this office.
                  </p>
                </div>

                <!-- 1-Click Preset Stacks for Office -->
                <div class="flex items-center gap-1.5 flex-wrap">
                  <Button
                    size="sm"
                    variant="outline"
                    class="h-7 text-[11px] bg-[#141424] border-purple-500/30 text-purple-300 hover:bg-purple-950/40 cursor-pointer"
                    onclick={() => applyPresetToOffice(["github", "postgres", "redis", "docker", "sentry", "shell", "filesystem"])}
                  >
                    <span>+ Full-Stack Dev Stack</span>
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    class="h-7 text-[11px] bg-[#141424] border-cyan-500/30 text-cyan-300 hover:bg-cyan-950/40 cursor-pointer"
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
                    <div class="p-3.5 rounded-xl border border-[#202034] bg-[#121222] space-y-2.5">
                      <div class="flex items-center justify-between">
                        <div class="flex items-center gap-2.5">
                          <Avatar.Root class="size-7 rounded-lg ring-1 ring-purple-400/30">
                            <Avatar.Image src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, "bottts")} />
                            <Avatar.Fallback>{(m.bot?.name || m.rank).slice(0, 2)}</Avatar.Fallback>
                          </Avatar.Root>
                          <div>
                            <span class="font-bold text-xs text-white">{m.bot?.name || m.rank}</span>
                            <span class="text-[10px] text-purple-300 font-mono ml-1.5">({m.rank})</span>
                          </div>
                        </div>
                        <Badge variant="outline" class="text-[10px] font-mono bg-purple-950/50 border-purple-500/30 text-purple-300">
                          {botAssigned.size} tools active
                        </Badge>
                      </div>

                      <!-- Tool Chips for this Bot -->
                      <div class="flex flex-wrap gap-1.5">
                        {#each allServers.slice(0, 16) as s (s.id)}
                          {@const isEnabled = botAssigned.has(s.id)}
                          <button
                            type="button"
                            class={cn(
                              "px-2 py-1 rounded-lg text-[10px] font-mono transition-all flex items-center gap-1.5 cursor-pointer border",
                              isEnabled
                                ? "bg-purple-600 text-white border-purple-400 font-bold shadow-sm"
                                : "bg-[#18182a] text-zinc-400 border-[#28283e] hover:bg-[#202038] hover:text-white"
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
                <div class="py-10 text-center text-zinc-500 border border-dashed border-[#202034] rounded-xl">
                  <p class="text-xs">Add members to this office to configure tool dispatch.</p>
                </div>
              {/if}
            </div>
          </Tabs.Content>

          <!-- TAB 5: OFFICE POLICY & STANDARDS -->
          <Tabs.Content value="policy" class="space-y-4">
            <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Shield class="size-3.5 text-cyan-400" />
                    <span>Office Policy & Autonomous Guardrails</span>
                  </h4>
                  <p class="text-[11px] text-zinc-400 mt-0.5">
                    Operating standards and review rules enforced on every agent execution in this office.
                  </p>
                </div>
              </div>

              <!-- Quick Templates -->
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-[10px] font-mono text-zinc-500 uppercase">Preset Templates:</span>
                {#each POLICY_TEMPLATES as pt}
                  <button
                    type="button"
                    class="text-[10px] font-mono bg-[#161628] hover:bg-purple-950/40 border border-[#28283e] hover:border-purple-500/40 text-purple-300 px-2 py-0.5 rounded-md cursor-pointer transition-colors"
                    onclick={() => (policy = pt.content)}
                  >
                    + {pt.title}
                  </button>
                {/each}
              </div>

              <Textarea
                bind:value={policy}
                placeholder="e.g. 1. All code changes require unit test verification&#10;2. Architecture decisions must be signed off by Tech Lead&#10;3. Never expose credentials or sensitive API tokens"
                rows={9}
                class="font-mono text-xs bg-[#121222] border-[#25253c] text-zinc-200 focus-visible:border-purple-500 leading-relaxed"
              />
            </div>
          </Tabs.Content>

          <!-- TAB 6: TERMS & CONDITIONS -->
          <Tabs.Content value="terms" class="space-y-4">
            <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <FileText class="size-3.5 text-amber-400" />
                    <span>Terms of Operation & Consensus</span>
                  </h4>
                  <p class="text-[11px] text-zinc-400 mt-0.5">
                    Legal boundaries, data privacy covenants, and agent task handoff contracts.
                  </p>
                </div>
              </div>

              <!-- Quick Templates -->
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-[10px] font-mono text-zinc-500 uppercase">Preset Templates:</span>
                {#each TERMS_TEMPLATES as tt}
                  <button
                    type="button"
                    class="text-[10px] font-mono bg-[#161628] hover:bg-amber-950/40 border border-[#28283e] hover:border-amber-500/40 text-amber-300 px-2 py-0.5 rounded-md cursor-pointer transition-colors"
                    onclick={() => (terms = tt.content)}
                  >
                    + {tt.title}
                  </button>
                {/each}
              </div>

              <Textarea
                bind:value={terms}
                placeholder="e.g. Terms: All workspace state remains encrypted locally. Multi-agent consensus requires signoff before production deployment."
                rows={9}
                class="font-mono text-xs bg-[#121222] border-[#25253c] text-zinc-200 focus-visible:border-purple-500 leading-relaxed"
              />
            </div>
          </Tabs.Content>

          <!-- TAB 7: BUDGET & TOKEN ALLOCATION -->
          <Tabs.Content value="budget" class="space-y-4">
            <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Coins class="size-3.5 text-emerald-400" />
                    <span>Office Token & Cost Allocation</span>
                  </h4>
                  <p class="text-[11px] text-zinc-400 mt-0.5">
                    Set total spending ceiling and distribute token weights across office team roles.
                  </p>
                </div>
              </div>

              <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 items-center">
                <div class="space-y-1.5">
                  <Label for="office-budget" class="text-xs font-bold text-zinc-200">Total Monthly Office Budget ($ USD)</Label>
                  <div class="relative">
                    <span class="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500 font-mono text-xs">$</span>
                    <Input
                      id="office-budget"
                      type="number"
                      bind:value={budget}
                      placeholder="e.g. 500"
                      class="pl-7 h-9 text-xs font-mono bg-[#141424] border-[#25253c] text-white"
                    />
                  </div>
                </div>

                <div class="p-3 rounded-xl bg-[#141424] border border-[#25253c] text-xs text-zinc-400 space-y-1">
                  <div class="flex justify-between">
                    <span>Ceiling Status:</span>
                    <span class="font-bold text-emerald-400">{budget ? `$${budget} Hard-Stop` : "Uncapped"}</span>
                  </div>
                  <div class="flex justify-between text-[11px] text-zinc-500">
                    <span>Active Member Share:</span>
                    <span>{members.length > 0 ? `$${((budget ? parseFloat(budget) : 0) / members.length).toFixed(2)} / agent` : "N/A"}</span>
                  </div>
                </div>
              </div>

              {#if members.length > 0}
                <div class="space-y-2 pt-2 border-t border-white/5">
                  <Label class="text-xs font-bold text-zinc-300">Roster Role Distribution</Label>
                  {#each members as m}
                    <div class="flex items-center justify-between p-2.5 rounded-xl border border-[#202034] bg-[#121222]">
                      <div class="flex items-center gap-2.5">
                        <Avatar.Root class="size-6.5 rounded-lg">
                          <Avatar.Image src={m.bot?.avatar_url || getDiceBearUrl(m.rank, "bottts")} />
                          <Avatar.Fallback class="text-[10px]">{m.rank.slice(0, 2)}</Avatar.Fallback>
                        </Avatar.Root>
                        <div>
                          <span class="font-bold text-xs text-white">{m.bot?.name || m.rank}</span>
                          <span class="text-[10px] text-zinc-400 font-mono ml-1.5">— {m.rank}</span>
                        </div>
                      </div>
                      <Badge variant="outline" class="text-[10px] font-mono text-emerald-300 border-emerald-500/30 bg-emerald-950/40">
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
            <div class="rounded-2xl border border-[#202034] bg-[#0e0e1a]/90 p-4 space-y-3 backdrop-blur-md">
              <div class="flex items-center justify-between">
                <div>
                  <h4 class="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                    <Target class="size-3.5 text-red-400" />
                    <span>Quarterly Office Milestone & Objective</span>
                  </h4>
                  <p class="text-[11px] text-zinc-400 mt-0.5">
                    Autonomous agents review and evaluate every task against this primary objective.
                  </p>
                </div>
              </div>

              <Textarea
                bind:value={goal}
                placeholder="e.g. Ship Version 2.0 by end of quarter: Implement real-time multi-agent consensus, zero-latency vector memory retrieval, and dark theme UI overhaul."
                rows={7}
                class="text-xs bg-[#121222] border-[#25253c] text-zinc-200 focus-visible:border-purple-500 leading-relaxed"
              />

              <div class="p-3 rounded-xl bg-purple-950/20 border border-purple-500/30 flex items-center gap-3">
                <Sparkles class="size-4 text-purple-400 shrink-0" />
                <p class="text-[11px] text-purple-200/90 leading-relaxed">
                  Autonomous agents in this room evaluate their reasoning steps against this mission goal before marking tasks complete.
                </p>
              </div>
            </div>
          </Tabs.Content>
        </Tabs.Root>
      </div>

      <!-- Unified Executive Footer (Single Save Changes Bar) -->
      <div class="px-6 py-3.5 border-t border-white/10 flex items-center justify-between bg-[#10101f]/90 shrink-0">
        <Button
          variant="outline"
          class="h-8.5 text-xs bg-red-950/30 border-red-500/40 text-red-300 hover:bg-red-900/50 hover:text-white cursor-pointer gap-1.5 transition-colors"
          onclick={handleDelete}
        >
          <Trash2 class="size-3.5" />
          <span>Delete Office</span>
        </Button>

        <div class="flex items-center gap-2.5">
          <Button
            variant="outline"
            class="h-8.5 text-xs border-[#29293e] text-zinc-300 hover:bg-[#1c1c2e] cursor-pointer"
            onclick={onClose}
          >
            Cancel
          </Button>

          <Button
            class={cn(
              "h-8.5 text-xs font-medium gap-1.5 cursor-pointer shadow-[0_0_20px_rgba(168,85,247,0.35)] transition-all",
              saveSuccess
                ? "bg-emerald-600 hover:bg-emerald-500 text-white"
                : "bg-purple-600 hover:bg-purple-500 text-white"
            )}
            disabled={isSaving}
            onclick={saveAll}
          >
            {#if saveSuccess}
              <Check class="size-3.5" />
              <span>Saved Successfully</span>
            {:else}
              <Save class="size-3.5" />
              <span>{isSaving ? "Saving..." : "Save Changes"}</span>
            {/if}
          </Button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<!-- Custom Executive Delete Confirmation Modal -->
{#if showDeleteConfirm}
  <Dialog.Root open={showDeleteConfirm} onOpenChange={(o) => !o && (showDeleteConfirm = false)}>
    <Dialog.Content class="sm:max-w-md max-w-md bg-[#0e0e1a] border border-red-500/40 rounded-3xl p-6 text-zinc-100 shadow-[0_0_60px_rgba(239,68,68,0.3)] flex flex-col gap-4 font-sans select-none z-50">
      <div class="flex items-start gap-3.5">
        <div class="size-11 rounded-2xl bg-red-950/70 border border-red-500/50 flex items-center justify-center text-red-400 shrink-0 shadow-[0_0_20px_rgba(239,68,68,0.35)]">
          <AlertTriangle class="size-6 animate-pulse" />
        </div>
        <div class="space-y-1">
          <Dialog.Title class="text-base font-extrabold text-white">
            Delete Autonomous Office?
          </Dialog.Title>
          <Dialog.Description class="text-xs text-zinc-400 leading-relaxed">
            Are you sure you want to delete <span class="text-white font-bold font-mono">"{name || room?.name}"</span>?
            All shared team memories, conversation threads, and agent tool dispatch configurations will be permanently removed.
          </Dialog.Description>
        </div>
      </div>

      <div class="p-3 rounded-xl bg-red-950/25 border border-red-500/30 text-[11px] text-red-300 font-mono flex items-center gap-2">
        <AlertCircle class="size-4 text-red-400 shrink-0" />
        <span>This action is permanent and cannot be undone.</span>
      </div>

      <div class="flex items-center justify-end gap-2.5 pt-2 border-t border-white/5">
        <Button
          variant="outline"
          class="h-8.5 text-xs bg-[#141424] border-[#29293e] text-zinc-300 hover:bg-[#1a1a2e] hover:text-white cursor-pointer"
          onclick={() => (showDeleteConfirm = false)}
          disabled={isDeleting}
        >
          Keep Office
        </Button>

        <Button
          class="h-8.5 text-xs bg-red-600 hover:bg-red-500 text-white font-medium gap-1.5 shadow-[0_0_25px_rgba(239,68,68,0.4)] cursor-pointer"
          disabled={isDeleting}
          onclick={executeDelete}
        >
          <Trash2 class="size-3.5" />
          <span>{isDeleting ? "Deleting Office..." : "Permanently Delete"}</span>
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}
