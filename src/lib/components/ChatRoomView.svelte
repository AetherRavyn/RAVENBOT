<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import * as Avatar from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as Card from "$lib/components/ui/card";
  import { Separator } from "$lib/components/ui/separator";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { getDiceBearUrl, isUserMessage, OFFICE_TEMPLATES } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import { recordUtterance, transcribeBlob, voiceErrorMessage } from "$lib/voice";
  import {
    ACCEPTED_IMAGE_MIMES,
    isTextFile,
    readAsDataUrl,
    readAsText,
    type PendingAttachment,
  } from "$lib/attachments";
  import { onMount, onDestroy, tick } from "svelte";
  import { notify } from "$lib/toast";
  import OfficeSettings from "$lib/components/OfficeSettings.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import {
    Building2,
    Users,
    Send,
    Sparkles,
    CheckCircle2,
    XCircle,
    Loader2,
    Circle,
    ArrowRight,
    Play,
    Pause,
    Radio,
    Shield,
    Workflow,
    Pencil,
    Layers,
    Cpu,
    Wrench,
    Server,
    Paperclip,
    Mic,
    Terminal,
    Settings,
    ArrowUp,
    Copy,
    Check,
  } from "@lucide/svelte";

  interface Props {
    room: any;
    bots: any[];
  }

  let { room, bots }: Props = $props();

  let members = $state<any[]>([]);
  let messages = $state<any[]>([]);
  let newMessage = $state("");
  let threadId: string | null = $state(null);
  let sending = $state(false);
  let chatContainer = $state<HTMLDivElement | null>(null);
  let showOfficeSettings = $state(false);
  // Plan mode
  let showPlanModal = $state(false);
  let planGoal = $state("");
  let planTasks = $state<Array<{ id: string; botId: string; label: string; dependsOn: number[] }>>([]);
  let planGenerating = $state(false);
  let planError = $state<string | null>(null);
  let draftingPlan = $state(false);
  let planQuestion = $state<string | null>(null);

  // Hiring / org provisioning
  type HireRole = {
    name: string;
    rank: string;
    specialty: string;
    system_prompt: string;
    is_lead: boolean;
    skills: string[];
  };
  let showHireModal = $state(false);
  let hireRoles = $state<HireRole[]>([]);
  let hireBrief = $state("");
  let hireDrafting = $state(false);
  let hireProvisioning = $state(false);
  let hireError = $state<string | null>(null);
  let hireQuestion = $state<string | null>(null);
  let hireSource = $state<"ceo" | "template" | null>(null);
  let openRolePrompt = $state<number | null>(null);
  let hireResult = $state<string | null>(null);

  // Owner agent management (model / skills / MCP & connectors) for any agent.
  let manageBot = $state<any>(null);
  let showBotSettings = $state(false);
  let showSkillManager = $state(false);
  let showMcpManager = $state(false);
  let notice = $state<string | null>(null);
  let noticeTimer: number | undefined;

  function showNotice(text: string) {
    notice = text;
    if (noticeTimer) window.clearTimeout(noticeTimer);
    noticeTimer = window.setTimeout(() => (notice = null), 5000);
  }

  function openManageAgent(m: any) {
    const bot = m?.bot || bots.find((b: any) => b.id === m?.bot_id);
    if (!bot) return;
    manageBot = bot;
  }

  function handleAgentUpdated(updated: any) {
    if (updated) {
      manageBot = updated;
      window.dispatchEvent(new CustomEvent("bots-changed"));
      showNotice(`${updated.name} updated`);
    }
    load();
  }

  function normalizeRole(r: any): HireRole {
    return {
      name: String(r?.name || "Agent"),
      rank: String(r?.rank || "Member"),
      specialty: String(r?.specialty || "Generalist"),
      system_prompt: String(r?.system_prompt || ""),
      is_lead: Boolean(r?.is_lead),
      skills: Array.isArray(r?.skills) ? r.skills.map(String) : [],
    };
  }

  async function openHireModal() {
    hireError = null;
    hireQuestion = null;
    hireSource = null;
    hireResult = null;
    hireBrief = room.goal || room.description || "";
    showHireModal = true;
    // Empty office: seed the built-in blueprint so there is always a team to
    // approve. Existing office: start empty so the CEO can add missing roles.
    if (members.length === 0) {
      try {
        const roles = (await invoke<any[]>("default_office_org", {
          officeTemplate: room.office_template,
        })) || [];
        hireRoles = roles.map(normalizeRole);
        hireSource = "template";
      } catch (e) {
        hireRoles = [];
      }
    } else {
      hireRoles = [];
    }
  }

  function addHireRole() {
    hireRoles = [
      ...hireRoles,
      { name: "New Agent", rank: "Member", specialty: "Generalist", system_prompt: "", is_lead: false, skills: [] },
    ];
  }

  function removeHireRole(idx: number) {
    hireRoles = hireRoles.filter((_, i) => i !== idx);
  }

  function setHireLead(idx: number) {
    hireRoles = hireRoles.map((r, i) => ({ ...r, is_lead: i === idx }));
  }

  async function draftTeamWithCeo() {
    hireDrafting = true;
    hireError = null;
    hireQuestion = null;
    try {
      const res: any = await invoke("draft_office_org", {
        chatroomId: room.id,
        brief: hireBrief || room.goal || room.description || "Staff this office.",
      });
      hireSource = res?.source || "ceo";
      if (res?.question) {
        hireQuestion = String(res.question);
        return;
      }
      const roles = Array.isArray(res?.roles) ? res.roles : [];
      if (roles.length === 0) {
        hireError = "The CEO couldn't propose a team — add roles manually.";
        return;
      }
      hireRoles = roles.map(normalizeRole);
    } catch (e: any) {
      hireError = String(e);
    } finally {
      hireDrafting = false;
    }
  }

  async function provisionHiredTeam() {
    if (hireRoles.length === 0) {
      hireError = "Add at least one role first.";
      return;
    }
    if (hireRoles.filter((r) => r.is_lead).length > 1) {
      hireError = "Only one lead (CEO) is allowed.";
      return;
    }
    hireProvisioning = true;
    hireError = null;
    try {
      const res: any = await invoke("provision_office_org", {
        chatroomId: room.id,
        roles: hireRoles.map((r) => ({
          name: r.name,
          rank: r.rank,
          specialty: r.specialty,
          system_prompt: r.system_prompt || null,
          is_lead: r.is_lead,
          skills: r.skills,
        })),
      });
      const createdCount = Array.isArray(res?.created) ? res.created.length : 0;
      const reusedCount = Array.isArray(res?.reused) ? res.reused.length : 0;
      hireResult = `Hired ${createdCount} new agent${createdCount === 1 ? "" : "s"} · reused ${reusedCount} existing from the fleet.`;
      window.dispatchEvent(new CustomEvent("bots-changed"));
      await load();
    } catch (e: any) {
      hireError = String(e);
    } finally {
      hireProvisioning = false;
    }
  }

  // User avatar from localStorage (chosen via Settings)
  let userAvatar = $state<string | null>(null);
  // Live office telemetry from the runtime stream (per-agent status + usage)
  let agentStatus = $state<Record<string, string>>({});
  // Live per-agent token streams (lanes) during a team run
  let lanes = $state<Record<string, string>>({});
  // Current tool each agent is running (live)
  let agentTool = $state<Record<string, string>>({});
  let officeTokens = $state(0);
  let officeCost = $state(0.0);
  let unlisten: UnlistenFn | null = null;
  $effect(() => {
    if (typeof localStorage !== "undefined") {
      userAvatar = localStorage.getItem("ravenbot_user_avatar");
      const handler = () => (userAvatar = localStorage.getItem("ravenbot_user_avatar"));
      window.addEventListener("user-avatar-changed", handler);
      return () => window.removeEventListener("user-avatar-changed", handler);
    }
  });

  const sampleTasks: Record<string, string[]> = {
 "it-office": [
 "Refactor state management and run full test suites",
 "Design database schema migration for multi-agent workflows",
 "Benchmark runtime latency and identify bottlenecks",
    ],
 "rot-archive": [
 "Transcribe arcane marginalia and verify occult sigils",
 "Formulate antidote elixir against necrotic corruption",
 "Catalog forbidden manuscripts and bind protective wards",
    ],
 "design": [
 "Create high-fidelity dark mode design tokens and components",
 "Audit UX navigation flow and eliminate friction points",
 "Generate animated brand asset library and icon set",
    ],
 "marketing": [
 "Draft multi-channel product launch campaign strategy",
 "Analyze competitor positioning and optimize key messaging",
 "Write high-converting technical release notes and copy",
    ],
 "sales": [
 "Build enterprise prospect qualification matrix",
 "Draft targeted outreach sequence for enterprise tier",
 "Prepare value proposition deck and objection handling",
    ],
  };

  let roomTasks = $derived(
    sampleTasks[room?.office_template] || [
 "Audit current workspace state and execute core plan",
 "Coordinate parallel team review across all disciplines",
 "Synthesize execution deliverables and generate summary report",
    ]
  );

  // Agents with live work, and whether the team is currently active at all
  // (covers Plan mode, which does not set `sending`).
  let activeMembers = $derived(
    members.filter((m) => {
      const st = agentStatus[m.bot?.id];
      return (
        (st && st !== "idle") ||
        ((lanes[m.bot?.id] || "").length > 0) ||
        Boolean(agentTool[m.bot?.id])
      );
    })
  );
  let teamActive = $derived(sending || planGenerating || activeMembers.length > 0);

  let templateInfo = $derived(
    OFFICE_TEMPLATES[room?.office_template as keyof typeof OFFICE_TEMPLATES] || OFFICE_TEMPLATES.custom
  );

  async function scrollToBottom() {
    await tick();
    if (chatContainer) {
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }
  }

  async function load() {
    try {
      const mems = (await invoke("list_chatroom_members", { chatroomId: room.id })) as any[];
      members = mems.map((m) => ({ ...m, bot: bots.find((b: any) => b.id === m.bot_id) }));
      const tid = await invoke("get_chatroom_thread", { chatroomId: room.id });
      if (tid) {
        threadId = tid as string;
        messages = (await invoke("list_messages", { threadId })) as any[];
      } else {
        threadId = null;
        messages = [];
      }
      scrollToBottom();
    } catch (e) {
      console.error(e);
    }
  }

  $effect(() => {
    if (room?.id) {
      threadId = null;
      messages = [];
      agentStatus = {};
      lanes = {};
      agentTool = {};
      officeTokens = 0;
      officeCost = 0;
      load();
    }
  });

  onMount(() => {
    listen<any>("agent-stream", (event) => {
        const p = event.payload;
        if (!p) return;
        if (p.kind === "status") {
          const state = p.state === "done" ? "idle" : p.state;
          agentStatus = { ...agentStatus, [p.bot_id]: state };
        } else if (p.kind === "usage") {
          officeTokens += p.tokens || 0;
          officeCost += p.cost || 0;
        } else if (p.kind === "delta") {
          // Live lane: stream tokens for member agents
          if (p.bot_id && members.some((m) => m.bot?.id === p.bot_id)) {
            lanes = { ...lanes, [p.bot_id]: (lanes[p.bot_id] || "") + (p.content || "") };
            scrollToBottom();
          }
        } else if (p.kind === "tool_started") {
          if (p.bot_id) {
            agentTool = { ...agentTool, [p.bot_id]: p.name || "tool" };
            agentStatus = { ...agentStatus, [p.bot_id]: "running_tool" };
          }
        } else if (p.kind === "tool_finished") {
          if (p.bot_id) {
            const next = { ...agentTool };
            delete next[p.bot_id];
            agentTool = next;
          }
        } else if (p.kind === "clear") {
          if (p.bot_id) {
            lanes = { ...lanes, [p.bot_id]: "" };
          }
        } else if (p.kind === "done") {
          lanes = {};
          agentTool = {};
          agentStatus = {};
        }
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.error("Failed to attach office stream listener:", e));
  });

  onDestroy(() => {
    unlisten?.();
    unlisten = null;
  });

  function openPlanModal() {
    planGoal = "";
    planTasks = [];
    planError = null;
    planQuestion = null;
    planTasks = members.map((m, i) => ({
      id: String(i),
      botId: m.bot_id,
      label: "Address your part of the goal",
      dependsOn: [] as number[],
    }));
    showPlanModal = true;
  }

  function addPlanTask() {
    planTasks = [...planTasks, { id: String(planTasks.length), botId: members[0]?.bot_id ?? "", label: "New task", dependsOn: [] }];
  }

  function removePlanTask(idx: number) {
    planTasks = planTasks.filter((_, i) => i !== idx).map((t, i) => ({ ...t, id: String(i), dependsOn: t.dependsOn.filter((d) => d !== idx).map((d) => d > idx ? d - 1 : d) }));
  }

  function updatePlanTaskLabel(idx: number, label: string) {
    planTasks = planTasks.map((t, i) => (i === idx ? { ...t, label } : t));
  }

  // Let the room's lead draft the plan from the goal, then let the user edit it.
  async function draftPlan() {
    if (!planGoal.trim()) { planError = "Describe the goal first."; return; }
    planError = null;
    planQuestion = null;
    draftingPlan = true;
    try {
      const res: any = await invoke("draft_office_plan", {
        chatroomId: room.id,
        goal: planGoal,
      });
      if (res?.question) {
        // The lead needs clarification — surface it and don't overwrite tasks.
        planQuestion = String(res.question);
        return;
      }
      const drafted = Array.isArray(res?.tasks) ? res.tasks : [];
      if (drafted.length === 0) {
        planError = "The lead couldn't draft a plan — add tasks manually.";
        return;
      }
      planTasks = drafted.map((t: any, i: number) => ({
        id: String(i),
        botId: t.bot_id,
        label: t.instruction,
        dependsOn: Array.isArray(t.depends_on) ? t.depends_on : [],
      }));
    } catch (e: any) {
      planError = String(e);
    } finally {
      draftingPlan = false;
    }
  }

  function updateTaskBot(idx: number, botId: string) {
    planTasks = planTasks.map((t, i) => (i === idx ? { ...t, botId } : t));
  }

  function togglePlanDep(taskIdx: number, depIdx: number) {
    planTasks = planTasks.map((t, i) => {
      if (i !== taskIdx) return t;
      const has = t.dependsOn.includes(depIdx);
      return { ...t, dependsOn: has ? t.dependsOn.filter((d) => d !== depIdx) : [...t.dependsOn, depIdx] };
    });
  }

  async function dispatchPlan() {
    if (!planGoal.trim()) { planError = "Describe the goal first."; return; }
    if (planTasks.length === 0) { planError = "Add at least one task."; return; }
    planError = null;
    planGenerating = true;
    // Optimistically show the dispatched goal in the room feed.
    const tempUserMsg = {
      id: "temp-plan-" + Date.now(),
      role: "user",
      content: `🎯 **Plan Mode** — goal: ${planGoal.trim()}`,
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempUserMsg];
    scrollToBottom();
    // Close the modal immediately: the room streams live lanes while the DAG
    // runs, so the user watches progress instead of a frozen button.
    showPlanModal = false;
    showNotice("Plan dispatched — the team is working. Progress streams below.");
    try {
      const tasks = planTasks.map((t) => ({
        bot_id: t.botId,
        instruction: `Goal: ${planGoal}\n\nYour task: ${t.label}`,
        depends_on: t.dependsOn,
      }));
      // Prefer an explicit orchestrator/lead; otherwise let the backend resolve
      // the room's lead (pass a nil UUID).
      const lead =
        members.find((m: any) => m.bot?.is_orchestrator)?.bot_id ??
        members[0]?.bot_id ??
 "00000000-0000-0000-0000-000000000000";
      const res: any = await invoke("execute_graph", {
        orchestratorBotId: lead,
        goal: planGoal,
        tasks,
        chatroomId: room.id,
      });
      // Reload the room thread the plan was posted into.
      const tid = res.thread_id || threadId;
      if (tid) {
        threadId = tid;
        messages = (await invoke("list_messages", { threadId: tid })) as any[];
        scrollToBottom();
      }
    } catch (e: any) {
      notify(`Plan dispatch failed: ${String(e)}`, "error");
    } finally {
      planGenerating = false;
    }
  }

  async function send(taskText?: string) {
    const typed = (taskText || newMessage).trim();
    if (sending) return;
    const attachments = pendingAttachments;
    if (!typed && attachments.length === 0) return;
    const text = typed || "(shared files)";

    sending = true;
    newMessage = "";
    pendingAttachments = [];
    lanes = {};
    agentStatus = {};

    // Optimistically insert user prompt
    const tempUserMsg = {
      id: "temp-" + Date.now(),
      role: "user",
      content: text,
      attachments: attachments.map((a) => ({
        name: a.name,
        mime_type: a.mime,
        is_image: a.isImage,
        data: a.data,
      })),
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempUserMsg];
    scrollToBottom();

    try {
      const res: any = await invoke("send_to_chatroom", {
        chatroomId: room.id,
        content: text,
        attachments: attachments.length ? attachments : undefined,
      });
      threadId = res.thread_id;
      messages = (await invoke("list_messages", { threadId })) as any[];
    } catch (e) {
      console.error("Office dispatch error:", e);
    } finally {
      sending = false;
      lanes = {};
      scrollToBottom();
    }
  }

  function statusDot(status: string): string {
    switch (status) {
      case "thinking": return "bg-warning";
      case "running_tool": return "bg-info";
      case "waiting_on_user": return "bg-danger";
      case "paused": return "bg-[var(--brand)]";
      default: return "bg-success";
    }
  }

  function formatTime(isoStr: string) {
    if (!isoStr) return "";
    try {
      const d = new Date(isoStr);
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } catch {
      return "";
    }
  }

  let isListening = $state(false);
  let activeRecorder: any = null;
  let pendingAttachments = $state<PendingAttachment[]>([]);

  async function attachFiles(files: FileList | File[]) {
    for (const file of Array.from(files)) {
      const mime = (file.type || "").toLowerCase();
      const ext = (file.name?.split(".").pop() || "").toLowerCase();
      if (ACCEPTED_IMAGE_MIMES.includes(mime)) {
        const base64 = (await readAsDataUrl(file)).split(",")[1] || "";
        if (base64) {
          pendingAttachments = [...pendingAttachments, {
            name: file.name || "pasted-image.png",
            mime: mime || "image/png",
            data: base64,
            isImage: true,
          }];
        }
        continue;
      }
      if (isTextFile(file)) {
        const text = await readAsText(file);
        if (!text) continue;
        newMessage = (newMessage ? newMessage + "\n\n" : "") +
          `Attached file [${file.name}]:\n\`\`\`${ext}\n${text}\n\`\`\`\n`;
        continue;
      }
      const base64 = (await readAsDataUrl(file)).split(",")[1] || "";
      if (!base64) continue;
      pendingAttachments = [...pendingAttachments, {
        name: file.name || "attachment.bin",
        mime: mime || "application/octet-stream",
        data: base64,
        isImage: false,
      }];
    }
  }

  function removePendingAttachment(idx: number) {
    pendingAttachments = pendingAttachments.filter((_, i) => i !== idx);
  }

  function attachFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.onchange = async (e) => {
      const files = (e.target as HTMLInputElement).files;
      if (files?.length) await attachFiles(files);
    };
    input.click();
  }

  async function toggleVoice() {
    if (isListening) {
      activeRecorder?.stop();
      isListening = false;
      return;
    }
    isListening = true;
    try {
      const rec = await recordUtterance({ maxMs: 25000, silenceMs: 1700 });
      activeRecorder = rec;
      const blob = await rec.result;
      activeRecorder = null;
      if (!blob) return;
      const text = await transcribeBlob(blob);
      if (text) newMessage = (newMessage ? newMessage + " " : "") + text;
    } catch (e: any) {
      console.error("Voice input failed:", e);
      alert(voiceErrorMessage(e));
    } finally {
      isListening = false;
    }
  }
