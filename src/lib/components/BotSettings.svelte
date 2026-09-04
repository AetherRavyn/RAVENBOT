<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Tabs from "$lib/components/ui/tabs";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl } from "$lib/utils";
  import {
    Bot,
    Cpu,
    Sliders,
    FileCode,
    Trash2,
    Save,
    Crown,
    Server,
    AlertTriangle,
    Check,
    Palette,
  } from "@lucide/svelte";

  interface Props {
    bot: any;
    open: boolean;
    onClose: () => void;
    onUpdated: (bot: any) => void;
  }

  let { bot, open, onClose, onUpdated }: Props = $props();

  let name = $state("");
  let description = $state("");
  let avatarUrl = $state<string | null>(null);
  let avatarStyle = $state("bottts");
  let showAvatarPicker = $state(false);

  let modelProvider = $state("openrouter");
  let modelId = $state("anthropic/claude-3-5-sonnet");
  let temperature = $state(0.7);
  let maxTokens = $state(4096);
  let customPrompt = $state("");
  let isOrchestrator = $state(false);
  let activeTab = $state("model");
  let showDeleteConfirm = $state(false);
  let isSaving = $state(false);

  const providers = [
    {
      id: "openrouter",
      name: "OpenRouter (Cloud)",
      description: "Claude 3.5, GPT-4o, Llama 3.1, Gemini",
      models: [
        { id: "anthropic/claude-3-5-sonnet", name: "Claude 3.5 Sonnet" },
        { id: "anthropic/claude-3-haiku", name: "Claude 3 Haiku" },
        { id: "openai/gpt-4o", name: "GPT-4o" },
        { id: "openai/gpt-4o-mini", name: "GPT-4o Mini" },
        { id: "google/gemini-pro-1.5", name: "Gemini Pro 1.5" },
        { id: "meta-llama/llama-3.1-70b-instruct", name: "Llama 3.1 70B" },
      ],
    },
    {
      id: "anthropic",
      name: "Anthropic Direct",
      description: "Direct Claude API access",
      models: [
        { id: "claude-3-5-sonnet-20241022", name: "Claude 3.5 Sonnet" },
        { id: "claude-3-haiku-20240307", name: "Claude 3 Haiku" },
      ],
    },
    {
      id: "openai",
      name: "OpenAI Direct",
      description: "Direct GPT-4o API access",
      models: [
        { id: "gpt-4o", name: "GPT-4o" },
        { id: "gpt-4o-mini", name: "GPT-4o Mini" },
        { id: "gpt-4-turbo", name: "GPT-4 Turbo" },
      ],
    },
    {
      id: "ollama",
      name: "Ollama Local AI",
      description: "100% sovereign offline inference",
      models: [
        { id: "llama3.1:8b", name: "Llama 3.1 8B" },
        { id: "llama3.1:70b", name: "Llama 3.1 70B" },
        { id: "mistral:7b", name: "Mistral 7B" },
        { id: "codellama:34b", name: "CodeLlama 34B" },
      ],
    },
  ];

  let availableModels = $derived(
    providers.find((p) => p.id === modelProvider)?.models || []
  );

  let currentAvatarUrl = $derived(
    avatarUrl || getDiceBearUrl(name || bot?.name || "Agent", avatarStyle)
  );

  async function save() {
    if (!bot) return;
    isSaving = true;

    const updatedBot = {
      ...bot,
      name,
      description,
      avatar_url: avatarUrl || currentAvatarUrl,
      avatar_style: avatarStyle,
      avatar_color: bot.avatar_color,
      status: bot.status,
      is_orchestrator: isOrchestrator,
      config: {
        ...bot.config,
        model_provider: modelProvider,
        model_id: modelId,
        temperature,
        max_tokens: maxTokens,
        custom_prompt: customPrompt || null,
      },
      updated_at: new Date().toISOString(),
    };

    try {
      await invoke("update_bot", { bot: updatedBot });
      onUpdated(updatedBot);
      onClose();
    } catch (e) {
      console.error("Failed to update bot:", e);
    } finally {
      isSaving = false;
    }
  }

  async function deleteBot() {
    if (!bot) return;
    try {
      await invoke("delete_bot", { botId: bot.id });
      onUpdated(null);
      onClose();
    } catch (e) {
      console.error("Failed to delete bot:", e);
    }
  }

  $effect(() => {
    if (bot) {
      name = bot.name || "";
      description = bot.description || "";
      avatarUrl = bot.avatar_url || null;
      avatarStyle = bot.avatar_style || "bottts";
      modelProvider = bot.config?.model_provider || "openrouter";
      modelId = bot.config?.model_id || "anthropic/claude-3-5-sonnet";
      temperature = bot.config?.temperature ?? 0.7;
      maxTokens = bot.config?.max_tokens || 4096;
      customPrompt = bot.config?.custom_prompt || "";
      isOrchestrator = Boolean(bot.is_orchestrator);
    }
  });
