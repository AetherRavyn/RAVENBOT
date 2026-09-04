<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import * as Avatar from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as Card from "$lib/components/ui/card";
  import { Separator } from "$lib/components/ui/separator";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { getDiceBearUrl, OFFICE_TEMPLATES } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import { onMount, onDestroy, tick } from "svelte";
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
    Layers,
    Cpu,
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

  // User avatar from localStorage (chosen via Settings)
  let userAvatar = $state<string | null>(null);
  // Live office telemetry from the runtime stream (per-agent status + usage)
  let agentStatus = $state<Record<string, string>>({});
  // Live per-agent token streams (lanes) during a team run
  let lanes = $state<Record<string, string>>({});
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
        } else if (p.kind === "clear") {
          if (p.bot_id) {
            lanes = { ...lanes, [p.bot_id]: "" };
          }
        } else if (p.kind === "done") {
          lanes = {};
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

  async function send(taskText?: string) {
    const text = (taskText || newMessage).trim();
    if (!text || sending) return;

    sending = true;
    newMessage = "";
    lanes = {};
    agentStatus = {};

    // Optimistically insert user prompt
    const tempUserMsg = {
      id: "temp-" + Date.now(),
      role: "user",
      content: text,
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempUserMsg];
    scrollToBottom();

    try {
      const res: any = await invoke("send_to_chatroom", { chatroomId: room.id, content: text });
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
  let recognition: any = null;

  function attachFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".txt,.md,.rs,.ts,.js,.py,.json,.toml,.yaml,.yml,.css,.html,.sh";
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (file) {
        const text = await file.text();
        const ext = file.name.split(".").pop() || "text";
        newMessage = (newMessage ? newMessage + "\n\n" : "") + `Attached file [${file.name}]:\n\`\`\`${ext}\n${text}\n\`\`\`\n`;
      }
    };
    input.click();
  }

  function toggleVoice() {
    const SpeechRecognition = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    if (!SpeechRecognition) {
      alert("Speech recognition is not supported in this environment. You can type directly in the composer.");
      return;
    }
    if (isListening) {
      recognition?.stop();
      isListening = false;
      return;
    }
    try {
      recognition = new SpeechRecognition();
      recognition.continuous = false;
      recognition.interimResults = true;
      recognition.lang = "en-US";
      recognition.onstart = () => { isListening = true; };
      recognition.onresult = (event: any) => {
        let text = "";
        for (let i = event.resultIndex; i < event.results.length; ++i) {
          text += event.results[i][0].transcript;
        }
        if (text) {
          newMessage = (newMessage ? newMessage + " " : "") + text.trim();
        }
      };
      recognition.onerror = () => { isListening = false; };
      recognition.onend = () => { isListening = false; };
      recognition.start();
    } catch {
      isListening = false;
    }
  }
</script>

<div class="flex flex-col h-full overflow-hidden select-none bg-[#07070a] text-zinc-100 font-sans">
  <!-- Top Office Header Bar -->
  <header class="h-15 px-4 border-b border-[#1a1a28] bg-[#09090e] flex items-center justify-between z-10 shrink-0">
    <div class="flex items-center gap-3">
      <!-- Office Avatar -->
      <div class="size-10 rounded-2xl overflow-hidden bg-[#181826] border-2 border-purple-500/40 p-0.5 shrink-0 shadow-md">
        <img
          src={room.avatar_url || getDiceBearUrl(room.name, room.avatar_style || "bottts")}
          alt={room.name}
          class="size-full rounded-xl object-cover"
        />
      </div>

      <div class="flex flex-col">
        <div class="flex items-center gap-2">
          <span class="font-bold text-sm text-white">{room.name}</span>
          <span class="font-mono text-[10px] py-0.2 px-2 rounded-md bg-[#161624] border border-[#27273a] text-purple-300 capitalize">
            {room.office_template.replace("-", " ")}
          </span>
        </div>
        <div class="flex items-center gap-2 text-[11px] text-zinc-400 mt-0.5">
          <span class="text-emerald-400 font-mono flex items-center gap-1">
            <Radio class="size-2.5" />
            Parallel Lane
          </span>
          <span class="text-zinc-600">·</span>
          <span>{members.length} {members.length === 1 ? 'Specialist' : 'Specialists'} Assigned</span>
          <span class="text-zinc-600">·</span>
          <span class="font-mono text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/10 border border-emerald-500/30 text-emerald-300" title="Live office telemetry (tokens / cost this session)">
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
            class="size-8 rounded-full overflow-hidden bg-[#181826] border-2 border-[#09090e] ring-1 ring-purple-500/40 shadow-sm transition-transform hover:scale-110 hover:z-10"
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
          <div class="size-8 rounded-full bg-[#181826] border-2 border-[#09090e] ring-1 ring-purple-500/40 flex items-center justify-center text-[10px] font-bold text-purple-300">
            +{members.length - 5}
          </div>
        {/if}
      </div>
      <button type="button" onclick={() => (showOfficeSettings = true)} class="size-8 rounded-xl bg-[#1a1a28] border border-[#2a2a3e] text-zinc-400 hover:text-white hover:border-purple-500/50 flex items-center justify-center shrink-0 ml-1" title="Office Settings — members, policy, budget, goal, memory" aria-label="Open office settings">
        <Settings class="size-4" />
      </button>
    </div>
  </header>

  <!-- Office Lane Roles Bar -->
  {#if members.length > 0}
    <div class="px-4 py-2 bg-[#0c0c14] border-b border-[#181824] flex items-center gap-2 overflow-x-auto no-scrollbar shrink-0">
      <span class="text-[10px] font-bold uppercase tracking-wider text-zinc-500 shrink-0 flex items-center gap-1">
        <Workflow class="size-3 text-purple-400" />
        Roles:
      </span>
      <div class="flex items-center gap-1.5 min-w-0">
        {#each members as m}
          {@const st = agentStatus[m.bot?.id] || "idle"}
          <div class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-xl bg-[#141420] border border-[#232334] text-xs shrink-0 {st !== 'idle' ? 'border-purple-500/40' : ''}">
            <div class="size-4 rounded-full overflow-hidden shrink-0 border border-white/20">
              <img
                src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "bottts")}
                alt=""
                class="size-full object-cover"
              />
            </div>
            <span class="font-bold text-white text-[11px]">{m.rank}</span>
            <span class="text-zinc-400 text-[10px]">· {m.specialty}</span>
            <span class="size-1.5 rounded-full shrink-0 {st === 'thinking'
              ? 'bg-amber-400 animate-pulse'
              : st === 'running_tool'
                ? 'bg-sky-400 animate-pulse'
                : st === 'idle'
                  ? 'bg-emerald-400'
                  : 'bg-zinc-600'}" title={st}></span>
            {#if st !== 'idle'}
              <span class="text-[9px] font-mono {st === 'thinking' ? 'text-amber-300' : 'text-sky-300'}">
                {st === 'thinking' ? 'thinking…' : st === 'running_tool' ? 'tool…' : st}
              </span>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Main Chat & Task Feed -->
  <div class="flex-1 flex flex-col overflow-hidden bg-[#000000]">
    <div bind:this={chatContainer} class="flex-1 overflow-y-auto p-4 sm:p-6 space-y-6">
      <div class="max-w-3xl mx-auto space-y-6">
        {#each messages as msg (msg.id || msg.created_at)}
          {@const isUser = msg.role === "user"}
          {@const isError = typeof msg.content === "string" && msg.content.includes("⚠️ **Model Error:**")}
          {@const rawText = typeof msg.content === "string" ? msg.content : msg.content?.text || JSON.stringify(msg.content)}

          <div class="flex gap-3.5 {isUser ? 'justify-end' : 'justify-start'} group">
            {#if !isUser}
              <div class="size-8 rounded-xl overflow-hidden bg-[#121218] border border-white/10 shrink-0 mt-1 shadow-sm">
                <img
                  src={room.avatar_url || getDiceBearUrl(room.name, room.avatar_style || "bottts")}
                  alt={room.name}
                  class="size-full object-cover"
                />
              </div>
            {/if}

            <div class="max-w-[85%] sm:max-w-[78%] space-y-1.5">
              {#if isError}
                <div class="rounded-2xl p-4 bg-red-950/30 border border-red-800/40 text-zinc-200 space-y-2 shadow-xl">
                  <div class="flex items-center gap-2 text-red-400 font-bold text-xs font-mono">
                    <Shield class="size-4 shrink-0" />
                    <span>Office Pipeline Error</span>
                  </div>
                  <p class="text-xs text-zinc-300 leading-relaxed font-sans">
                    {rawText.replace("⚠️ **Model Error:** ", "")}
                  </p>
                </div>
              {:else if isUser}
                <div class="rounded-2xl px-4 py-3 text-xs leading-relaxed text-zinc-100 bg-[#1e1e24] border border-[#2f2f38] shadow-md selection:bg-sky-500/30">
                  <p class="whitespace-pre-wrap font-sans text-xs leading-relaxed">{rawText}</p>
                </div>
              {:else}
                <div class="rounded-2xl px-4 py-3.5 bg-[#0b0b10] border border-white/10 shadow-sm space-y-2 text-zinc-200 selection:bg-sky-500/30">
                  <MarkdownRenderer content={rawText} />
                </div>
              {/if}

              <div class="text-[10px] text-zinc-500 px-1 {isUser ? 'text-right' : 'text-left'}">
                {formatTime(msg.created_at)}
              </div>
            </div>

            {#if isUser}
              <div class="size-8 rounded-full overflow-hidden bg-[#181820] border border-white/15 shrink-0 mt-1 shadow-sm">
                <img src={userAvatar || getDiceBearUrl("You", "micah")} alt="You" class="size-full object-cover" />
              </div>
            {/if}
          </div>
        {:else}
          <!-- Empty State: Modern Office Mission Control -->
          <div class="p-8 text-center border border-dashed border-white/10 rounded-3xl bg-[#09090e]/80 max-w-xl mx-auto my-6 shadow-xl space-y-4">
            <div class="size-16 rounded-2xl bg-white/5 border border-white/15 mx-auto flex items-center justify-center text-sky-400 shadow-2xl">
              <Building2 class="size-8" />
            </div>

            <div>
              <h3 class="font-bold text-base text-white tracking-tight">
                {room.name} Workspace
              </h3>
              <p class="text-xs text-zinc-400 mt-1 max-w-md mx-auto leading-relaxed">
                {room.description || "Multi-agent collaborative pipeline. Directives are automatically orchestrated across assigned team specialists in parallel."}
              </p>
            </div>

            <!-- Pre-configured Team Lane Overview -->
            {#if members.length > 0}
              <div class="flex items-center justify-center gap-3 pt-2">
                {#each members as m}
                  <div class="flex flex-col items-center gap-1">
                    <div class="size-9 rounded-xl overflow-hidden bg-[#14141c] border border-white/15 shadow-md">
                      <img
                        src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "bottts")}
                        alt={m.rank}
                        class="size-full object-cover"
                      />
                    </div>
                    <span class="text-[10px] font-bold text-zinc-300 font-mono">{m.rank}</span>
                  </div>
                {/each}
              </div>
            {/if}

            <!-- Quick Starter Tasks -->
            <div class="space-y-2 pt-4 text-left">
              <span class="text-[10px] font-bold text-zinc-500 uppercase tracking-wider font-mono flex items-center gap-1.5">
                <Sparkles class="size-3 text-sky-400" />
                Collaborative Directives:
              </span>
              <div class="grid grid-cols-1 gap-2">
                {#each roomTasks as task}
                  <button
                    type="button"
                    class="text-left text-xs p-3 rounded-xl border border-white/10 bg-[#0d0d14] hover:border-zinc-500 hover:bg-[#13131c] transition-all text-zinc-300 hover:text-white flex items-center justify-between group cursor-pointer"
                    onclick={() => send(task)}
                  >
                    <span class="truncate">{task}</span>
                    <ArrowRight class="size-3.5 text-sky-400 opacity-0 group-hover:opacity-100 transition-opacity shrink-0 ml-2" />
                  </button>
                {/each}
              </div>
            </div>
          </div>
        {/each}

        <!-- Live Agent Lanes: watch the team think, per specialist -->
        {#if sending}
          <div class="space-y-2.5">
            {#each members as m (m.bot?.id)}
              {@const st = agentStatus[m.bot?.id] || "idle"}
              {@const laneText = lanes[m.bot?.id] || ""}
              {#if st !== "idle" || laneText}
                <div class="flex gap-3 justify-start">
                  <div class="size-8 rounded-xl overflow-hidden bg-[#14141c] border border-purple-500/30 shrink-0 mt-1 shadow-sm">
                    <img
                      src={m.bot?.avatar_url || getDiceBearUrl(m.bot?.name || m.rank, m.bot?.avatar_style || "avataaars")}
                      alt={m.rank}
                      class="size-full object-cover"
                    />
                  </div>
                  <div class="max-w-[80%] space-y-1">
                    <div class="flex items-center gap-1.5 text-[10px] font-mono">
                      <span class="font-bold text-white">{m.rank}</span>
                      <span class="text-zinc-500">· {m.specialty}</span>
                      <span class="text-[9px] {st === 'running_tool' ? 'text-sky-400' : 'text-amber-400'}">
                        {st === 'running_tool' ? 'running tool…' : st === 'thinking' ? 'thinking…' : st}
                      </span>
                    </div>
                    {#if laneText}
                      <div class="rounded-2xl px-4 py-3 bg-[#1e1e24] border border-[#2f2f38] shadow-md selection:bg-sky-500/30">
                        <div class="text-zinc-200">
                          <MarkdownRenderer content={laneText} />
                        </div>
                        <span class="inline-block w-1.5 h-3.5 bg-purple-400 animate-pulse ml-0.5 align-middle rounded-sm"></span>
                      </div>
                    {:else}
                      <div class="rounded-2xl px-4 py-3 bg-[#1e1e24] border border-[#2f2f38] shadow-md w-fit">
                        <div class="flex items-center gap-1.5">
                          <span class="size-1.5 rounded-full bg-zinc-400 animate-bounce"></span>
                          <span class="size-1.5 rounded-full bg-zinc-400 animate-bounce [animation-delay:150ms]"></span>
                          <span class="size-1.5 rounded-full bg-zinc-400 animate-bounce [animation-delay:300ms]"></span>
                        </div>
                      </div>
                    {/if}
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <!-- Grok Floating Capsule Compose Bar -->
    <div class="p-4 bg-gradient-to-t from-black via-black/90 to-transparent shrink-0">
      <div class="max-w-3xl mx-auto rounded-2xl border border-zinc-800 bg-[#0d0d12]/95 backdrop-blur-xl p-3 shadow-2xl focus-within:border-zinc-600 transition-all">
        <textarea
          bind:value={newMessage}
          placeholder="Describe task for {room.name} — will orchestrate across {members.length || 'all'} specialists..."
          rows={1}
          class="w-full bg-transparent text-xs sm:text-sm text-white placeholder:text-zinc-500 resize-none focus:outline-none min-h-[44px] max-h-40 leading-relaxed font-sans"
          onkeydown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        ></textarea>

        <!-- Action Toolbar Inside Capsule -->
        <div class="flex items-center justify-between pt-2 border-t border-white/5 mt-1">
          <div class="flex items-center gap-1.5">
            <button
              type="button"
              class="size-7 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-white/5 flex items-center justify-center transition-colors cursor-pointer"
              onclick={attachFile}
              title="Attach workspace code or text file"
            >
              <Paperclip class="size-3.5" />
            </button>

            <button
              type="button"
              class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {isListening ? 'text-rose-400 bg-rose-500/20 border border-rose-500/50 animate-pulse shadow-sm' : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5'}"
              onclick={toggleVoice}
              title={isListening ? "Listening... (Click to stop speech-to-text)" : "Voice input (Speech-to-Text)"}
            >
              <Mic class="size-3.5" />
            </button>

            <span class="text-[10px] font-mono text-zinc-500 ml-1">
              {members.length} Parallel Agents Assigned
            </span>
          </div>

          <button
            type="button"
            onclick={() => send()}
            disabled={!newMessage.trim() || sending}
            class="h-8 px-4 rounded-full flex items-center gap-1.5 transition-all cursor-pointer font-medium text-xs {newMessage.trim() && !sending
              ? 'bg-white text-black hover:bg-zinc-200 shadow-md scale-105'
              : 'bg-zinc-800 text-zinc-500 cursor-not-allowed opacity-50'}"
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

<style>
  .no-scrollbar::-webkit-scrollbar { display: none; }
  .no-scrollbar { -ms-overflow-style: none; scrollbar-width: none; }
</style>