</script>

<div class="flex flex-col h-full overflow-hidden select-none bg-[var(--surface-0)] text-[var(--text-primary)] font-sans">
  <!-- Top Office Header Bar -->
  <header class="h-15 px-4 border-[var(--hairline)] border-[var(--hairline)] bg-[var(--surface-0)] flex items-center justify-between z-10 shrink-0">
    <div class="flex items-center gap-3">
      <!-- Office Avatar -->
      <div class="size-10 rounded-2xl overflow-hidden bg-[var(--surface-3)] border-[var(--hairline)] border-[var(--brand)]/40 p-0.5 shrink-0 shadow-md">
        <img
          src={room.avatar_url || getDiceBearUrl(room.name, room.avatar_style || "bottts")}
          alt={room.name}
          class="size-full rounded-xl object-cover"
        />
      </div>

      <div class="flex flex-col">
        <div class="flex items-center gap-2">
          <span class="font-bold text-sm text-white">{room.name}</span>
          <span class="font-mono text-[10px] py-0.2 px-2 rounded-md bg-[var(--surface-3)] border border-[var(--hairline)] text-[var(--brand-text)] capitalize">
            {room.office_template.replace("-", " ")}
          </span>
          <button
            type="button"
            class="icon-btn size-6"
            onclick={() => (showOfficeSettings = true)}
            title="Edit office name, description & avatar"
            aria-label="Edit office details"
          >
            <Pencil class="size-3.5" />
          </button>
        </div>
        <div class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)] mt-0.5">
          <span class="text-success font-mono flex items-center gap-1">
            <Radio class="size-2.5" />
            Parallel Lane
          </span>
          <span class="text-[var(--text-muted)]">·</span>
          <span>{members.length} {members.length === 1 ? 'Specialist' : 'Specialists'} Assigned</span>
          <span class="text-[var(--text-muted)]">·</span>
          <span class="font-mono text-[10px] px-1.5 py-0.5 rounded bg-success/10 border border-success/30 text-success" title="Live office telemetry (tokens / cost this session)">
            ${officeCost.toFixed(4)} · {officeTokens.toLocaleString()} tok
          </span>
        </div>
      </div>
    </div>

    <!-- Assigned Bot Roster Avatars -->
    <div class="flex items-center gap-3">
      <div class="flex -space-x-2">
        {#each members.slice(0, 5) as m, i}
          <div
            class="size-8 rounded-full overflow-hidden bg-[var(--surface-3)] border-[var(--hairline)] border-[var(--surface-0)] ring-1 ring-[var(--brand)]/40 shadow-sm transition-transform hover:scale-110 hover:z-10"
            title={`${m.bot?.name || m.rank} (${m.specialty})`}
          >
            <img
              src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "avataaars")}
              alt={m.rank}
              class="size-full object-cover"
            />
          </div>
        {/each}
        {#if members.length > 5}
          <div class="size-8 rounded-full bg-[var(--surface-3)] border-[var(--hairline)] border-[var(--surface-0)] ring-1 ring-[var(--brand)]/40 flex items-center justify-center text-[10px] font-bold text-[var(--brand-text)]">
            +{members.length - 5}
          </div>
        {/if}
      </div>
      <button type="button" onclick={() => openHireModal()} class="size-8 rounded-xl bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-success/50 flex items-center justify-center shrink-0 ml-1" title="Hire team — CEO proposes the agents this office needs" aria-label="Hire team">
        <Users class="size-4" />
      </button>
      <button type="button" onclick={() => (showOfficeSettings = true)} class="size-8 rounded-xl bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] flex items-center justify-center shrink-0 ml-1" title="Office settings — name, avatar, goal, policy, members, tools & budget" aria-label="Office settings">
        <Settings class="size-4" />
      </button>
      <button type="button" onclick={() => openPlanModal()} class="size-8 rounded-xl bg-[var(--hairline)] border border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] flex items-center justify-center shrink-0 ml-1" title="Plan mode — break a goal into specialist tasks" aria-label="Open plan mode">
        <Workflow class="size-4" />
      </button>
    </div>
  </header>

  <!-- Team strip: stable chips (no layout shift, status as a dot) -->
  {#if members.length > 0}
    <div class="px-4 py-2.5 bg-[var(--surface-1)] border-[var(--hairline)] border-[var(--hairline)] flex items-center gap-3 shrink-0">
      <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--text-muted)] shrink-0 flex items-center gap-1.5">
        <Workflow class="size-3 text-[var(--brand)]" />
        Team
        <span class="font-mono text-[var(--text-muted)]">{members.length}</span>
      </span>
      <div class="flex items-center gap-1.5 min-w-0 overflow-x-auto no-scrollbar">
        {#each members as m}
          {@const st = agentStatus[m.bot?.id] || "idle"}
          <button
            type="button"
            onclick={() => openManageAgent(m)}
            class="inline-flex items-center gap-2 h-8 pl-1 pr-2.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--brand)] hover:bg-[var(--surface-3)] shrink-0 transition-colors cursor-pointer"
            title={`${m.bot?.name || m.rank} — ${m.specialty}${st !== "idle" ? ` · ${st.replace("_", " ")}` : ""} (click to manage)`}
          >
            <span class="relative shrink-0">
              <img
                src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "bottts")}
                alt=""
                class="size-6 rounded-md object-cover border border-[var(--hairline)]"
              />
              <span class="absolute -bottom-0.5 -right-0.5 size-2 rounded-full ring-2 ring-[var(--surface-2)] {statusDot(st)}"></span>
            </span>
            <span class="text-[11px] font-semibold text-[var(--text-primary)] whitespace-nowrap">{m.rank}</span>
            <span class="hidden xl:inline text-[10px] text-[var(--text-muted)] truncate max-w-[8rem]">{m.specialty}</span>
            {#if st !== "idle"}
              <Loader2 class="size-3 text-warning animate-spin shrink-0" />
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Transient notice (hiring / agent updates) -->
  {#if notice}
    <div class="px-4 py-1.5 bg-[var(--brand-soft)] border-[var(--hairline)] border-[var(--brand)]/40 text-[11px] text-[var(--text-secondary)] flex items-center justify-between gap-3 shrink-0 animate-fade-in">
      <span>{notice}</span>
      <button type="button" onclick={() => (notice = null)} class="text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer" aria-label="Dismiss">✕</button>
    </div>
  {/if}

  <!-- Main Chat & Task Feed -->
  <div class="flex-1 flex flex-col overflow-hidden bg-[var(--surface-0)]">
    <div bind:this={chatContainer} class="flex-1 overflow-y-auto p-4 sm:p-6 space-y-6">
      <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto space-y-6">
        {#each messages as msg (msg.id || msg.created_at)}
          {@const isUser = isUserMessage(msg)}
          {@const isError = typeof msg.content === "string" && msg.content.includes("⚠️ **Model Error:**")}
          {@const rawText = typeof msg.content === "string" ? msg.content : msg.content?.text || JSON.stringify(msg.content)}
          {@const senderBot = msg.sender_bot_id ? bots.find((b: any) => b.id === msg.sender_bot_id) : null}
          {@const senderName = senderBot?.name || msg.sender_name || room.name}
          {@const senderAvatar = senderBot?.avatar_url || getDiceBearUrl(senderName, senderBot?.avatar_style || "bottts")}

          <div class="flex gap-3.5 {isUser ? 'justify-end' : 'justify-start'} group">
            {#if !isUser}
              <div class="size-8 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline)] shrink-0 mt-1 shadow-sm">
                <img
                  src={senderAvatar}
                  alt={senderName}
                  class="size-full object-cover"
                />
              </div>
            {/if}

            <div class="max-w-[85%] sm:max-w-[78%] space-y-1.5">
              {#if !isUser && !isError}
                <div class="text-[10px] font-mono px-1 flex items-center gap-1.5">
                  <span class="font-bold text-[var(--text-secondary)]">{senderName}</span>
                  {#if senderBot?.specialty}
                    <span class="text-[var(--text-muted)]">· {senderBot.specialty}</span>
                  {/if}
                </div>
              {/if}
              {#if isError}
                <div class="rounded-2xl p-4 bg-red-950/30 border border-red-800/40 text-[var(--text-secondary)] space-y-2 shadow-xl">
                  <div class="flex items-center gap-2 text-red-400 font-bold text-xs font-mono">
                    <Shield class="size-4 shrink-0" />
                    <span>Office Pipeline Error</span>
                  </div>
                  <p class="text-xs text-[var(--text-secondary)] leading-relaxed font-sans">
                    {rawText.replace("⚠️ **Model Error:** ", "")}
                  </p>
                </div>
              {:else if isUser}
                <div class="rounded-2xl px-4 py-3 text-xs leading-relaxed text-[var(--text-primary)] bg-[var(--surface-3)] border border-[var(--hairline)] shadow-md selection:bg-[var(--brand-soft)]">
                  <p class="whitespace-pre-wrap font-sans text-xs leading-relaxed">{rawText}</p>
                </div>
              {:else}
                <div class="rounded-2xl px-4 py-3.5 lg:px-5 bg-[var(--surface-1)] border border-[var(--hairline)] shadow-sm space-y-2 text-[var(--text-secondary)] text-[13px] lg:text-sm leading-relaxed selection:bg-[var(--brand)]/30">
                  <MarkdownRenderer content={rawText} />
                </div>
              {/if}

              <div class="text-[10px] text-[var(--text-muted)] px-1 {isUser ? 'text-right' : 'text-left'}">
                {formatTime(msg.created_at)}
              </div>
            </div>

            {#if isUser}
              <div class="size-8 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--hairline-strong)] shrink-0 mt-1 shadow-sm">
                <img src={userAvatar || getDiceBearUrl("You", "micah")} alt="You" class="size-full object-cover" />
              </div>
            {/if}
          </div>
        {:else}
          <!-- Empty State: Modern Office Mission Control -->
          <div class="p-8 text-center border border-dashed border-[var(--hairline)] rounded-xl bg-[var(--surface-0)]/80 max-w-xl mx-auto my-6 shadow-xl space-y-4">
            <div class="size-16 rounded-2xl bg-[var(--surface-2)] border border-[var(--hairline-strong)] mx-auto flex items-center justify-center text-[var(--brand-text)] shadow-2xl">
              <Building2 class="size-8" />
            </div>

            <div>
              <h3 class="font-bold text-base text-white tracking-tight">
                {room.name} Workspace
              </h3>
              <p class="text-xs text-[var(--text-tertiary)] mt-1 max-w-md mx-auto leading-relaxed">
                {room.description || "Multi-agent collaborative pipeline. Directives are automatically orchestrated across assigned team specialists in parallel."}
              </p>
            </div>

            <!-- Pre-configured Team Lane Overview -->
            {#if members.length > 0}
              <div class="flex items-center justify-center gap-3 pt-2">
                {#each members as m}
                  <div class="flex flex-col items-center gap-1">
                    <div class="size-9 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline-strong)] shadow-md">
                      <img
                        src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "bottts")}
                        alt={m.rank}
                        class="size-full object-cover"
                      />
                    </div>
                    <span class="text-[10px] font-bold text-[var(--text-secondary)] font-mono">{m.rank}</span>
                  </div>
                {/each}
              </div>
            {:else}
              <!-- No team yet: let the CEO staff the office -->
              <div class="p-4 mt-2 rounded-2xl border border-success/30 bg-success/20 text-left space-y-2">
                <div class="text-xs font-bold text-success flex items-center gap-1.5">
                  <Users class="size-3.5" /> This office has no team yet
                </div>
                <p class="text-[11px] text-[var(--text-tertiary)] leading-relaxed">
                  Give the CEO this office's mission and it will propose exactly which agents it needs
                  (planner, builders, testers, QA…). Approve the roster and they're hired instantly —
                  no manual bot setup.
                </p>
                <button
                  type="button"
                  onclick={() => openHireModal()}
                  class="h-8 px-3 rounded-xl bg-success hover:bg-success text-white text-xs font-medium flex items-center gap-1.5 cursor-pointer"
                >
                  <Sparkles class="size-3.5" /> Staff this office
                </button>
              </div>
            {/if}

            <!-- Quick Starter Tasks -->
            <div class="space-y-2 pt-4 text-left">
              <span class="text-[10px] font-bold text-[var(--text-muted)] uppercase tracking-wider font-mono flex items-center gap-1.5">
                <Sparkles class="size-3 text-[var(--brand-text)]" />
                Collaborative Directives:
              </span>
              <div class="grid grid-cols-1 gap-2">
                {#each roomTasks as task}
                  <button
                    type="button"
                    class="text-left text-xs p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--hairline-strong)] hover:bg-[#13131c] transition-all text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-between group cursor-pointer"
                    onclick={() => send(task)}
                  >
                    <span class="truncate">{task}</span>
                    <ArrowRight class="size-3.5 text-[var(--brand-text)] opacity-0 group-hover:opacity-100 transition-opacity shrink-0 ml-2" />
                  </button>
                {/each}
              </div>
            </div>
          </div>
        {/each}

        <!-- Live team activity: exactly what each specialist is doing now -->
        {#if teamActive}
          <div class="space-y-3">
            <div class="flex items-center gap-2 text-[10px] font-bold uppercase tracking-wider text-[var(--text-muted)]">
              <span class="size-1.5 rounded-full bg-success animate-pulse"></span>
              Live team activity · {activeMembers.length > 0 ? `${activeMembers.length} working` : "orchestrating"}
            </div>

            {#each activeMembers as m (m.bot?.id)}
              {@const st = agentStatus[m.bot?.id] || "idle"}
              {@const laneText = lanes[m.bot?.id] || ""}
              {@const toolName = agentTool[m.bot?.id]}
              <div class="flex gap-3 justify-start animate-rise-in">
                <div class="relative size-8 shrink-0 mt-1">
                  <img
                    src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "avataaars")}
                    alt={m.rank}
                    class="size-8 rounded-xl object-cover border border-[var(--hairline)] shadow-sm"
                  />
                  <span class="absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full ring-2 ring-[var(--surface-0)] {statusDot(st)}"></span>
                </div>

                <div class="min-w-0 flex-1 max-w-[85%] space-y-1.5">
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="text-[11px] font-bold text-white truncate">{m.bot?.name || m.rank}</span>
                    <span class="text-[10px] text-[var(--text-muted)] truncate hidden sm:inline">{m.rank}</span>
                    <span class="ml-auto shrink-0">
                      {#if toolName}
                        <span class="font-mono text-[9px] px-1.5 py-0.5 rounded bg-info/10 border border-info/30 text-info inline-flex items-center gap-1">
                          <Wrench class="size-2.5" /> {toolName}
                        </span>
                      {:else}
                        <span class="font-mono text-[9px] px-1.5 py-0.5 rounded {st === "thinking" ? "bg-warning/10 border border-warning/30 text-warning" : "bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)]"}">
                          {st === "thinking" ? "thinking" : st === "waiting_on_user" ? "waiting" : "working"}
                        </span>
                      {/if}
                    </span>
                  </div>

                  {#if laneText}
                    <div class="rounded-2xl rounded-tl-md px-4 py-3 bg-[var(--surface-1)] border border-[var(--hairline)] shadow-sm">
                      <div class="text-[13px] lg:text-sm text-[var(--text-secondary)]">
                        <MarkdownRenderer content={laneText} />
                      </div>
                      <span class="inline-block w-1.5 h-3.5 bg-[var(--brand)] animate-pulse ml-0.5 align-middle rounded-sm"></span>
                    </div>
                  {:else}
                    <div class="rounded-2xl rounded-tl-md px-4 py-3 bg-[var(--surface-1)] border border-[var(--hairline)] w-fit">
                      <span class="typing-dot"></span><span class="typing-dot"></span><span class="typing-dot"></span>
                    </div>
                  {/if}
                </div>
              </div>
            {/each}

            {#if activeMembers.length === 0}
              <div class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)] pl-1">
                <Loader2 class="size-3.5 animate-spin text-[var(--brand)]" />
                <span>{planGenerating ? "Dispatching the plan…" : "CEO is planning the work…"}</span>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>

    <!-- Grok Floating Capsule Compose Bar -->
    <div class="p-4 bg-[var(--surface-0)] shrink-0">
      <div class="max-w-3xl lg:max-w-4xl xl:max-w-5xl mx-auto rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]  p-3 shadow-2xl transition-all duration-200 focus-within:border-[var(--brand)]">
        {#if pendingAttachments.length}
          <div class="flex flex-wrap gap-1.5 pb-2">
            {#each pendingAttachments as att, idx}
              {#if att.isImage}
                <div class="relative size-12 rounded-lg overflow-hidden border border-[var(--hairline-strong)] bg-[var(--surface-2)]">
                  <img src={`data:${att.mime};base64,${att.data}`} alt={att.name} class="size-full object-cover" />
                  <button type="button" class="absolute top-0.5 right-0.5 size-4 rounded-full bg-[var(--surface-2)]lack/60 text-white text-[9px] flex items-center justify-center cursor-pointer hover:bg-danger/80" onclick={() => removePendingAttachment(idx)} title="Remove attachment">✕</button>
                </div>
              {:else}
                <div class="relative flex items-center gap-1.5 h-12 max-w-[200px] pl-2.5 pr-6 rounded-lg border border-[var(--hairline-strong)] bg-[var(--surface-2)]" title={att.name}>
                  <Paperclip class="size-3.5 text-[var(--brand-text)] shrink-0" />
                  <div class="min-w-0">
                    <div class="text-[10px] text-[var(--text-secondary)] truncate">{att.name}</div>
                    <div class="text-[9px] text-[var(--text-muted)] font-mono uppercase">{att.mime.split("/").pop()}</div>
                  </div>
                  <button type="button" class="absolute top-1 right-1 size-4 rounded-full bg-[var(--surface-2)]lack/60 text-white text-[9px] flex items-center justify-center cursor-pointer hover:bg-danger/80" onclick={() => removePendingAttachment(idx)} title="Remove attachment">✕</button>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
        <textarea
          bind:value={newMessage}
          placeholder={members.length === 0
            ? "Hire the team first (Users button) — the CEO will staff this office"
            : `Describe task for ${room.name} — will orchestrate across ${members.length} specialists...`}
          rows={1}
          class="w-full bg-transparent text-xs sm:text-sm text-white placeholder:text-[var(--text-muted)] resize-none focus:outline-none min-h-[44px] max-h-40 leading-relaxed font-sans"
          onkeydown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        ></textarea>

        <!-- Action Toolbar Inside Capsule -->
        <div class="flex items-center justify-between pt-2 border-t border-[var(--hairline)] mt-1">
          <div class="flex items-center gap-1.5">
            <button
              type="button"
              class="size-7 rounded-lg text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center transition-colors cursor-pointer"
              onclick={attachFile}
              title="Attach workspace code or text file"
            >
              <Paperclip class="size-3.5" />
            </button>

            <button
              type="button"
              class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {isListening ? 'text-danger bg-danger/20 border border-danger/50 animate-pulse shadow-sm' : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
              onclick={toggleVoice}
              title={isListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
            >
              <Mic class="size-3.5" />
            </button>

            <span class="text-[10px] font-mono text-[var(--text-muted)] ml-1">
              {members.length} Parallel Agents Assigned
            </span>
          </div>

          <button
            type="button"
            onclick={() => send()}
            disabled={((!newMessage.trim() && pendingAttachments.length === 0) || sending || members.length === 0)}
            class="h-8 px-4 rounded-full flex items-center gap-1.5 transition-all duration-200 cursor-pointer font-semibold text-xs {(newMessage.trim() || pendingAttachments.length) && !sending && members.length > 0
              ? 'btn-brand text-white hover:scale-[1.03] active:scale-95'
              : 'bg-[var(--surface-3)] text-[var(--text-muted)] cursor-not-allowed'}"
          >
            {#if sending}
              <Loader2 class="size-3.5 animate-spin" />
              <span>Splitting…</span>
            {:else}
              <ArrowUp class="size-3.5 stroke-[2.5]" />
              <span>Dispatch</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  </div>
</div>

{#if showOfficeSettings}
  <OfficeSettings
    room={room}
    {bots}
    open={showOfficeSettings}
    onClose={() => (showOfficeSettings = false)}
    onUpdated={(updated) => {
      room = updated;
      window.dispatchEvent(new CustomEvent("office-updated", { detail: { room: updated } }));
    }}
    onDeleted={() => {
      showOfficeSettings = false;
      window.dispatchEvent(new CustomEvent("office-deleted", { detail: { roomId: room?.id } }));
    }}
  />
{/if}

<!-- Plan Mode Modal -->
{#if showPlanModal}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--surface-2)]lack/60 " role="button" tabindex="0" aria-label="Close plan modal" onclick={(e) => { if (e.target === e.currentTarget) showPlanModal = false; }} onkeydown={(e) => { if (e.key === "Escape") showPlanModal = false; }}>
    <div class="modal-panel w-full max-w-2xl max-h-[85vh] flex flex-col">
      <div class="px-6 pt-5 pb-3 border-[var(--hairline)] border-[var(--hairline)] shrink-0">
        <h3 class="text-base font-bold text-white">Plan Mode — {room.name}</h3>
        <p class="text-xs text-[var(--text-tertiary)]">Break a goal into specialist tasks, set dependencies, dispatch as a DAG.</p>
      </div>

      <div class="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        <div class="space-y-1.5">
          <div class="flex items-center justify-between">
            <label for="plan-goal" class="text-xs font-bold text-white">Goal</label>
            <button
              type="button"
              onclick={draftPlan}
              disabled={draftingPlan || !planGoal.trim()}
              class="h-6 px-2.5 text-[10px] rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] text-[var(--brand-text)] hover:bg-[var(--brand-soft)] cursor-pointer disabled:opacity-40 flex items-center gap-1"
              title="Ask the office lead to break this goal into tasks"
            >
              {#if draftingPlan}
                <Loader2 class="size-3 animate-spin" /> Drafting…
              {:else}
                <Sparkles class="size-3" /> Draft with lead
              {/if}
            </button>
          </div>
          <textarea id="plan-goal" bind:value={planGoal} rows={2} placeholder="What should the team accomplish?" class="w-full px-3 py-2 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] text-sm text-white placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus:border-[var(--brand)]/50"></textarea>
        </div>

        {#if planQuestion}
          <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-xs text-[var(--brand-text)] space-y-1">
            <div class="font-bold flex items-center gap-1.5">❓ The lead needs clarification</div>
            <p>{planQuestion}</p>
            <p class="text-[10px] text-[var(--brand-text)]/70">Refine the goal above, then draft again — or add tasks manually.</p>
          </div>
        {/if}

        {#if members.length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each members as m}
              <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-tertiary)]">{m.rank} · {m.specialty}</span>
            {/each}
          </div>
        {/if}

        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <p class="text-xs font-bold text-white">Tasks ({planTasks.length})</p>
            <span onclick={addPlanTask} class="text-[10px] text-[var(--brand-text)] hover:text-[var(--brand-text)] cursor-pointer" role="button" tabindex="0" onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); addPlanTask(); } }}>+ Add task</span>
          </div>
          {#each planTasks as task, idx}
            <div class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] space-y-2">
              <div class="flex items-center gap-2">
                <SimpleSelect
                  value={task.botId}
                  options={members.map((m: any) => ({ value: m.bot_id, label: `${m.bot?.name || m.rank} · ${m.specialty}` }))}
                  onValueChange={(v) => updateTaskBot(idx, v)}
                  class="h-7 w-44 text-[10px] rounded-lg"
                />
                <input bind:value={task.label} oninput={(e) => updatePlanTaskLabel(idx, (e.target as HTMLInputElement).value)} class="flex-1 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-[var(--brand)]/50" placeholder="Task description..." />
                <button type="button" onclick={() => removePlanTask(idx)} class="text-[var(--text-muted)] hover:text-danger cursor-pointer">✕</button>
              </div>
            </div>
          {:else}
            <div class="py-6 text-center text-xs text-[var(--text-muted)] border-[var(--hairline)] border-dashed border-[var(--hairline)] rounded-xl">No tasks yet — add one to start.</div>
          {/each}
        </div>

        {#if planError}
          <div class="p-3 rounded-xl bg-danger/40 border border-danger/40 text-xs text-danger">{planError}</div>
        {/if}
      </div>

      <div class="px-6 py-3 border-t border-[var(--hairline)] flex justify-end gap-2 shrink-0">
        <button type="button" onclick={() => (showPlanModal = false)} class="h-8 px-4 text-xs bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-xl cursor-pointer">Cancel</button>
        <button type="button" onclick={dispatchPlan} disabled={planGenerating} class="h-8 px-4 text-xs bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-white font-medium rounded-xl cursor-pointer disabled:opacity-50">
          {#if planGenerating}
            <span>Dispatching...</span>
          {:else}
            <span>Dispatch Plan</span>
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Hire Team Modal — CEO proposes the office org -->
{#if showHireModal}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--surface-2)]lack/60 " role="button" tabindex="0" aria-label="Close hire modal" onclick={(e) => { if (e.target === e.currentTarget) showHireModal = false; }} onkeydown={(e) => { if (e.key === "Escape") showHireModal = false; }}>
    <div class="modal-panel w-full max-w-3xl max-h-[88vh] flex flex-col">
      <div class="px-6 pt-5 pb-3 border-[var(--hairline)] border-[var(--hairline)] shrink-0">
        <h3 class="text-base font-bold text-white flex items-center gap-2">
          <Users class="size-4 text-success" /> Hire Team — {room.name}
        </h3>
        <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
          The CEO proposes the agents this office needs. Edit the roster, then hire them — each agent
          is created with its role prompt and skills.
        </p>
      </div>

      <div class="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        <div class="space-y-1.5">
          <div class="flex items-center justify-between">
            <label for="hire-brief" class="text-xs font-bold text-white">What is this office for?</label>
            <span class="text-[10px] text-[var(--text-muted)]">
              {#if hireSource === "ceo"}CEO-proposed{:else if hireSource === "template"}Template blueprint{:else}Manual{/if}
            </span>
          </div>
          <textarea
            id="hire-brief"
            bind:value={hireBrief}
            rows={2}
            placeholder="e.g. Build and ship a production REST API with tests and a QA pass"
            class="w-full px-3 py-2 rounded-xl bg-[var(--surface-2)] border border-[var(--hairline)] text-sm text-white placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus:border-success/50"
          ></textarea>
          <button
            type="button"
            onclick={draftTeamWithCeo}
            disabled={hireDrafting}
            class="h-7 px-3 text-[11px] rounded-lg border border-success/40 bg-success/40 text-success hover:bg-success/50 cursor-pointer disabled:opacity-40 flex items-center gap-1.5"
          >
            {#if hireDrafting}
              <Loader2 class="size-3 animate-spin" /> Asking the CEO…
            {:else}
              <Sparkles class="size-3" /> Let the CEO propose the team
            {/if}
          </button>
          <p class="text-[10px] text-[var(--text-muted)] leading-relaxed pt-0.5">
            The CEO checks your whole fleet first — existing agents that match a role are reused
            (and just added to this office), so the same agent is never created twice. Every agent's
            model, skills, MCP servers and connectors stay editable by you from the roster above.
          </p>
        </div>

        {#if hireQuestion}
          <div class="p-3 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-xs text-[var(--brand-text)] space-y-1">
            <div class="font-bold">❓ The CEO needs clarification</div>
            <p>{hireQuestion}</p>
          </div>
        {/if}

        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <p class="text-xs font-bold text-white">Roster ({hireRoles.length})</p>
            <span onclick={addHireRole} class="text-[10px] text-success hover:text-success cursor-pointer" role="button" tabindex="0" onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); addHireRole(); } }}>+ Add role</span>
          </div>

          {#each hireRoles as role, idx}
            <div class="p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] space-y-2">
              <div class="flex items-center gap-2">
                <label class="flex items-center gap-1 text-[10px] text-[var(--text-tertiary)] shrink-0 cursor-pointer" title="Make this the office lead (CEO)">
                  <input type="radio" name="hire-lead" checked={role.is_lead} onchange={() => setHireLead(idx)} class="accent-[var(--brand)] cursor-pointer" />
                  Lead
                </label>
                <input bind:value={role.name} placeholder="Name" class="w-28 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <input bind:value={role.rank} placeholder="Rank" class="w-24 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <input bind:value={role.specialty} placeholder="Specialty" class="flex-1 h-7 px-2 text-[10px] bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg focus:outline-none focus:border-success/50" />
                <button type="button" onclick={() => removeHireRole(idx)} class="text-[var(--text-muted)] hover:text-danger cursor-pointer">✕</button>
              </div>
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  onclick={() => (openRolePrompt = openRolePrompt === idx ? null : idx)}
                  class="text-[10px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] cursor-pointer"
                >
                  {openRolePrompt === idx ? "▾ Hide instructions" : "▸ Role instructions"}
                </button>
                {#if role.is_lead}
                  <span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] font-bold">CEO / ORCHESTRATOR</span>
                {/if}
                {#if role.skills.length}
                  <span class="text-[9px] text-[var(--text-muted)] truncate">{role.skills.length} skills</span>
                {/if}
              </div>
              {#if openRolePrompt === idx}
                <textarea
                  bind:value={role.system_prompt}
                  rows={3}
                  placeholder="Operating instructions for this agent…"
                  class="w-full px-2 py-1.5 text-[11px] bg-[var(--surface-1)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-lg resize-none focus:outline-none focus:border-success/50"
                ></textarea>
              {/if}
            </div>
          {:else}
            <div class="py-6 text-center text-xs text-[var(--text-muted)] border-[var(--hairline)] border-dashed border-[var(--hairline)] rounded-xl">
              No roles yet — ask the CEO to propose a team, or add roles manually.
            </div>
          {/each}
        </div>

        {#if hireError}
          <div class="p-3 rounded-xl bg-danger/40 border border-danger/40 text-xs text-danger">{hireError}</div>
        {/if}

        {#if hireResult}
          <div class="p-3 rounded-xl bg-success/40 border border-success/40 text-xs text-success flex items-center gap-2">
            <CheckCircle2 class="size-3.5 shrink-0" />
            <span>{hireResult}</span>
          </div>
        {/if}
      </div>

      <div class="px-6 py-3 border-t border-[var(--hairline)] flex justify-end gap-2 shrink-0">
        {#if hireResult}
          <button type="button" onclick={() => (showHireModal = false)} class="h-8 px-4 text-xs bg-success hover:bg-success text-white font-medium rounded-xl cursor-pointer">Done</button>
        {:else}
          <button type="button" onclick={() => (showHireModal = false)} class="h-8 px-4 text-xs bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-xl cursor-pointer">Cancel</button>
          <button
            type="button"
            onclick={provisionHiredTeam}
            disabled={hireProvisioning || hireRoles.length === 0}
            class="h-8 px-4 text-xs bg-success hover:bg-success text-white font-medium rounded-xl cursor-pointer disabled:opacity-50 flex items-center gap-1.5"
          >
            {#if hireProvisioning}
              <Loader2 class="size-3.5 animate-spin" /> Hiring…
            {:else}
              Hire {hireRoles.length} agent{hireRoles.length === 1 ? "" : "s"}
            {/if}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}


<!-- Manage Agent chooser — owner can edit model, skills, MCP & connectors -->
{#if manageBot && !showBotSettings && !showSkillManager && !showMcpManager}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--surface-2)]lack/60 "
    role="button"
    tabindex="0"
    aria-label="Close manage agent"
    onclick={(e) => { if (e.target === e.currentTarget) manageBot = null; }}
    onkeydown={(e) => { if (e.key === "Escape") manageBot = null; }}
  >
    <div class="modal-panel w-full max-w-md p-5 space-y-4 animate-scale-in">
      <div class="flex items-center gap-3">
        <div class="size-11 rounded-xl overflow-hidden bg-[var(--surface-2)] border border-[var(--hairline-strong)] shrink-0">
          <img
            src={manageBot.avatar_url || getDiceBearUrl(manageBot.name, manageBot.avatar_style || "bottts")}
            alt={manageBot.name}
            class="size-full object-cover"
          />
        </div>
        <div class="min-w-0">
          <div class="font-bold text-sm text-white truncate flex items-center gap-1.5">
            {manageBot.name}
            {#if manageBot.is_orchestrator}<span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--brand-soft)] border border-[var(--brand)]/40 text-[var(--brand-text)] font-bold">LEAD</span>{/if}
          </div>
          <div class="text-[11px] text-[var(--text-tertiary)] truncate">
            {manageBot.rank || "Agent"} · {manageBot.specialty || "Generalist"}
          </div>
          <div class="text-[10px] font-mono text-[var(--text-muted)] truncate mt-0.5">
            {manageBot.config?.model_provider || "?"}/{manageBot.config?.model_id || "?"}
          </div>
        </div>
      </div>

      <div class="grid grid-cols-1 gap-2">
        <button
          type="button"
          class="w-full flex items-center gap-3 p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)] hover:bg-[var(--surface-2)] transition-colors cursor-pointer text-left"
          onclick={() => (showBotSettings = true)}
        >
          <Cpu class="size-4 text-[var(--brand)] shrink-0" />
          <span class="min-w-0">
            <span class="block text-xs font-bold text-white">Model & Engine</span>
            <span class="block text-[10px] text-[var(--text-tertiary)]">Provider, model id, temperature, sandbox, system prompt</span>
          </span>
        </button>

        <button
          type="button"
          class="w-full flex items-center gap-3 p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)] hover:bg-[var(--surface-2)] transition-colors cursor-pointer text-left"
          onclick={() => (showSkillManager = true)}
        >
          <Wrench class="size-4 text-[var(--brand)] shrink-0" />
          <span class="min-w-0">
            <span class="block text-xs font-bold text-white">Skills</span>
            <span class="block text-[10px] text-[var(--text-tertiary)]">Enable tools and workflow skills for this agent</span>
          </span>
        </button>

        <button
          type="button"
          class="w-full flex items-center gap-3 p-3 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)] hover:bg-[var(--surface-2)] transition-colors cursor-pointer text-left"
          onclick={() => (showMcpManager = true)}
        >
          <Server class="size-4 text-[var(--brand)] shrink-0" />
          <span class="min-w-0">
            <span class="block text-xs font-bold text-white">MCP Tools & Connectors</span>
            <span class="block text-[10px] text-[var(--text-tertiary)]">Assign connectors and MCP servers to this agent</span>
          </span>
        </button>
      </div>

      <div class="flex justify-end">
        <button type="button" onclick={() => (manageBot = null)} class="h-8 px-4 text-xs bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] rounded-xl cursor-pointer">Close</button>
      </div>
    </div>
  </div>
{/if}

{#if showBotSettings && manageBot}
  {#await import("$lib/components/BotSettings.svelte") then BotSettings}
    <BotSettings.default
      bot={manageBot}
      open={showBotSettings}
      onClose={() => { showBotSettings = false; manageBot = null; }}
      onUpdated={(updated: any) => { handleAgentUpdated(updated); }}
    />
  {/await}
{/if}

{#if showSkillManager && manageBot}
  {#await import("$lib/components/SkillManager.svelte") then SkillManager}
    <SkillManager.default
      bot={manageBot}
      open={showSkillManager}
      onClose={() => { showSkillManager = false; manageBot = null; }}
      onUpdated={(updated: any) => { handleAgentUpdated(updated); }}
    />
  {/await}
{/if}

{#if showMcpManager && manageBot}
  {#await import("$lib/components/McpManager.svelte") then McpManager}
    <McpManager.default
      bot={manageBot}
      open={showMcpManager}
      onClose={() => { showMcpManager = false; manageBot = null; }}
    />
  {/await}
{/if}