</script>

{#if open && bot}
  <Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
    <Dialog.Content class="sm:max-w-2xl max-h-[85vh] flex flex-col bg-[#0c0c14]/98 border border-purple-500/30 shadow-[0_0_50px_rgba(147,51,234,0.25)] backdrop-blur-2xl rounded-3xl p-0 overflow-hidden">
      <!-- Fixed Header -->
      <div class="px-6 pt-5 pb-3 border-b border-white/10 shrink-0">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-3">
            <button
              type="button"
              class="size-12 rounded-2xl overflow-hidden bg-[#181826] border-2 border-purple-500/40 p-0.5 shadow-md group hover:border-purple-400 transition-all cursor-pointer relative shrink-0"
              onclick={() => {
                activeTab = "identity";
                showAvatarPicker = true;
              }}
              title="Click to change avatar"
            >
              <img
                src={currentAvatarUrl}
                alt={name || bot.name}
                class="size-full rounded-xl object-cover"
              />
            </button>
            <div>
              <Dialog.Title class="text-base font-bold flex items-center gap-2 text-white">
                <span>{name || bot.name}</span>
                {#if isOrchestrator}
                  <span class="text-[9px] font-bold text-purple-400 bg-purple-950/80 border border-purple-800/50 px-1.5 py-0.5 rounded-md font-mono flex items-center gap-1">
                    <Crown class="size-3" />
                    Orchestrator
                  </span>
                {/if}
              </Dialog.Title>
              <Dialog.Description class="text-xs text-zinc-400">
                Configure identity, DiceBear avatar, model routing and system prompt
              </Dialog.Description>
            </div>
          </div>
        </div>

        <Tabs.Root bind:value={activeTab} class="w-full mt-3">
          <Tabs.List class="grid w-full grid-cols-3 bg-[#11111b] border border-[#202030] p-1 rounded-xl">
            <Tabs.Trigger value="model" class="gap-1.5 text-xs font-medium data-[state=active]:bg-purple-600 data-[state=active]:text-white">
              <Cpu class="size-3.5" />
              Model & Engine
            </Tabs.Trigger>
            <Tabs.Trigger value="identity" class="gap-1.5 text-xs font-medium data-[state=active]:bg-purple-600 data-[state=active]:text-white">
              <Bot class="size-3.5" />
              Identity & Avatar
            </Tabs.Trigger>
            <Tabs.Trigger value="prompt" class="gap-1.5 text-xs font-medium data-[state=active]:bg-purple-600 data-[state=active]:text-white">
              <FileCode class="size-3.5" />
              System Prompt
            </Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
      </div>

      <!-- Scrollable Tabs Content -->
      <div class="flex-1 overflow-y-auto px-6 py-4">
        {#if activeTab === "model"}
          <div class="space-y-4">
            <div class="space-y-2">
              <Label class="text-xs font-bold uppercase tracking-wider text-zinc-400">
                Model Provider
              </Label>
              <div class="grid grid-cols-2 gap-2">
                {#each providers as p}
                  <button
                    type="button"
                    class="flex flex-col text-left p-3 rounded-xl border transition-all text-xs {modelProvider === p.id ? 'border-purple-500 bg-purple-950/30 ring-1 ring-purple-500/50 shadow-[0_0_15px_rgba(147,51,234,0.15)]' : 'border-[#1f1f2e] bg-[#0f0f18]/80 hover:border-purple-500/40'} cursor-pointer"
                    onclick={() => {
                      modelProvider = p.id;
                      if (p.models.length > 0) {
                        modelId = p.models[0].id;
                      }
                    }}
                  >
                    <div class="flex items-center justify-between w-full font-bold text-white">
                      <span class="flex items-center gap-1.5">
                        {#if p.id === "ollama"}
                          <Server class="size-3.5 text-emerald-400" />
                        {:else}
                          <Cpu class="size-3.5 text-purple-400" />
                        {/if}
                        {p.name}
                      </span>
                      {#if modelProvider === p.id}
                        <Check class="size-3.5 text-purple-400" />
                      {/if}
                    </div>
                    <span class="text-[11px] text-zinc-400 mt-1 line-clamp-1">{p.description}</span>
                  </button>
                {/each}
              </div>
            </div>

            <div class="space-y-1.5">
              <Label for="model-select" class="text-xs font-bold uppercase tracking-wider text-zinc-400">
                Specific Model
              </Label>
              <select
                id="model-select"
                bind:value={modelId}
                class="w-full h-9 rounded-xl border border-[#28283e] bg-[#141424] px-3 pr-8 text-xs text-zinc-200 focus:border-purple-500 focus:ring-1 focus:ring-purple-500/30 outline-none cursor-pointer hover:bg-[#18182a]"
              >
                {#each availableModels as m}
                  <option value={m.id} class="bg-[#0e0e1a] text-zinc-100">{m.name} ({m.id})</option>
                {/each}
              </select>
            </div>

            <div class="grid grid-cols-2 gap-4 pt-1">
              <div class="space-y-2 p-3.5 rounded-2xl bg-[#0f0f18]/80 border border-[#1e1e2d]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="temp-slider" class="font-bold text-zinc-300">Temperature</Label>
                  <span class="font-mono text-[11px] px-1.5 py-0.2 rounded bg-[#181826] text-purple-300 border border-purple-500/20">{temperature.toFixed(2)}</span>
                </div>
                <input
                  id="temp-slider"
                  type="range"
                  min="0"
                  max="2"
                  step="0.05"
                  bind:value={temperature}
                  class="w-full accent-purple-500 h-1.5 bg-[#1a1a28] rounded-lg cursor-pointer"
                />
                <div class="flex justify-between text-[10px] text-zinc-500">
                  <span>Deterministic (0.0)</span>
                  <span>Creative (2.0)</span>
                </div>
              </div>

              <div class="space-y-2 p-3.5 rounded-2xl bg-[#0f0f18]/80 border border-[#1e1e2d]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="max-tokens-input" class="font-bold text-zinc-300">Max Tokens</Label>
                  <span class="font-mono text-[11px] px-1.5 py-0.2 rounded bg-[#181826] text-purple-300 border border-purple-500/20">{maxTokens}</span>
                </div>
                <Input
                  id="max-tokens-input"
                  type="number"
                  bind:value={maxTokens}
                  min="256"
                  max="128000"
                  step="256"
                  class="h-8 text-xs bg-[#141420] border-[#252538] text-zinc-200"
                />
                <div class="text-[10px] text-zinc-500">
                  Max response length in tokens
                </div>
              </div>
            </div>
          </div>
        {:else if activeTab === "identity"}
          <div class="space-y-4">
            <!-- Avatar Customization Box -->
            <div class="p-3.5 rounded-2xl bg-[#11111d] border border-purple-500/20 flex items-center justify-between">
              <div class="flex items-center gap-3.5">
                <div class="size-14 rounded-2xl overflow-hidden bg-[#181826] border-2 border-purple-500/40 p-0.5 shadow-md shrink-0">
                  <img
                    src={currentAvatarUrl}
                    alt="Current Avatar"
                    class="size-full rounded-xl object-cover"
                  />
                </div>
                <div class="flex flex-col">
                  <span class="text-sm font-bold text-white">
                    Agent Avatar
                  </span>
                  <span class="text-xs text-purple-300 capitalize font-mono mt-0.5">
                    Current Style: {avatarStyle}
                  </span>
                </div>
              </div>

              <Button
                type="button"
                variant="outline"
                size="sm"
                class="h-8 gap-1.5 text-xs bg-[#171726] border-purple-500/30 text-purple-300 hover:bg-purple-950/40 hover:text-white"
                onclick={() => (showAvatarPicker = !showAvatarPicker)}
              >
                <Palette class="size-3.5 text-purple-400" />
                {showAvatarPicker ? "Hide Picker" : "Change Look"}
              </Button>
            </div>

            <!-- Collapsible Avatar Picker -->
            {#if showAvatarPicker}
              <div class="p-3.5 rounded-2xl bg-[#090910] border border-purple-500/30 animate-in fade-in zoom-in-95">
                <AvatarPicker
                  seed={name || bot.name}
                  style={avatarStyle}
                  customUrl={avatarUrl}
                  onSelect={(url, style) => {
                    avatarUrl = url;
                    avatarStyle = style;
                  }}
                />
              </div>
            {/if}

            <div class="space-y-1.5">
              <Label for="bot-name" class="text-xs font-bold uppercase tracking-wider text-zinc-400">
                Agent Name
              </Label>
              <Input id="bot-name" bind:value={name} placeholder="e.g. Bro, Chief, Analyst..." class="h-9 text-xs bg-[#141420] border-[#252538] text-zinc-200" />
            </div>

            <div class="space-y-1.5">
              <Label for="bot-desc" class="text-xs font-bold uppercase tracking-wider text-zinc-400">
                Specialization & Mission
              </Label>
              <Input
                id="bot-desc"
                bind:value={description}
                placeholder="What does this agent specialize in?"
                class="h-9 text-xs bg-[#141420] border-[#252538] text-zinc-200"
              />
            </div>

            <div class="p-3.5 rounded-2xl border border-[#1e1e2d] bg-[#0f0f18]/80 flex items-center justify-between">
              <div class="space-y-0.5">
                <div class="flex items-center gap-1.5 font-bold text-xs text-white">
                  <Crown class="size-3.5 text-purple-400" />
                  Orchestrator Mode
                </div>
                <p class="text-[11px] text-zinc-400">
                  Allows this agent to spawn sub-tasks, delegate work, and coordinate other bots
                </p>
              </div>
              <input
                type="checkbox"
                bind:checked={isOrchestrator}
                class="size-4 accent-purple-500 rounded cursor-pointer"
              />
            </div>
          </div>
        {:else if activeTab === "prompt"}
          <div class="space-y-3">
            <div class="space-y-1.5">
              <div class="flex items-center justify-between">
                <Label for="system-prompt" class="text-xs font-bold uppercase tracking-wider text-zinc-400">
                  Custom System Directive
                </Label>
                <span class="text-[10px] text-zinc-500 font-mono">
                  {customPrompt.length} chars
                </span>
              </div>
              <Textarea
                id="system-prompt"
                bind:value={customPrompt}
                placeholder="Enter custom instructions, behavioral guidelines, constraints, and system persona..."
                rows={8}
                class="font-mono text-xs leading-relaxed bg-[#141420] border-[#252538] text-zinc-200"
              />
              <p class="text-[11px] text-zinc-500">
                Leave blank to use default RAVEN sovereign desktop agent instructions.
              </p>
            </div>
          </div>
        {/if}
      </div>

      <!-- Fixed Footer -->
      <div class="px-6 py-3.5 border-t border-white/10 bg-[#0a0a12] flex items-center justify-between shrink-0">
        <Button
          variant="destructive"
          size="sm"
          class="gap-1.5 text-xs bg-red-950/40 text-red-400 hover:bg-red-900/60 border border-red-800/40"
          onclick={() => (showDeleteConfirm = true)}
        >
          <Trash2 class="size-3.5" />
          Delete Agent
        </Button>

        <div class="flex items-center gap-2">
          <Button variant="outline" size="sm" class="bg-[#141420] border-[#252538] text-zinc-300 hover:bg-[#1a1a2a]" onclick={onClose}>
            Cancel
          </Button>
          <Button size="sm" class="gap-1.5 bg-purple-600 hover:bg-purple-500 text-white font-medium shadow-md shadow-purple-950/50" onclick={save} disabled={isSaving || !name.trim()}>
            <Save class="size-3.5" />
            {isSaving ? "Saving..." : "Save Changes"}
          </Button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Delete Confirm Dialog -->
  <Dialog.Root open={showDeleteConfirm} onOpenChange={(o) => (!o && (showDeleteConfirm = false))}>
    <Dialog.Content class="sm:max-w-md bg-[#0e0e16] border-[#252538]">
      <Dialog.Header class="gap-2">
        <div class="size-12 rounded-full bg-red-950/60 text-red-400 flex items-center justify-center mx-auto ring-8 ring-red-900/20 border border-red-800/40">
          <AlertTriangle class="size-6" />
        </div>
        <Dialog.Title class="text-center text-lg font-bold text-white">
          Delete "{bot.name}"?
        </Dialog.Title>
        <Dialog.Description class="text-center text-xs text-zinc-400">
          This will permanently remove this agent, its memory configurations, and all associated chat threads.
        </Dialog.Description>
      </Dialog.Header>

      <Dialog.Footer class="gap-2 sm:gap-0 mt-2 pt-2 border-t border-[#202030]">
        <Button variant="outline" size="sm" onclick={() => (showDeleteConfirm = false)}>
          Cancel
        </Button>
        <Button variant="destructive" size="sm" class="gap-1.5 bg-red-600 hover:bg-red-500 font-medium" onclick={deleteBot}>
          <Trash2 class="size-3.5" />
          Confirm Deletion
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
{/if}
