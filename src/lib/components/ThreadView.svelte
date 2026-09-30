<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy, tick } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import AgentIntelligence from "$lib/components/AgentIntelligence.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import ArtifactPanel from "$lib/components/ArtifactPanel.svelte";
  import RoutinesPanel from "$lib/components/RoutinesPanel.svelte";
  import SyncPanel from "$lib/components/SyncPanel.svelte";
  import type { Artifact } from "$lib/artifact";
  import { getDiceBearUrl } from "$lib/utils";
  import { cn } from "$lib/utils.js";
  import {
    Plus,
    DollarSign,
    Pause,
    Play,
    Paperclip,
    Mic,
    Loader2,
    Volume2,
    Ghost,
    Clock,
    Pencil,
    CheckCircle2,
    XCircle,
    Circle,
    Sparkles,
    MessageSquare,
    Brain,
    AlertTriangle,
    Key,
    Settings,
    ArrowUp,
    ChevronDown,
    Copy,
    Check,
    RotateCcw,
    Boxes,
    Search,
    Globe,
    History,
    Trash2,
    Terminal,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    onBotUpdated?: (bot: any) => void;
  }

  let { bot, onBotUpdated }: Props = $props();

  let threads = $state<any[]>([]);
  let selectedThreadId = $state<string | null>(null);
  let messages = $state<any[]>([]);
  let newMessage = $state("");
  let sending = $state(false);
  let showCostInfo = $state(false);
  let showIntelligence = $state(false);
  let showThreadDrawer = $state(false);
  let showThreadDropdown = $state(false);
  let showRoutines = $state(false);
  let showSync = $state(false);
  // Model quick switcher (per-conversation, Grok-style)
  let showModelSwitcher = $state(false);
  let switcherModel = $state("");
  // Inline image attachments (paste/drop) sent with the next message
  let pendingImages = $state<{ name: string; mime: string; data: string }[]>([]);
  let sessionTokens = $state(0);
  let sessionCost = $state(0.0);
  let userAvatar = $state<string | null>(null);
  let messagesContainer = $state<HTMLDivElement | null>(null);
  let textareaRef = $state<HTMLTextAreaElement | null>(null);
  let deepSearchActive = $state(false);
  let thinkActive = $state(false);
  let copiedMessageId = $state<string | null>(null);

  // Live streaming state (tokens arrive over the agent-stream event channel)
  let streamingText = $state("");
  let streamingTool = $state<string | null>(null);
  let streamingSources = $state<any[]>([]);
  let regenerating = $state(false);
  let renamingThreadId = $state<string | null>(null);
  let renameValue = $state("");
  // Cross-thread message search
  let searchQuery = $state("");
  let searchResults = $state<any[]>([]);
  let searchPerformed = $state(false);

  async function runSearch() {
    const q = searchQuery.trim();
    if (!q) return;
    try {
      searchResults = await invoke("search_messages", { query: q, limit: 15 });
      searchPerformed = true;
    } catch (e) {
      console.error("Search failed:", e);
    }
  }
  let openArtifact = $state<Artifact | null>(null);
  // Temporary (ephemeral) chat mode: new threads skip agent-memory persistence
  let tempActive = $state(false);
  // Edit-and-resend: editing a prior user turn removes everything after it
  let editingMessage = $state<{ id: string; original: string } | null>(null);
  // Hands-free voice mode: STT → send → TTS loop
  let voiceMode = $state(false);
  let speaking = $state(false);
  let unlisten: UnlistenFn | null = null;

  async function openSource(url: string) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch (e) {
      console.error("Failed to open source:", e);
    }
  }

  function domainOf(url: string) {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  let botStatusTheme = $derived(getStatusTheme(bot?.status));
  let currentThread = $derived(threads.find((t) => t.id === selectedThreadId));

  $effect(() => {
    if (typeof localStorage !== "undefined") {
      userAvatar = localStorage.getItem("ravenbot_user_avatar");
      const h = () => (userAvatar = localStorage.getItem("ravenbot_user_avatar"));
      window.addEventListener("user-avatar-changed", h);
      return () => window.removeEventListener("user-avatar-changed", h);
    }
  });

  const samplePrompts = [
    { title: "Analyze Codebase", desc: "Inspect repo structure & identify optimization points", icon: Terminal },
    { title: "Task Graph Plan", desc: "Draft a parallel multi-agent execution strategy", icon: Brain },
    { title: "Health Check", desc: "Summarize active agent status, memory & telemetry", icon: Sparkles },
    { title: "Security Audit", desc: "Verify local sandbox isolation, eBPF & quotas", icon: Globe },
  ];

  onMount(async () => {
    try {
      threads = await invoke("list_threads", { botId: bot.id });
      if (threads.length > 0) {
        await loadMessages(threads[0].id);
      }
    } catch (e) {
      console.error("Failed to load threads:", e);
    }

    // Live token streaming from the runtime
    try {
      unlisten = await listen<any>("agent-stream", (event) => {
        const payload = event.payload;
        const tid = payload?.thread_id;
        if (!tid || tid !== selectedThreadId) return;
        switch (payload?.kind) {
          case "delta":
            streamingText += payload.content || "";
            scrollToBottom();
            break;
          case "clear":
            streamingText = "";
            break;
          case "tool_started":
            streamingTool = payload.name;
            break;
          case "tool_finished":
            streamingTool = null;
            break;
          case "sources":
            for (const src of payload?.sources || []) {
              if (src?.url && !streamingSources.some((s) => s.url === src.url)) {
                streamingSources = [...streamingSources, src];
              }
            }
            break;
          case "done":
            streamingText = "";
            streamingTool = null;
            streamingSources = [];
            break;
          case "status":
            // Live status ring (thinking / running_tool / done → idle)
            if (payload?.bot_id === bot.id) {
              const nextStatus = payload.state === "done" ? "idle" : payload.state;
              if (bot.status !== nextStatus) {
                onBotUpdated?.({ ...bot, status: nextStatus });
              }
            }
            break;
          case "usage":
            // Real telemetry from the completed run
            if (payload?.thread_id === selectedThreadId) {
              sessionTokens += payload.tokens || 0;
              sessionCost += payload.cost || 0;
            }
            break;
        }
      });
    } catch (e) {
      console.error("Failed to attach stream listener:", e);
    }
  });

  onDestroy(() => {
    unlisten?.();
    unlisten = null;
    stopVoiceMode();
  });

  $effect(() => {
    if (bot?.id) {
      threads = [];
      selectedThreadId = null;
      messages = [];
      streamingText = "";
      streamingSources = [];
      // Lifetime telemetry baseline (events keep it live afterwards)
      invoke("get_session_usage", { botId: bot.id })
        .then((res: any) => {
          sessionTokens = res?.tokens || 0;
          sessionCost = res?.cost || 0;
        })
        .catch(console.error);
      invoke("list_threads", { botId: bot.id })
        .then((result) => {
          threads = result as any[];
          if (threads.length > 0) {
            loadMessages(threads[0].id);
          }
        })
        .catch(console.error);
    }
  });

  async function scrollToBottom() {
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }

  async function loadMessages(threadId: string) {
    selectedThreadId = threadId;
    showThreadDropdown = false;
    streamingText = "";
    streamingTool = null;
    streamingSources = [];
    editingMessage = null;
    try {
      messages = await invoke("list_messages", { threadId });
      scrollToBottom();
    } catch (e) {
      console.error("Failed to load messages:", e);
    }
  }

  async function createNewThread() {
    try {
      const thread = await invoke("create_thread", {
        botId: bot.id,
        title: `Thread #${threads.length + 1}`,
        ephemeral: tempActive,
      });
      threads = [thread, ...threads];
      selectedThreadId = (thread as any).id;
      messages = [];
      showThreadDropdown = false;
      await tick();
      textareaRef?.focus();
    } catch (e) {
      console.error("Failed to create thread:", e);
    }
  }

  function triggerOpenSettings() {
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("open-settings"));
    }
  }

  async function togglePause() {
    try {
      if (bot.status === "paused") {
        await invoke("resume_all");
        onBotUpdated?.({ ...bot, status: "idle" });
      } else {
        await invoke("pause_all");
        onBotUpdated?.({ ...bot, status: "paused" });
      }
    } catch (e) {
      console.error("Failed to toggle pause:", e);
    }
  }

  async function renameThread(threadId: string) {
    if (renamingThreadId === threadId) {
      // Second click: commit
      const title = renameValue.trim();
      if (title) {
        try {
          await invoke("rename_thread", { threadId, title });
          threads = threads.map((t) => (t.id === threadId ? { ...t, title } : t));
        } catch (e) {
          console.error("Failed to rename thread:", e);
        }
      }
      renamingThreadId = null;
    } else {
      renamingThreadId = threadId;
      renameValue = threads.find((t) => t.id === threadId)?.title || "";
    }
  }

  let deleteArmed = $state<string | null>(null);

  async function deleteThread(threadId: string) {
    if (deleteArmed !== threadId) {
      // First click: arm (two-step confirm, no native confirm() in webview)
      deleteArmed = threadId;
      setTimeout(() => {
        if (deleteArmed === threadId) deleteArmed = null;
      }, 3000);
      return;
    }
    deleteArmed = null;
    try {
      await invoke("delete_thread", { threadId });
      threads = threads.filter((t) => t.id !== threadId);
      if (selectedThreadId === threadId) {
        selectedThreadId = null;
        messages = [];
        if (threads.length > 0) {
          await loadMessages(threads[0].id);
        }
      }
    } catch (e) {
      console.error("Failed to delete thread:", e);
    }
  }

  async function sendMessage(textToSend?: string) {
    const rawText = (textToSend || newMessage).trim();
    if (!rawText || sending) return;

    // Apply DeepSearch or Think prefixes if toggled
    let text = rawText;
    if (deepSearchActive && !text.startsWith("[DeepSearch]")) {
      text = `[DeepSearch] ${text}`;
    }
    if (thinkActive && !text.startsWith("[Think]")) {
      text = `[Think] ${text}`;
    }

    sending = true;
    newMessage = "";
    if (textareaRef) {
      textareaRef.style.height = "auto";
    }

    try {
      if (!selectedThreadId) {
        const thread = await invoke("create_thread", {
          botId: bot.id,
          title: rawText.slice(0, 35) + (rawText.length > 35 ? "..." : ""),
          ephemeral: tempActive,
        });
        threads = [thread, ...threads];
        selectedThreadId = (thread as any).id;
      }

      // Optimistically show user message right away so it is NEVER lost
      const tempAttachments = pendingImages.map((p) => ({
        id: "temp-att-" + Date.now() + "-" + p.name,
        name: p.name,
        mime_type: p.mime,
        size: p.data.length,
        path: "",
        data: p.data,
        is_image: true,
      }));
      const tempUserMsg = {
        id: "temp-" + Date.now(),
        thread_id: selectedThreadId,
        role: "user",
        content: text,
        attachments: tempAttachments,
        created_at: new Date().toISOString(),
      };
      messages = [...messages, tempUserMsg];
      scrollToBottom();

      if (editingMessage) {
        // Edit-and-resend: backend removes this turn + everything after it
        await invoke("edit_and_resend", {
          threadId: selectedThreadId,
          messageId: editingMessage.id,
          content: text,
          attachments: pendingImages.length ? pendingImages : undefined,
        });
        editingMessage = null;
      } else {
        await invoke("send_message", {
          threadId: selectedThreadId,
          content: text,
          attachments: pendingImages.length ? pendingImages : undefined,
        });
      }
      pendingImages = [];

      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
    } catch (e: any) {
      console.error("Failed to send message:", e);
      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
    } finally {
      sending = false;
      streamingText = "";
      streamingTool = null;
      streamingSources = [];
      editingMessage = null;
      scrollToBottom();

      // Voice mode: speak the response, then resume hands-free listening
      if (voiceMode) {
        const last = messages[messages.length - 1];
        const lastText = last
          ? (typeof last.content === "string"
            ? last.content
            : last.content?.text || "")
          : "";
        if (lastText) {
          speakForVoice(lastText);
        } else {
          startVoiceLoop();
        }
      }
    }
  }

  function copyMessage(id: string, text: string) {
    navigator.clipboard.writeText(text).then(() => {
      copiedMessageId = id;
      setTimeout(() => {
        if (copiedMessageId === id) copiedMessageId = null;
      }, 2000);
    });
  }

  function startEditing(id: string, text: string) {
    if (sending || regenerating) return;
    editingMessage = { id, original: text };
    newMessage = text;
    textareaRef?.focus();
  }

  function cancelEditing() {
    editingMessage = null;
    newMessage = "";
    if (textareaRef) textareaRef.style.height = "auto";
  }

  const modelPresets = [
    { provider: "ollama", label: "Ollama (Local)", model: "llama3.1", badge: "Sovereign" },
    { provider: "openrouter", label: "OpenRouter", model: "anthropic/claude-3.5-sonnet", badge: null },
    { provider: "anthropic", label: "Anthropic", model: "claude-3-5-sonnet-20241022", badge: null },
    { provider: "openai", label: "OpenAI", model: "gpt-4o", badge: null },
  ];

  function openModelSwitcher() {
    switcherModel = bot?.config?.model_id || "";
    showModelSwitcher = !showModelSwitcher;
  }

  async function switchProvider(provider: string, model: string) {
    if (!bot?.config || sending) return;
    try {
      const updated = {
        ...bot,
        config: { ...bot.config, model_provider: provider, model_id: model },
      };
      await invoke("update_bot", { bot: updated });
      showModelSwitcher = false;
      onBotUpdated?.(updated);
    } catch (e) {
      console.error("Failed to switch provider:", e);
    }
  }

  async function applySwitcherModel() {
    const model = switcherModel.trim();
    if (!model || !bot?.config || sending) return;
    try {
      const updated = {
        ...bot,
        config: { ...bot.config, model_id: model },
      };
      await invoke("update_bot", { bot: updated });
      showModelSwitcher = false;
      onBotUpdated?.(updated);
    } catch (e) {
      console.error("Failed to apply model:", e);
    }
  }

  async function regenerate() {
    if (!selectedThreadId || sending || regenerating) return;
    regenerating = true;
    streamingText = "";
    streamingSources = [];
    try {
      await invoke("regenerate_message", { threadId: selectedThreadId });
      await loadMessages(selectedThreadId);
    } catch (e) {
      console.error("Failed to regenerate:", e);
      if (selectedThreadId) {
        await loadMessages(selectedThreadId);
      }
    } finally {
      regenerating = false;
      streamingText = "";
      scrollToBottom();
    }
  }

  function handleTextareaInput(e: Event) {
    const target = e.target as HTMLTextAreaElement;
    target.style.height = "auto";
    target.style.height = Math.min(target.scrollHeight, 160) + "px";
  }

  // ---- Inline image attachments (paste / drag-drop) ----

  const ACCEPTED_IMAGE_MIMES = ["image/png", "image/jpeg", "image/jpg", "image/gif", "image/webp"];
  const TEXT_FILE_EXTS = ["txt", "md", "rs", "ts", "tsx", "js", "jsx", "py", "json", "toml", "yaml", "yml", "css", "html", "sh", "sql", "go", "java", "c", "cpp", "h", "rb", "php", "swift", "kt"];

  function attachFiles(files: FileList | File[]) {
    for (const file of Array.from(files)) {
      // Images ride as inline attachments (vision)
      if (ACCEPTED_IMAGE_MIMES.includes(file.type)) {
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = String(reader.result || "");
          const base64 = dataUrl.split(",")[1] || "";
          if (base64) {
            pendingImages = [...pendingImages, {
              name: file.name || "pasted-image.png",
              mime: file.type || "image/png",
              data: base64,
            }];
          }
        };
        reader.readAsDataURL(file);
        continue;
      }

      // Text files insert into the composer (paperclip-style)
      const ext = (file.name?.split(".").pop() || "").toLowerCase();
      if (!TEXT_FILE_EXTS.includes(ext)) continue;
      const reader = new FileReader();
      reader.onload = () => {
        const text = String(reader.result || "");
        if (!text) return;
        newMessage = (newMessage ? newMessage + "\n\n" : "") +
          `Attached file [${file.name}]:\n\`\`\`${ext}\n${text}\n\`\`\`\n`;
      };
      reader.readAsText(file);
    }
  }

  function handleComposerPaste(e: ClipboardEvent) {
    if (e.clipboardData?.files?.length) {
      e.preventDefault();
      attachFiles(e.clipboardData.files);
    }
  }

  function handleComposerDrop(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer?.files?.length) {
      attachFiles(e.dataTransfer.files);
    }
  }

  function removePendingImage(idx: number) {
    pendingImages = pendingImages.filter((_, i) => i !== idx);
  }

  function getStatusTheme(status: string) {
    switch (status) {
      case "idle":
        return { dot: "bg-emerald-400", text: "text-emerald-400", label: "Ready" };
      case "thinking":
        return { dot: "bg-amber-400 animate-pulse", text: "text-amber-400", label: "Reasoning…" };
      case "running_tool":
        return { dot: "bg-sky-400 animate-pulse", text: "text-sky-400", label: "Running Tool…" };
      case "waiting_on_user":
        return { dot: "bg-rose-500", text: "text-rose-400", label: "Waiting on input" };
      case "paused":
        return { dot: "bg-purple-400", text: "text-purple-400", label: "Paused" };
      default:
        return { dot: "bg-emerald-400", text: "text-zinc-400", label: status || "Ready" };
    }
  }

  function getModelDisplayName(b: any) {
    if (!b?.config) return "claude-3.5-sonnet";
    const p = b.config.model_provider || "openrouter";
    const m = b.config.model_id || "claude-3-5-sonnet";
    return `${p}/${m.split("/").pop()}`;
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

  // ---- Hands-free Voice Mode (STT → send → TTS loop) ----

  let voiceRecognition: any = null;
  let voicePendingText = "";
  let voiceListening = false;

  function toggleVoiceMode() {
    if (voiceMode) {
      stopVoiceMode();
      return;
    }
    const SR = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    if (!SR) {
      alert("Voice mode needs the Web Speech API, which isn't available in this environment.");
      return;
    }
    voiceMode = true;
    startVoiceLoop();
  }

  function stopVoiceMode() {
    voiceMode = false;
    voicePendingText = "";
    voiceListening = false;
    try {
      voiceRecognition?.stop();
    } catch {}
    try {
      window.speechSynthesis?.cancel();
    } catch {}
    speaking = false;
  }

  function startVoiceLoop() {
    if (!voiceMode || voiceListening) return;
    const SR = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    if (!SR) {
      voiceMode = false;
      return;
    }
    try {
      voiceRecognition = new SR();
    } catch {
      voiceMode = false;
      return;
    }
    voiceRecognition.continuous = false;
    voiceRecognition.interimResults = true;
    voiceRecognition.lang = "en-US";
    voiceRecognition.onstart = () => {
      voiceListening = true;
    };
    voiceRecognition.onresult = (event: any) => {
      for (let i = event.resultIndex; i < event.results.length; ++i) {
        if (event.results[i].isFinal) {
          const transcript = event.results[i][0].transcript.trim();
          if (transcript) {
            voicePendingText = transcript;
          }
        }
      }
    };
    voiceRecognition.onend = () => {
      voiceListening = false;
      if (!voiceMode) return;
      // Wait out an in-flight send/regenerate before looping
      if (sending || regenerating) {
        setTimeout(startVoiceLoop, 500);
        return;
      }
      if (voicePendingText) {
        const text = voicePendingText;
        voicePendingText = "";
        sendMessage(text);
      } else {
        startVoiceLoop();
      }
    };
    voiceRecognition.onerror = () => {
      voiceListening = false;
      if (voiceMode) {
        setTimeout(startVoiceLoop, 800);
      }
    };
    try {
      voiceRecognition.start();
    } catch {
      voiceListening = false;
    }
  }

  function speakForVoice(md: string) {
    const synth = window.speechSynthesis;
    if (!synth) {
      if (voiceMode) startVoiceLoop();
      return;
    }
    try {
      synth.cancel();
    } catch {}
    const clean = stripForSpeech(md);
    if (!clean) {
      if (voiceMode) startVoiceLoop();
      return;
    }
    const utterance = new SpeechSynthesisUtterance(clean);
    utterance.rate = 1.05;
    utterance.onend = () => {
      speaking = false;
      if (voiceMode) startVoiceLoop();
    };
    utterance.onerror = () => {
      speaking = false;
      if (voiceMode) startVoiceLoop();
    };
    speaking = true;
    synth.speak(utterance);
  }

  function stripForSpeech(md: string): string {
    return md
      .replace(/([\s\S]*?)<\/think>/g, "")
      .replace(/```[\s\S]*?```/g, " code omitted. ")
      .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
      .replace(/\[([^\]]*)\]\(([^)]*)\)/g, "$1")
      .replace(/[#*_>`|~]+/g, " ")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 1200);
  }
</script>

<svelte:window onclick={() => (showThreadDropdown = false, showModelSwitcher = false)} />

{#snippet sourcesChips(sources: any[])}
  {#if sources && sources.length > 0}
    <div class="rounded-xl border border-white/10 bg-[#0b0b10] p-2.5 space-y-1.5">
      <div class="flex items-center gap-1.5 text-[10px] font-mono text-zinc-400 uppercase tracking-wider">
        <Globe class="size-3 text-sky-400" />
        <span>Sources ({sources.length})</span>
      </div>
      <div class="flex flex-wrap gap-1.5">
        {#each sources as src, idx}
          {@const label = domainOf(src.url)}
          <button
            type="button"
            class="max-w-[220px] h-6 px-2 rounded-lg bg-white/5 border border-white/10 hover:bg-sky-500/15 hover:border-sky-500/40 flex items-center gap-1.5 text-[10px] text-zinc-300 hover:text-sky-300 transition-colors cursor-pointer"
            title={src.title || src.url}
            onclick={() => openSource(src.url)}
          >
            <span class="size-3.5 rounded bg-sky-500/20 text-sky-400 font-mono flex items-center justify-center text-[8px] shrink-0">{idx + 1}</span>
            <span class="truncate font-mono">{label}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
{/snippet}

<div class="flex flex-col h-full overflow-hidden select-none bg-[#050507] text-zinc-100 font-sans relative">
  <!-- Sleek Top Header Bar (Grok Style) -->
  <header class="h-13 px-4 border-b border-[#1c1c24] bg-[#09090d]/90 backdrop-blur-md flex items-center justify-between z-20 shrink-0">
    <!-- Left: Bot Avatar & Info + Thread Switcher Dropdown -->
    <div class="flex items-center gap-3 min-w-0">
      <div class="relative size-8 rounded-xl overflow-hidden bg-[#14141c] border border-white/10 p-0.5 shrink-0 shadow-sm">
        <img
          src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "avataaars")}
          alt={bot.name}
          class="size-full rounded-lg object-cover"
        />
        <span class="absolute bottom-0 right-0 size-2 rounded-full ring-1 ring-black {botStatusTheme.dot}"></span>
      </div>

      <div class="flex items-center gap-2 min-w-0">
        <span class="font-bold text-sm text-white truncate">{bot.name}</span>

        <!-- Model Quick Switcher (clickable pill, Grok-style) -->
        <div class="relative">
          <button
            type="button"
            class="text-[10px] font-mono py-0.5 px-2 rounded-md bg-white/5 border border-white/10 text-zinc-400 hover:text-white hover:border-zinc-500 truncate cursor-pointer transition-colors hidden sm:inline-flex items-center gap-1 max-w-[220px]"
            onclick={(e) => {
              e.stopPropagation();
              openModelSwitcher();
            }}
            title="Switch model (per-conversation)"
          >
            <span class="truncate">{getModelDisplayName(bot)}</span>
            <ChevronDown class="size-2.5 shrink-0" />
          </button>

          {#if showModelSwitcher}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="absolute left-0 top-7 z-50 w-72 bg-[#0e0e14] border border-[#262633] rounded-2xl shadow-2xl p-2 space-y-1 animate-in fade-in zoom-in-95 backdrop-blur-xl"
              onclick={(e) => e.stopPropagation()}
            >
              <span class="block px-2 py-1 text-[10px] font-bold text-zinc-400 uppercase tracking-wider font-mono border-b border-white/10">
                Switch Model
              </span>

              {#each modelPresets as preset}
                {@const isActive = bot?.config?.model_provider === preset.provider}
                <button
                  type="button"
                  class="w-full text-left px-2.5 py-1.5 rounded-xl text-xs flex items-center justify-between transition-colors cursor-pointer {isActive ? 'bg-white/10 text-white border border-white/15' : 'text-zinc-400 hover:text-white hover:bg-white/5'}"
                  onclick={() => switchProvider(preset.provider, preset.model)}
                  title={`Switch to ${preset.model}`}
                >
                  <span class="font-medium">{preset.label}</span>
                  <span class="text-[9px] font-mono text-zinc-500 truncate ml-2">{preset.model}</span>
                </button>
              {/each}

              <!-- Custom model id -->
              <div class="flex items-center gap-1.5 pt-1 border-t border-white/10 mt-1">
                <input
                  bind:value={switcherModel}
                  placeholder="model id…"
                  class="flex-1 min-w-0 h-6 px-2 rounded-lg bg-[#07070a] border border-white/10 text-[10px] font-mono text-white placeholder:text-zinc-500 focus:outline-none focus:border-sky-500/50"
                  onkeydown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      e.stopPropagation();
                      applySwitcherModel();
                    }
                  }}
                />
                <Button
                  size="sm"
                  class="h-6 px-2 text-[10px] bg-white text-black hover:bg-zinc-200 cursor-pointer shrink-0"
                  onclick={applySwitcherModel}
                >
                  Apply
                </Button>
              </div>
            </div>
          {/if}
        </div>

        {#if currentThread?.ephemeral}
          <span class="text-[10px] font-mono py-0.5 px-2 rounded-md bg-amber-500/15 border border-amber-500/30 text-amber-300 flex items-center gap-1 shrink-0" title="Temporary chat — not feeding agent memory">
            <Ghost class="size-3" />
            <span>Temporary</span>
          </span>
        {/if}
      </div>

      <!-- Thread Switcher Dropdown -->
      <div class="relative ml-2">
        <button
          type="button"
          class="h-7 px-2.5 rounded-lg border border-white/10 bg-white/5 hover:bg-white/10 text-xs text-zinc-300 flex items-center gap-1.5 cursor-pointer font-medium transition-colors"
          onclick={(e) => {
            e.stopPropagation();
            showThreadDropdown = !showThreadDropdown;
          }}
          title="Switch Thread"
        >
          <MessageSquare class="size-3 text-sky-400" />
          <span class="max-w-[130px] truncate text-[11px] font-mono">
            {currentThread?.title || (threads.length > 0 ? "Threads (" + threads.length + ")" : "New Thread")}
          </span>
          <ChevronDown class="size-3 text-zinc-400" />
        </button>

        {#if showThreadDropdown}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute left-0 top-9 z-50 w-64 bg-[#0e0e14] border border-[#262633] rounded-2xl shadow-2xl p-2 space-y-1 animate-in fade-in zoom-in-95 backdrop-blur-xl"
            onclick={(e) => e.stopPropagation()}
          >
            <div class="flex items-center justify-between px-2 py-1 border-b border-white/10">
              <span class="text-[10px] font-bold text-zinc-400 uppercase tracking-wider font-mono">Chat History</span>
              <button
                type="button"
                class="text-[10px] text-sky-400 hover:text-sky-300 flex items-center gap-1 cursor-pointer"
                onclick={createNewThread}
              >
                <Plus class="size-3" /> New
              </button>
            </div>

            <div class="max-h-60 overflow-y-auto space-y-0.5 py-1">
              {#each threads as thread (thread.id)}
                {@const isSelected = selectedThreadId === thread.id}
                <button
                  type="button"
                  class="w-full text-left px-2.5 py-1.5 rounded-xl text-xs truncate transition-colors cursor-pointer flex items-center justify-between {isSelected
                    ? 'bg-white/10 text-white font-medium border border-white/15'
                    : 'text-zinc-400 hover:text-white hover:bg-white/5'}"
                  onclick={() => loadMessages(thread.id)}
                >
                  <span class="truncate">{thread.title || "Untitled"}</span>
                  {#if isSelected}
                    <span class="size-1.5 rounded-full bg-sky-400 shrink-0 ml-2"></span>
                  {/if}
                </button>
              {:else}
                <div class="p-3 text-center text-xs text-zinc-500">No previous threads</div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Quick New Thread Button -->
      <button
        type="button"
        class="h-7 px-2 rounded-lg border border-white/10 bg-white/5 hover:bg-white/10 text-xs text-zinc-300 flex items-center gap-1 cursor-pointer transition-colors"
        onclick={createNewThread}
        title="Start fresh conversation"
      >
        <Plus class="size-3.5" />
        <span class="hidden md:inline text-[11px]">New</span>
      </button>
    </div>

    <!-- Right Header Controls -->
    <div class="flex items-center gap-2">
      <!-- Session Telemetry Pill -->
      <button
        type="button"
        class="h-7 px-2.5 rounded-lg border border-white/10 bg-white/5 text-xs font-mono flex items-center gap-1.5 text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={() => (showCostInfo = !showCostInfo)}
        title="Session Telemetry & Tokens"
      >
        <DollarSign class="size-3 text-emerald-400" />
        <span>${sessionCost.toFixed(4)}</span>
      </button>

      <!-- Agent Intelligence / Skills Button -->
      <button
        type="button"
        class="h-7 px-2.5 rounded-lg border border-white/10 bg-white/5 text-xs flex items-center gap-1.5 text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={() => (showIntelligence = true)}
        title="Agent Intelligence & Memory"
      >
        <Brain class="size-3 text-sky-400" />
        <span class="hidden md:inline text-[11px]">{bot.name.split(" ")[0]} Intelligence</span>
      </button>

      <!-- Thread Drawer Toggle Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer {showThreadDrawer ? 'bg-sky-500/20 text-sky-300 border-sky-500/40' : ''}"
        onclick={() => (showThreadDrawer = !showThreadDrawer)}
        title="Toggle Thread History Sidebar"
      >
        <History class="size-3.5" />
      </button>

      <!-- Routines / Scheduler Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer {showRoutines ? 'bg-sky-500/20 text-sky-300 border-sky-500/40' : ''}"
        onclick={() => (showRoutines = true)}
        title="Scheduled Routines (cron)"
      >
        <Clock class="size-3.5" />
      </button>

      <!-- Fleet Sync / Backup Button -->
      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer {showSync ? 'bg-sky-500/20 text-sky-300 border-sky-500/40' : ''}"
        onclick={() => (showSync = true)}
        title="Fleet Sync & Backup (signed bundles)"
      >
        <Boxes class="size-3.5" />
      </button>

      <!-- Settings Shortcut -->
      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={triggerOpenSettings}
        title="Configure Model & API Keys (⌘,)"
      >
        <Settings class="size-3.5" />
      </button>

      <!-- Pause / Play Agent -->
      <button
        type="button"
        class="size-7 rounded-lg border border-white/10 bg-white/5 flex items-center justify-center text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors cursor-pointer"
        onclick={togglePause}
        title={bot.status === "paused" ? "Resume agent" : "Pause all agents (kill switch)"}
      >
        {#if bot.status === "paused"}
          <Play class="size-3.5 fill-current text-purple-400" />
        {:else}
          <Pause class="size-3.5" />
        {/if}
      </button>
    </div>
  </header>

  <!-- Telemetry Strip Banner -->
  {#if showCostInfo}
    <div class="px-4 py-2 bg-[#09090e] border-b border-[#1c1c24] flex items-center justify-between text-xs text-zinc-400 font-mono">
      <div class="flex items-center gap-6">
        <span>Tokens: <strong class="text-white">{sessionTokens.toLocaleString()}</strong></span>
        <span>Cost: <strong class="text-emerald-400">${sessionCost.toFixed(4)}</strong></span>
        <span>Model: <strong class="text-sky-300">{getModelDisplayName(bot)}</strong></span>
      </div>
      <span class="text-[10px] px-2 py-0.5 rounded bg-sky-950/60 border border-sky-800/40 text-sky-300 font-mono">
        LOCAL HARDWARE ENCLAVE
      </span>
    </div>
  {/if}

  <!-- Main Chat Body & Optional Slide-out Thread Drawer -->
  <div class="flex flex-1 overflow-hidden relative">
    <!-- Optional Slide-out Thread History Drawer -->
    {#if showThreadDrawer}
      <div class="w-60 border-r border-[#1c1c24] bg-[#07070a] flex flex-col overflow-hidden shrink-0 z-10 animate-in slide-in-from-left duration-200">
        <div class="p-3 border-b border-[#1c1c24] flex items-center justify-between">
          <span class="text-[11px] font-bold text-zinc-400 uppercase tracking-wider font-mono flex items-center gap-1.5">
            <History class="size-3.5 text-sky-400" />
            Thread History
          </span>
          <button
            type="button"
            class="size-6 rounded-md bg-white/5 border border-white/10 text-zinc-400 hover:text-white flex items-center justify-center cursor-pointer"
            onclick={createNewThread}
            title="New thread"
          >
            <Plus class="size-3.5" />
          </button>
        </div>

        <!-- Cross-thread search -->
        <div class="p-2 border-b border-[#1c1c24]">
          <div class="relative">
            <Search class="absolute left-2 top-1/2 -translate-y-1/2 size-3 text-zinc-500 pointer-events-none" />
            <input
              bind:value={searchQuery}
              onkeydown={(e) => {
                if (e.key === "Enter") { e.preventDefault(); runSearch(); }
              }}
              placeholder="Search all threads… (⏎)"
              class="w-full h-7 pl-7 pr-2 rounded-lg bg-[#0e0e14] border border-white/10 text-[10px] text-white placeholder:text-zinc-500 focus:outline-none focus:border-sky-500/50"
            />
          </div>
          {#if searchResults.length > 0}
            <div class="mt-1.5 space-y-1 max-h-48 overflow-y-auto">
              {#each searchResults as hit (hit.message_id)}
                <button
                  type="button"
                  class="w-full text-left px-2 py-1.5 rounded-lg bg-white/5 border border-white/10 hover:border-sky-500/40 transition-colors cursor-pointer"
                  onclick={() => loadMessages(hit.thread_id)}
                >
                  <div class="flex items-center gap-1 text-[9px] font-mono text-zinc-500">
                    <span class="text-sky-400 truncate max-w-[100px]">{hit.thread_title || "Thread"}</span>
                    <span class="shrink-0">· {hit.role}</span>
                  </div>
                  <p class="text-[10px] text-zinc-300 leading-snug line-clamp-2 mt-0.5">{hit.snippet}</p>
                </button>
              {/each}
            </div>
          {:else if searchPerformed}
            <div class="mt-1.5 text-[10px] text-zinc-500 text-center">No matches found</div>
          {/if}
        </div>

        <div class="flex-1 overflow-y-auto p-2 space-y-1">
          {#each threads as thread (thread.id)}
            {@const isSelected = selectedThreadId === thread.id}
            <button
              type="button"
              class="w-full text-left px-3 py-2 rounded-xl text-xs truncate transition-all block focus:outline-none cursor-pointer {isSelected
                ? 'bg-white/10 border border-white/20 text-white font-medium shadow-sm'
                : 'text-zinc-400 hover:text-white hover:bg-white/5'}"
              onclick={() => loadMessages(thread.id)}
            >
              {thread.title || "Untitled thread"}
            </button>
          {:else}
            <div class="p-4 text-center text-xs text-zinc-500">No threads yet</div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Chat Messages Stream (+ optional Artifact split view) -->
    <div class="flex flex-1 overflow-hidden bg-[#000000]">
      <div class="flex flex-col overflow-hidden {openArtifact ? 'w-[54%] shrink-0' : 'flex-1'}">
      <div bind:this={messagesContainer} class="flex-1 overflow-y-auto p-4 sm:p-6 space-y-6">
        <div class="max-w-3xl mx-auto space-y-6">
          {#each messages as message (message.id || message.created_at)}
            {@const isUser = message.role === "user"}
            {@const isModelError = typeof message.content === "string" && message.content.includes("⚠️ **Model Error:**")}
            {@const rawContent = typeof message.content === "string" ? message.content : message.content?.text || JSON.stringify(message.content)}
            {@const hasChecklist = message.content?.type === "checklist" || (typeof message.content === "object" && message.content?.items)}
            {@const messageSources = Array.isArray(message.content?.sources) ? message.content.sources : []}
            {@const messageImages = Array.isArray(message.attachments) ? message.attachments.filter((a: any) => a?.is_image && a?.data) : []}

            <div class="flex gap-3.5 {isUser ? 'justify-end' : 'justify-start'} group">
              {#if !isUser}
                <!-- Bot Avatar -->
                <div class="size-8 rounded-xl overflow-hidden bg-[#121218] border border-white/10 shrink-0 mt-1 shadow-sm">
                  <img
                    src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "avataaars")}
                    alt={bot.name}
                    class="size-full object-cover"
                  />
                </div>
              {/if}

              <div class="max-w-[85%] sm:max-w-[78%] space-y-1.5">
                {#if isModelError}
                  <!-- Model Configuration Required Card -->
                  <div class="rounded-2xl p-4 bg-red-950/30 border border-red-800/40 text-zinc-200 space-y-3 shadow-xl">
                    <div class="flex items-center gap-2 text-red-400 font-bold text-xs font-mono">
                      <AlertTriangle class="size-4 shrink-0" />
                      <span>Model Configuration Required</span>
                    </div>

                    <p class="text-xs text-zinc-300 leading-relaxed font-sans">
                      {rawContent.replace("⚠️ **Model Error:** ", "")}
                    </p>

                    <div class="pt-1 flex items-center gap-2">
                      <Button
                        size="sm"
                        class="h-8 gap-1.5 text-xs bg-white text-black hover:bg-zinc-200 font-medium shadow cursor-pointer"
                        onclick={triggerOpenSettings}
                      >
                        <Key class="size-3.5" />
                        Configure API Key in Settings (⌘,)
                      </Button>
                    </div>
                  </div>
                {:else if isUser}
                  <!-- Grok User Message Bubble -->
                  <div class="rounded-2xl px-4 py-3 text-xs leading-relaxed text-zinc-100 bg-[#1e1e24] border border-[#2f2f38] shadow-md selection:bg-sky-500/30">
                    <p class="whitespace-pre-wrap font-sans text-xs leading-relaxed">{rawContent}</p>
                    {#if messageImages.length}
                      <div class="flex flex-wrap gap-1.5 pt-1.5">
                        {#each messageImages as att}
                          <img
                            src={`data:${att.mime_type};base64,${att.data}`}
                            alt={att.name || "attached image"}
                            class="max-h-40 rounded-lg border border-white/10 object-contain bg-[#101016]"
                          />
                        {/each}
                      </div>
                    {/if}
                  </div>
                {:else}
                  <!-- Grok Assistant Message (Clean Markdown + Expandable Thought Block) -->
                  <div class="space-y-3">
                    {#if hasChecklist}
                      <!-- Signature Grok "Thought for X steps" Collapsible Accordion -->
                      <details class="group rounded-xl border border-white/10 bg-[#0b0b10] overflow-hidden" open>
                        <summary class="flex items-center justify-between px-3 py-2 text-[11px] font-mono text-zinc-400 cursor-pointer hover:text-zinc-200 hover:bg-white/5 transition-colors">
                          <div class="flex items-center gap-2">
                            <Brain class="size-3.5 text-sky-400" />
                            <span>Reasoning & Task Execution ({message.content.items.length} steps)</span>
                          </div>
                          <ChevronDown class="size-3.5 group-open:rotate-180 transition-transform" />
                        </summary>

                        <div class="p-3 border-t border-white/10 space-y-1.5 bg-[#07070a]">
                          {#each message.content.items as item}
                            <div class="flex items-center gap-2.5 text-xs bg-[#101016] p-2.5 rounded-xl border border-white/5">
                              {#if item.status === "completed"}
                                <CheckCircle2 class="size-4 text-emerald-400 shrink-0" />
                              {:else if item.status === "failed"}
                                <XCircle class="size-4 text-rose-400 shrink-0" />
                              {:else if item.status === "in_progress"}
                                <Loader2 class="size-4 text-sky-400 animate-spin shrink-0" />
                              {:else}
                                <Circle class="size-4 text-zinc-600 shrink-0" />
                              {/if}
                              <span class="font-medium text-zinc-200">{item.label}</span>
                              {#if item.result}
                                <span class="text-zinc-400 ml-auto text-[11px] font-mono">{item.result}</span>
                              {/if}
                            </div>
                          {/each}
                        </div>
                      </details>
                    {/if}

                    <!-- Rich Markdown Formatted Text Output -->
                    <div class="text-zinc-200 selection:bg-sky-500/30">
                      <MarkdownRenderer
                        content={hasChecklist ? (message.content.text || "") : rawContent}
                        onOpenArtifact={(a) => (openArtifact = a)}
                      />
                    </div>

                    <!-- Persisted Web Sources / Citations -->
                    {@render sourcesChips(messageSources)}
                  </div>
                {/if}

                <!-- Message Action Strip (Copy, Edit, Timestamp, Hover Actions) -->
                <div class="flex items-center gap-3 text-[10px] text-zinc-500 px-1 {isUser ? 'justify-end' : 'justify-start'}">
                  <span>{formatTime(message.created_at)}</span>

                  {#if isUser && !sending && !regenerating}
                    <button
                      type="button"
                      class="opacity-0 group-hover:opacity-100 transition-opacity text-zinc-400 hover:text-white flex items-center gap-1 cursor-pointer"
                      onclick={() => startEditing(message.id || message.created_at, rawContent)}
                      title="Edit and resend (removes the response after this message)"
                    >
                      <Pencil class="size-3" />
                      <span>Edit</span>
                    </button>
                  {/if}

                  {#if !isModelError}
                    <button
                      type="button"
                      class="opacity-0 group-hover:opacity-100 transition-opacity text-zinc-400 hover:text-white flex items-center gap-1 cursor-pointer"
                      onclick={() => copyMessage(message.id || message.created_at, rawContent)}
                      title="Copy full message"
                    >
                      {#if copiedMessageId === (message.id || message.created_at)}
                        <Check class="size-3 text-emerald-400" />
                        <span class="text-emerald-400 font-mono">Copied</span>
                      {:else}
                        <Copy class="size-3" />
                        <span>Copy</span>
                      {/if}
                    </button>
                  {/if}

                  {#if !isModelError && !isUser && (messages[messages.length - 1]?.id === message.id) && !sending && !regenerating}
                    <button
                      type="button"
                      class="opacity-0 group-hover:opacity-100 transition-opacity text-zinc-400 hover:text-white flex items-center gap-1 cursor-pointer"
                      onclick={regenerate}
                      title="Regenerate response"
                    >
                      <RotateCcw class="size-3" />
                      <span>Regenerate</span>
                    </button>
                  {/if}
                </div>
              </div>

              {#if isUser}
                <!-- User Avatar -->
                <div class="size-8 rounded-full overflow-hidden bg-[#181820] border border-white/15 shrink-0 mt-1 shadow-sm">
                  <img src={userAvatar || getDiceBearUrl("You", "micah")} alt="You" class="size-full object-cover" />
                </div>
              {/if}
            </div>
          {:else}
            <!-- Empty Thread State (Grok Style) -->
            <div class="my-10 text-center space-y-6 max-w-xl mx-auto">
              <!-- Bot Identity Emblem -->
              <div class="relative inline-block">
                <div class="size-16 rounded-2xl overflow-hidden bg-[#121218] border border-white/15 mx-auto shadow-2xl p-0.5">
                  <img
                    src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "avataaars")}
                    alt={bot.name}
                    class="size-full rounded-xl object-cover"
                  />
                </div>
                <span class="absolute -bottom-1 -right-1 size-3.5 rounded-full ring-2 ring-black {botStatusTheme.dot}"></span>
              </div>

              <div class="space-y-1.5">
                <h3 class="font-black text-xl text-white tracking-tight">
                  What would you like to explore?
                </h3>
                <p class="text-xs text-zinc-400 max-w-md mx-auto leading-relaxed">
                  {bot.description || "Sovereign desktop agent ready to execute autonomous tasks, run code, or synthesize research."}
                </p>
              </div>

              <!-- Grok Prompt Suggestion Grid -->
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 text-left pt-2">
                {#each samplePrompts as p}
                  {@const Icon = p.icon}
                  <button
                    type="button"
                    class="p-3.5 rounded-2xl border border-zinc-800/80 bg-[#0a0a0f] hover:bg-[#121218] hover:border-zinc-600 transition-all cursor-pointer group flex flex-col justify-between"
                    onclick={() => sendMessage(p.desc)}
                  >
                    <div class="flex items-center justify-between mb-1.5">
                      <span class="font-bold text-xs text-white group-hover:text-sky-300 transition-colors">{p.title}</span>
                      <Icon class="size-3.5 text-zinc-500 group-hover:text-sky-400 transition-colors" />
                    </div>
                    <p class="text-[11px] text-zinc-400 leading-normal line-clamp-2">{p.desc}</p>
                  </button>
                {/each}
              </div>
            </div>
          {/each}

          <!-- Live Streaming Assistant Bubble -->
          {#if (sending || regenerating) && selectedThreadId}
            <div class="flex gap-3.5 justify-start">
              <div class="size-8 rounded-xl overflow-hidden bg-[#121218] border border-white/10 shrink-0 mt-1 shadow-sm">
                <img
                  src={bot.avatar_url || getDiceBearUrl(bot.name, bot.avatar_style || "avataaars")}
                  alt={bot.name}
                  class="size-full object-cover"
                />
              </div>

              <div class="max-w-[85%] sm:max-w-[78%] space-y-1.5">
                {#if streamingText}
                  <div class="rounded-2xl px-4 py-3 bg-[#1e1e24] border border-[#2f2f38] shadow-md selection:bg-sky-500/30">
                    <div class="text-zinc-200">
                      <MarkdownRenderer content={streamingText} />
                    </div>
                    <span class="inline-block w-1.5 h-3.5 bg-sky-400 animate-pulse ml-0.5 align-middle rounded-sm"></span>
                  </div>
                {:else}
                  <!-- Skeleton loading lines (GROK-style shimmer, pre-first-token) -->
                  <div class="rounded-2xl px-4 py-3.5 bg-[#1e1e24] border border-[#2f2f38] shadow-md w-fit min-w-[280px]">
                    <div class="space-y-2.5">
                      <div class="shimmer h-3 rounded-full w-[85%]"></div>
                      <div class="shimmer h-3 rounded-full w-[70%] [animation-delay:120ms]"></div>
                      <div class="shimmer h-3 rounded-full w-[45%] [animation-delay:240ms]"></div>
                    </div>
                  </div>
                {/if}

                {#if streamingTool}
                  <!-- Tool execution skeleton row -->
                  <div class="rounded-xl border border-white/10 bg-[#0b0b10] p-2.5 flex items-center gap-2.5 w-fit">
                    <Loader2 class="size-3.5 text-sky-400 animate-spin shrink-0" />
                    <div class="space-y-1.5">
                      <div class="shimmer h-2.5 rounded-full w-40"></div>
                      <div class="shimmer h-2.5 rounded-full w-28 [animation-delay:120ms]"></div>
                    </div>
                  </div>
                {/if}

                <div class="flex items-center gap-2 text-[10px] text-zinc-500 px-1 font-mono">
                  {#if streamingTool}
                    <Loader2 class="size-3 animate-spin text-sky-400" />
                    <span class="text-sky-400">Running tool: {streamingTool}</span>
                  {:else if streamingText}
                    <span class="text-sky-400">Streaming…</span>
                  {:else}
                    <span>{bot.name} is thinking…</span>
                  {/if}
                </div>

                <!-- Live source chips during streaming -->
                {@render sourcesChips(streamingSources)}
              </div>
            </div>
          {/if}
        </div>
      </div>

      <!-- Grok Floating Capsule Composer -->
      <div class="p-4 bg-gradient-to-t from-black via-black/90 to-transparent shrink-0">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="max-w-3xl mx-auto rounded-2xl border border-zinc-800 bg-[#0d0d12]/95 backdrop-blur-xl p-3 shadow-2xl focus-within:border-zinc-600 transition-all"
          ondragover={(e) => e.preventDefault()}
          ondrop={handleComposerDrop}
        >
      <!-- Pending inline image attachments -->
      {#if pendingImages.length}
        <div class="flex flex-wrap gap-1.5 pb-1.5">
          {#each pendingImages as img, idx}
            <div class="relative size-14 rounded-lg overflow-hidden border border-white/15 bg-[#101016] shadow-sm">
              <img
                src={`data:${img.mime};base64,${img.data}`}
                alt={img.name}
                class="size-full object-cover"
              />
              <button
                type="button"
                class="absolute top-0.5 right-0.5 size-4 rounded-full bg-black/70 text-white text-[9px] flex items-center justify-center cursor-pointer hover:bg-rose-500/80 transition-colors"
                onclick={() => removePendingImage(idx)}
                title="Remove attachment"
              >
                ✕
              </button>
            </div>
          {/each}
        </div>
      {/if}

      <textarea
        bind:this={textareaRef}
        bind:value={newMessage}
        oninput={handleTextareaInput}
        onpaste={handleComposerPaste}
        placeholder="Ask anything, run code, or attach images to {bot.name}..."
        rows={1}
        class="w-full bg-transparent text-xs sm:text-sm text-white placeholder:text-zinc-500 resize-none focus:outline-none min-h-[44px] max-h-40 leading-relaxed font-sans"
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            sendMessage();
          } else if (e.key === "Escape" && editingMessage) {
            e.preventDefault();
            cancelEditing();
          }
        }}
      ></textarea>

      <!-- Edit-and-resend banner -->
      {#if editingMessage}
        <div class="flex items-center justify-between pt-2 border-t border-amber-500/20 mt-1">
          <div class="flex items-center gap-1.5 text-[10px] font-mono text-amber-300">
            <Pencil class="size-3" />
            <span>Editing message — Enter resends; responses after it are removed. Esc to cancel.</span>
          </div>
          <button
            type="button"
            class="text-[10px] font-mono text-zinc-400 hover:text-white cursor-pointer"
            onclick={cancelEditing}
            title="Cancel edit"
          >
            Cancel
          </button>
        </div>
      {/if}

          <!-- Action Toolbar Inside Capsule -->
          <div class="flex items-center justify-between pt-2 border-t border-white/5 mt-1">
            <!-- Left Tool Toggles -->
            <div class="flex items-center gap-1.5">
              <button
                type="button"
                class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {deepSearchActive
                  ? 'bg-sky-500/20 text-sky-300 border-sky-500/50 shadow-[0_0_10px_rgba(56,189,248,0.2)]'
                  : 'border-white/10 text-zinc-400 hover:text-zinc-200 hover:bg-white/5'}"
                onclick={() => (deepSearchActive = !deepSearchActive)}
                title="Toggle DeepSearch Web Intelligence"
              >
                <Globe class="size-3" />
                <span>DeepSearch</span>
              </button>

              <button
                type="button"
                class="h-7 px-2.5 rounded-lg border text-[11px] font-mono flex items-center gap-1.5 transition-all cursor-pointer {thinkActive
                  ? 'bg-indigo-500/20 text-indigo-300 border-indigo-500/50 shadow-[0_0_10px_rgba(99,102,241,0.2)]'
                  : 'border-white/10 text-zinc-400 hover:text-zinc-200 hover:bg-white/5'}"
                onclick={() => (thinkActive = !thinkActive)}
                title="Toggle Deep Reasoning Mode"
              >
                <Brain class="size-3" />
                <span>Think</span>
              </button>

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

              <button
                type="button"
                class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {voiceMode ? 'text-emerald-400 bg-emerald-500/20 border border-emerald-500/50 shadow-sm' : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5'} {speaking ? 'animate-pulse' : ''}"
                onclick={toggleVoiceMode}
                title={voiceMode ? "Voice mode on — click to stop (hands-free loop)" : "Voice mode: hands-free talk → response spoken aloud"}
              >
                <Volume2 class="size-3.5" />
              </button>

              <button
                type="button"
                class="size-7 rounded-lg flex items-center justify-center transition-all cursor-pointer {tempActive ? 'text-amber-400 bg-amber-500/15 border border-amber-500/40' : 'text-zinc-400 hover:text-zinc-200 hover:bg-white/5'}"
                onclick={() => (tempActive = !tempActive)}
                title={tempActive ? "Temporary chat ON — new threads won't feed agent memory" : "Temporary chat: conversations won't feed agent memory"}
              >
                <Ghost class="size-3.5" />
              </button>
            </div>

            <!-- Right Controls: Model Pill & High-Contrast Send Button -->
            <div class="flex items-center gap-2">
              <span class="text-[10px] font-mono text-zinc-500 px-2 py-0.5 rounded border border-white/5 bg-white/[0.02] hidden sm:inline">
                {getModelDisplayName(bot)}
              </span>

              <button
                type="button"
                onclick={() => sendMessage()}
                disabled={!newMessage.trim() || sending}
                class="size-8 rounded-full flex items-center justify-center transition-all cursor-pointer {newMessage.trim() && !sending
                  ? 'bg-white text-black hover:bg-zinc-200 shadow-md scale-105'
                  : 'bg-zinc-800 text-zinc-500 cursor-not-allowed opacity-50'}"
                title="Send message (Enter)"
              >
                {#if sending}
                  <Loader2 class="size-3.5 animate-spin" />
                {:else}
                  <ArrowUp class="size-4 stroke-[2.5]" />
                {/if}
              </button>
            </div>
          </div>
        </div>

        <div class="text-center mt-2">
          <span class="text-[10px] text-zinc-500 font-mono">
            RAVENBOT local enclave active • ⌘K for command palette • ⌘, for settings
          </span>
        </div>
      </div>
      </div>

      <!-- Artifact / Canvas Split Panel -->
      {#if openArtifact}
        <div class="w-[46%] border-l border-[#1c1c24] shrink-0">
          <ArtifactPanel artifact={openArtifact} onClose={() => (openArtifact = null)} />
        </div>
      {/if}
    </div>
  </div>
</div>

<!-- Intelligence Modal -->
{#if showIntelligence}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/70 backdrop-blur-md flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (showIntelligence = false)}
  >
    <div
      class="w-full max-w-lg bg-[#0e0e14] border border-[#262633] rounded-3xl p-6 shadow-2xl relative space-y-4"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-center justify-between border-b border-white/10 pb-3">
        <div class="flex items-center gap-2">
          <Brain class="size-4 text-sky-400" />
          <span class="font-bold text-sm text-white">{bot.name} Intelligence</span>
        </div>
        <button
          type="button"
          class="size-6 rounded-md hover:bg-white/10 text-zinc-400 hover:text-white flex items-center justify-center cursor-pointer text-xs font-mono"
          onclick={() => (showIntelligence = false)}
        >
          ✕
        </button>
      </div>
      <AgentIntelligence
        botId={bot.id}
        botName={bot.name}
      />
    </div>
  </div>
{/if}

<!-- Fleet Sync / Backup Modal -->
{#if showSync}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/70 backdrop-blur-md flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (showSync = false)}
  >
    <div
      class="w-full max-w-lg bg-[#0e0e14] border border-[#262633] rounded-3xl p-6 shadow-2xl relative space-y-4"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-center justify-between border-b border-white/10 pb-3">
        <div class="flex items-center gap-2">
          <Boxes class="size-4 text-sky-400" />
          <span class="font-bold text-sm text-white">Fleet Sync & Backup</span>
        </div>
        <button
          type="button"
          class="size-6 rounded-md hover:bg-white/10 text-zinc-400 hover:text-white flex items-center justify-center cursor-pointer text-xs font-mono"
          onclick={() => (showSync = false)}
        >
          ✕
        </button>
      </div>
      <SyncPanel bot={bot} onBotImported={() => onBotUpdated?.(bot)} />
    </div>
  </div>
{/if}

<!-- Routines / Scheduler Modal -->
{#if showRoutines}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/70 backdrop-blur-md flex items-center justify-center p-4 animate-in fade-in"
    onclick={() => (showRoutines = false)}
  >
    <div
      class="w-full max-w-lg bg-[#0e0e14] border border-[#262633] rounded-3xl p-6 shadow-2xl relative space-y-4"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-center justify-between border-b border-white/10 pb-3">
        <div class="flex items-center gap-2">
          <Clock class="size-4 text-sky-400" />
          <span class="font-bold text-sm text-white">{bot.name} Routines</span>
        </div>
        <button
          type="button"
          class="size-6 rounded-md hover:bg-white/10 text-zinc-400 hover:text-white flex items-center justify-center cursor-pointer text-xs font-mono"
          onclick={() => (showRoutines = false)}
        >
          ✕
        </button>
      </div>
      <RoutinesPanel bot={bot} />
    </div>
  </div>
{/if}

<style>
  .shimmer {
    background: linear-gradient(
      90deg,
      rgba(255, 255, 255, 0.06) 25%,
      rgba(255, 255, 255, 0.16) 50%,
      rgba(255, 255, 255, 0.06) 75%
    );
    background-size: 200% 100%;
    animation: shimmer-slide 1.4s ease-in-out infinite;
  }
  @keyframes shimmer-slide {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }
</style>
