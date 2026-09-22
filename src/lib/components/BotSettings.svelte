<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Tabs from "$lib/components/ui/tabs";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl } from "$lib/utils";
  import { notify } from "$lib/toast";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import ModelPicker from "$lib/components/ModelPicker.svelte";
  import {
    getCatalog, providerById, modelsFor, temperatureMax,
    type CatalogProvider,
  } from "$lib/model-catalog";
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
    ShieldCheck,
    Volume2,
    Monitor,
    FolderOpen,
    Loader2,
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
  // Approval mode: ask (default) / auto / full — "bots ask before they act".
  type ApprovalModeId = "ask" | "auto" | "full";
  let approvalMode = $state<ApprovalModeId>("ask");
  // Execution engine: "native" (built-in loop) or an external agent CLI.
  interface EngineInfo {
    id: string; display_name: string; command: string;
    available: boolean; version?: string | null; install_hint?: string | null;
    sign_in_hint?: string | null; models?: string[];
  }
  let engine = $state("native");
  let engineOptions = $state<EngineInfo[]>([]);
  let fallbackProvider = $state("");
  let maxToolRounds = $state(12);
  // Voice & auto-read (per-bot TTS voice)
  let voiceId = $state("default");
  let autoRead = $state(false);
  let ttsVoices = $state<{ id: string; name: string; provider: string }[]>([]);
  let ttsInfo = $state<{ engine: string; multi_voice: boolean } | null>(null);
  // Host computer control (off by default; blocked on Wayland).
  let hostControl = $state(false);
  // Optional per-agent project folder override (else inherit office/channel)
  let workingFolder = $state("");
  // Model override for external engine CLIs.
  let engineModel = $state("");
  // Command isolation tier + the effective backend report.
  interface SandboxReport {
    backend: string; filesystem_isolated: boolean; network_isolated: boolean;
    resource_limits: boolean; note: string;
  }
  let sandboxTier = $state<"OsLevel" | "Docker" | "Host">("OsLevel");
  let sandboxReport = $state<SandboxReport | null>(null);

  async function refreshSandboxReport(tier: string) {
    try {
      sandboxReport = await invoke<SandboxReport>("get_sandbox_report", { tier });
    } catch {
      sandboxReport = null;
    }
  }
  let activeTab = $state("model");
  let showDeleteConfirm = $state(false);
  let isSaving = $state(false);
  // Provider catalog (single source of truth) + live discovered models.
  // Keyless/key-missing providers still show their catalog fallbacks so the
  // picker never goes blank — OpenMausBot-style: dim with reason, not empty.
  let catalog = $state<CatalogProvider[]>([]);
  let discoveredModels = $state<any[]>([]);
  let isLoadingModels = $state(false);
  let modelLoadError = $state<string | null>(null);
  let missingKey = $state(false);

  let providers = $derived(catalog);
  let activeProvider = $derived(providerById(catalog, modelProvider));

  // Load the catalog once per dialog open
  $effect(() => {
    if (open && catalog.length === 0) {
      getCatalog().then((list) => { catalog = list; }).catch(() => {});
    }
  });

  // Fetch live models when the provider changes
  $effect(() => {
    if (!open) return;
    // touch modelProvider so the effect re-runs on switch
    const p = modelProvider;
    if (!p) return;
    loadModels(p);
  });

  async function loadModels(provider: string) {
    isLoadingModels = true;
    modelLoadError = null;
    missingKey = false;
    try {
      const models = await invoke<any[]>("fetch_provider_models", { provider });
      discoveredModels = models || [];
      // Keep the current model if it's still valid; otherwise fall to the
      // first live model, then to the catalog default (not a blind index).
      const ids = new Set(discoveredModels.map((m: any) => m.id));
      if (discoveredModels.length && !ids.has(modelId)) {
        modelId = providerById(catalog, provider)?.default_model || discoveredModels[0].id;
      }
    } catch (e: any) {
      const msg = String(e);
      modelLoadError = msg;
      missingKey = /no api key|auth/i.test(msg);
      discoveredModels = [];
    } finally {
      isLoadingModels = false;
    }
  }

  function refreshModels() {
    if (modelProvider) loadModels(modelProvider);
  }

  // Live list wins; catalog fallbacks otherwise (never a blank picker).
  let availableModels = $derived(modelsFor(activeProvider, discoveredModels, isLoadingModels));

  // Provider temperature ceiling (CommandCode ≤ 1) — clamp the slider value
  // on switch so a saved 2.0 can't 400 the next run.
  let tempMax = $derived(temperatureMax(modelProvider));
  $effect(() => {
    if (temperature > tempMax) temperature = tempMax;
  });

  // Search state + configured-provider flags for the key badges.
  let providerSearch = $state("");
  let modelSearch = $state("");
  let configuredIds = $state<string[]>([]);

  let visibleProviders = $derived(
    providerSearch.trim()
      ? catalog.filter((p) =>
          p.name.toLowerCase().includes(providerSearch.toLowerCase()) ||
          p.id.toLowerCase().includes(providerSearch.toLowerCase()) ||
          p.fallback_models.some((m) =>
            m.id.toLowerCase().includes(providerSearch.toLowerCase()) ||
            m.name.toLowerCase().includes(providerSearch.toLowerCase()),
          ),
        )
      : catalog,
  );

  let visibleModels = $derived(
    modelSearch.trim()
      ? availableModels.filter((m) =>
          m.id.toLowerCase().includes(modelSearch.toLowerCase()) ||
          m.name.toLowerCase().includes(modelSearch.toLowerCase()),
        )
      : availableModels,
  );

  function selectProvider(id: string) {
    modelProvider = id;
    providerSearch = "";
    modelSearch = "";
    // Prefer the catalog default for the new provider; the live list
    // corrects it on arrival if the default isn't offered.
    const def = providerById(catalog, id)?.default_model;
    if (def) modelId = def;
  }

  function openProvidersSettings() {
    onClose();
    window.dispatchEvent(new CustomEvent("open-settings"));
  }

  async function refreshConfiguredFlags() {
    try {
      configuredIds = await invoke<string[]>("get_configured_providers");
    } catch { configuredIds = []; }
  }

  $effect(() => {
    if (open) refreshConfiguredFlags();
  });

  let currentAvatarUrl = $derived(
    avatarUrl || getDiceBearUrl(name || bot?.name || "Agent", avatarStyle)
  );

  async function browseWorkingFolder() {
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        title: "Select project folder",
      });
      if (typeof picked === "string") workingFolder = picked;
    } catch (e) {
      notify(`Could not open folder picker: ${String(e)}`, "error");
    }
  }

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
      approval_mode: approvalMode,
      config: {
        ...bot.config,
        model_provider: modelProvider,
        model_id: modelId,
        temperature,
        max_tokens: maxTokens,
        custom_prompt: customPrompt || null,
        engine,
        fallback_provider: fallbackProvider || null,
        fallback_model: null,
        max_tool_rounds: maxToolRounds || null,
        sandbox_tier: sandboxTier,
        voice_id: voiceId === "default" ? null : voiceId,
        auto_read: autoRead,
        host_control: hostControl,
        engine_model: engineModel.trim() || null,
        working_folder: workingFolder.trim() || null,
      },
      updated_at: new Date().toISOString(),
    };

    try {
      await invoke("update_bot", { bot: updatedBot });
      // Validated approval-mode write (dedicated command normalizes the
      // value server-side instead of trusting raw bot JSON).
      try {
        const mode = await invoke<string>("set_approval_mode", { botId: bot.id, mode: approvalMode });
        updatedBot.approval_mode = mode;
      } catch (e) {
        console.error("Failed to set approval mode:", e);
      }
      savedSnapshot = currentSnapshot;
      notify("Agent saved", "success");
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
    invoke<EngineInfo[]>("list_engines")
      .then((list) => { engineOptions = list; })
      .catch(() => { engineOptions = []; });
  });

  $effect(() => {
    if (open) {
      invoke<any>("list_tts_voices")
        .then((res) => {
          ttsInfo = { engine: res?.engine || "local", multi_voice: Boolean(res?.multi_voice) };
          ttsVoices = Array.isArray(res?.voices) ? res.voices : [];
        })
        .catch(() => {
          ttsInfo = null;
          ttsVoices = [{ id: "default", name: "Engine default", provider: "auto" }];
        });
    }
  });

  // Dirty tracking: warn before discarding unsaved edits.
  let savedSnapshot = $state("");
  let currentSnapshot = $derived(
    JSON.stringify({
      name, description, avatarUrl, avatarStyle,
      modelProvider, modelId, temperature, maxTokens,
      customPrompt, isOrchestrator, approvalMode, engine, engineModel,
      fallbackProvider, maxToolRounds, sandboxTier,
      voiceId, autoRead, hostControl,
    }),
  );
  let dirty = $derived(savedSnapshot !== "" && currentSnapshot !== savedSnapshot);

  function requestClose() {
    if (dirty && !confirm("Discard unsaved changes?")) return;
    onClose();
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
      approvalMode = (bot.approval_mode === "auto" || bot.approval_mode === "full") ? bot.approval_mode : "ask";
      engine = bot.config?.engine || "native";
      fallbackProvider = bot.config?.fallback_provider || "";
      maxToolRounds = bot.config?.max_tool_rounds || 12;
      sandboxTier = bot.config?.sandbox_tier || "OsLevel";
      voiceId = bot.config?.voice_id || "default";
      autoRead = Boolean(bot.config?.auto_read);
      hostControl = Boolean(bot.config?.host_control);
      workingFolder = bot.config?.working_folder || "";
      engineModel = bot.config?.engine_model || "";
      refreshSandboxReport(sandboxTier);
      // Baseline for dirty tracking (after the fields are assigned).
      queueMicrotask(() => {
        savedSnapshot = currentSnapshot;
      });
    }
  });
</script>

{#if open && bot}
  <Dialog.Root {open} onOpenChange={(o) => !o && requestClose()}>
    <Dialog.Content class="sm:max-w-2xl max-h-[85vh] flex flex-col bg-[var(--surface-1)] border border-[var(--brand)]/30 ]  rounded-xl p-0 overflow-hidden">
      <!-- Fixed Header -->
      <div class="px-6 pt-5 pb-3 border-[var(--hairline)] border-[var(--hairline)] shrink-0">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-3">
            <button
              type="button"
              class="size-12 rounded-2xl overflow-hidden bg-[var(--surface-3)] border-[var(--hairline)] border-[var(--brand)]/40 p-0.5 shadow-md group hover:border-[var(--hairline-strong)] transition-all cursor-pointer relative shrink-0"
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
                  <span class="text-[9px] font-bold text-[var(--brand-text)] bg-[var(--brand-soft)] border border-[var(--brand)]/50 px-1.5 py-0.5 rounded-md font-mono flex items-center gap-1">
                    <Crown class="size-3" />
                    Orchestrator
                  </span>
                {/if}
              </Dialog.Title>
              <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
                Configure identity, DiceBear avatar, model routing and system prompt
              </Dialog.Description>
            </div>
          </div>
        </div>

        <Tabs.Root bind:value={activeTab} class="w-full mt-3">
          <Tabs.List class="grid w-full grid-cols-3 bg-[var(--surface-2)] border border-[var(--hairline)] p-1 rounded-xl">
            <Tabs.Trigger value="model" class="gap-1.5 text-xs font-medium data-[state=active]:bg-[var(--brand)] data-[state=active]:text-white">
              <Cpu class="size-3.5" />
              Model & Engine
            </Tabs.Trigger>
            <Tabs.Trigger value="identity" class="gap-1.5 text-xs font-medium data-[state=active]:bg-[var(--brand)] data-[state=active]:text-white">
              <Bot class="size-3.5" />
              Identity & Avatar
            </Tabs.Trigger>
            <Tabs.Trigger value="prompt" class="gap-1.5 text-xs font-medium data-[state=active]:bg-[var(--brand)] data-[state=active]:text-white">
              <FileCode class="size-3.5" />
              System Prompt
            </Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
      </div>

      <!-- Scrollable Tabs Content -->
      <div class="flex-1 overflow-y-auto px-6 py-4 pb-10">
        {#if activeTab === "model"}
          <div class="space-y-4">
            <!-- Execution engine: native loop vs an installed agent CLI -->
            <div class="space-y-2">
              <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                Execution Engine
              </Label>
              <div class="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  class="flex flex-col text-left p-3 rounded-xl border transition-all text-xs {engine === 'native' ? 'border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/50' : 'border-[var(--hairline)] bg-[var(--surface-1)]/80 hover:border-[var(--brand)]/40'} cursor-pointer"
                  onclick={() => (engine = "native")}
                >
                  <span class="font-bold text-white flex items-center gap-1.5"><Server class="size-3.5" /> Native</span>
                  <span class="text-[11px] text-[var(--text-tertiary)] mt-1">Built-in tool loop</span>
                  <span class="text-[10px] font-mono mt-1 text-success">always available</span>
                </button>
                {#each engineOptions as e (e.id)}
                  <button
                    type="button"
                    disabled={!e.available}
                    title={e.available ? `${e.command} ${e.version ?? ''}` : (e.install_hint ?? 'not installed')}
                    class="flex flex-col text-left p-3 rounded-xl border transition-all text-xs {engine === e.id ? 'border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/50' : 'border-[var(--hairline)] bg-[var(--surface-1)]/80 hover:border-[var(--brand)]/40'} cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                    onclick={() => (engine = e.id)}
                  >
                    <span class="flex items-center justify-between w-full gap-2">
                      <span class="font-bold text-white flex items-center gap-1.5 min-w-0">
                        <Cpu class="size-3.5 shrink-0" />
                        <span class="truncate">{e.display_name}</span>
                      </span>
                      {#if engine === e.id}<Check class="size-3.5 text-[var(--brand-text)] shrink-0" />{/if}
                    </span>
                    <span class="text-[11px] text-[var(--text-tertiary)] mt-1 line-clamp-2">{e.available ? (e.version || e.command) : (e.install_hint || 'not installed')}</span>
                    <span class="text-[10px] font-mono mt-1 {e.available ? 'text-success' : 'text-warning'}">
                      {e.available ? 'detected' : 'not installed'}
                    </span>
                  </button>
                {/each}
              </div>
              {#if engine !== "native"}
                {@const activeEngine = engineOptions.find((e) => e.id === engine)}
                <div class="space-y-2.5 rounded-xl border border-[var(--brand)]/25 bg-[var(--brand-soft)] p-3">
                  <p class="text-[11px] text-[var(--brand-text)] leading-relaxed">
                    This agent runs on the <span class="font-mono">{activeEngine?.display_name ?? engine}</span> CLI — it owns the tool loop,
                    model and sign-in. The provider and model below only apply when you switch back to Native.
                    {#if activeEngine?.sign_in_hint}
                      <span class="block text-[var(--text-tertiary)] mt-1">{activeEngine.sign_in_hint}</span>
                    {/if}
                  </p>
                  <div class="space-y-1">
                    <Label for="engine-model" class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                      Engine model (optional)
                    </Label>
                    <Input
                      id="engine-model"
                      bind:value={engineModel}
                      placeholder={activeEngine?.models?.length ? activeEngine.models[0] : "engine default"}
                      class="h-8 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
                    />
                    <p class="text-[10px] text-[var(--text-muted)]">Leave blank to use the CLI's own configured default.</p>
                  </div>
                </div>
              {/if}
            </div>

            <div class="space-y-2 {engine !== 'native' ? 'opacity-50 pointer-events-none' : ''}">
              <div class="flex items-center justify-between">
                <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">Model Provider</Label>
                <span class="text-[10px] font-mono text-[var(--text-muted)]">{visibleProviders.length} providers</span>
              </div>

              <ModelPicker
                providers={visibleProviders}
                provider={modelProvider}
                model={modelId}
                models={availableModels}
                configured={configuredIds}
                loading={isLoadingModels}
                error={missingKey
                  ? `Add an API key to unlock the live ${activeProvider?.name} list — catalog fallbacks shown.`
                  : (modelLoadError
                      ? (activeProvider?.id === "ollama"
                          ? "Ollama isn't reachable — start it with ollama serve."
                          : "Couldn't fetch the live model list.")
                      : null)}
                onSelectProvider={selectProvider}
                onSelectModel={(id) => (modelId = id)}
                onRefresh={refreshModels}
                onAddKey={openProvidersSettings}
                footerNote={discoveredModels.length > 0
                  ? `Live list · ${discoveredModels.length} models`
                  : "Catalog fallbacks · add a key for the live list"}
              />

              {#if modelId && !availableModels.some((m) => m.id === modelId)}
                <p class="text-[10px] text-[var(--text-muted)]">
                  Current model: <span class="font-mono text-[var(--text-secondary)]">{modelId}</span> (not listed here — kept as-is)
                </p>
              {/if}
            </div>

            <!-- Voice & auto-read (per-bot TTS voice) -->
            <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
              <div class="flex items-center justify-between">
                <Label class="text-xs font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
                  <Volume2 class="size-3.5 text-[var(--brand-text)]" /> Voice & Auto-read
                </Label>
                <span class="text-[10px] font-mono {ttsInfo?.multi_voice ? 'text-success' : 'text-[var(--text-muted)]'}">
                  {ttsInfo?.engine || "local"} engine
                </span>
              </div>
              <SimpleSelect
                value={voiceId}
                options={ttsVoices.map((v) => ({
                  value: v.id,
                  label: v.provider !== "auto" ? `${v.name} · ${v.provider}` : v.name,
                }))}
                onValueChange={(v) => (voiceId = v)}
                placeholder="Engine default"
                class="h-9 rounded-xl"
              />
              {#if ttsInfo && !ttsInfo.multi_voice}
                <p class="text-[10px] text-warning leading-relaxed">
                  Single-voice engine active. Add an OpenAI key in Settings → Providers to unlock per-bot voices.
                </p>
              {/if}
              <label class="flex items-center gap-2 text-xs text-[var(--text-secondary)] cursor-pointer">
                <input type="checkbox" bind:checked={autoRead} class="accent-[var(--brand)]" />
                Read this bot's replies aloud automatically
              </label>
            </div>

            <!-- Host computer control -->
            <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
              <Label class="text-xs font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
                <Monitor class="size-3.5 text-[var(--brand-text)]" /> Computer control
              </Label>
              <label class="flex items-start gap-2 text-xs text-[var(--text-secondary)] cursor-pointer">
                <input type="checkbox" bind:checked={hostControl} class="mt-0.5 accent-[var(--brand)]" />
                <span>
                  Allow this agent to control this computer (mouse &amp; keyboard)
                  <span class="block text-[10px] text-warning/90 mt-0.5">
                    Off by default. Blocked on Wayland sessions unless explicitly overridden. Prefer the agent's isolated desktop for risky work.
                  </span>
                </span>
              </label>
            </div>

            <!-- Project folder (per-agent override) -->
            <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
              <Label for="working-folder" class="text-xs font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
                <FolderOpen class="size-3.5 text-[var(--brand-text)]" /> Project Folder
              </Label>
              <div class="flex items-center gap-2">
                <Input
                  id="working-folder"
                  bind:value={workingFolder}
                  placeholder="~/RAVENBOT/projects/my-app (blank = inherit office)"
                  class="h-8 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
                />
                <Button type="button" size="sm" variant="outline" class="h-8 shrink-0 gap-1.5 border-[var(--hairline)]" onclick={browseWorkingFolder}>
                  <FolderOpen class="size-3.5" /> Browse…
                </Button>
              </div>
              <p class="text-[10px] text-[var(--text-muted)]">
                All file and shell work for this agent stays inside this folder. Leave blank to use the
                office's project folders (or an auto-created default).
              </p>
            </div>

            <div class="grid grid-cols-2 gap-4 pt-1">
              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="temp-slider" class="font-bold text-[var(--text-secondary)]">Temperature</Label>
                  <span class="font-mono text-[11px] px-1.5 py-0.2 rounded bg-[var(--surface-3)] text-[var(--brand-text)] border border-[var(--brand)]/25">{temperature.toFixed(2)}</span>
                </div>
                <input
                  id="temp-slider"
                  type="range"
                  min="0"
                  max={tempMax}
                  step="0.05"
                  bind:value={temperature}
                  class="w-full accent-[var(--brand)] h-1.5 bg-[var(--hairline)] rounded-lg cursor-pointer"
                />
                <div class="flex justify-between text-[10px] text-[var(--text-muted)]">
                  <span>Deterministic (0.0)</span>
                  <span>Creative ({tempMax.toFixed(1)}){tempMax < 2 ? ' · capped by provider' : ''}</span>
                </div>
              </div>

              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="max-tokens-input" class="font-bold text-[var(--text-secondary)]">Max Tokens</Label>
                  <span class="font-mono text-[11px] px-1.5 py-0.2 rounded bg-[var(--surface-3)] text-[var(--brand-text)] border border-[var(--brand)]/25">{maxTokens}</span>
                </div>
                <Input
                  id="max-tokens-input"
                  type="number"
                  bind:value={maxTokens}
                  min="256"
                  max="128000"
                  step="256"
                  class="h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
                />
                <div class="text-[10px] text-[var(--text-muted)]">
                  Max response length in tokens
                </div>
              </div>
            </div>

            <!-- Resilience: fallback provider + tool-round budget -->
            <div class="grid grid-cols-2 gap-4">
              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <Label for="fallback-provider" class="text-xs font-bold text-[var(--text-secondary)]">Fallback Provider</Label>
                <SimpleSelect
                  id="fallback-provider"
                  value={fallbackProvider}
                  options={[{ value: "", label: "None" }, ...catalog.map((p) => ({ value: p.id, label: p.name, icon: p.icon }))]}
                  onValueChange={(v) => (fallbackProvider = v)}
                  placeholder="None"
                  class="h-8"
                />
                <div class="text-[10px] text-[var(--text-muted)]">
                  Used automatically if the primary provider fails
                </div>
              </div>

              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="max-rounds-input" class="font-bold text-[var(--text-secondary)]">Max Tool Rounds</Label>
                  <span class="font-mono text-[11px] px-1.5 py-0.2 rounded bg-[var(--surface-3)] text-[var(--brand-text)] border border-[var(--brand)]/25">{maxToolRounds}</span>
                </div>
                <Input
                  id="max-rounds-input"
                  type="number"
                  bind:value={maxToolRounds}
                  min="1"
                  max="50"
                  step="1"
                  class="h-8 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
                />
                <div class="text-[10px] text-[var(--text-muted)]">
                  Model↔tool round-trip budget per run
                </div>
              </div>
            </div>
          </div>
        {:else if activeTab === "identity"}
          <div class="space-y-4">
            <!-- Avatar Customization Box -->
            <div class="p-3.5 rounded-2xl bg-[var(--surface-2)] border border-[var(--brand)]/25 flex items-center justify-between">
              <div class="flex items-center gap-3.5">
                <div class="size-14 rounded-2xl overflow-hidden bg-[var(--surface-3)] border-[var(--hairline)] border-[var(--brand)]/40 p-0.5 shadow-md shrink-0">
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
                  <span class="text-xs text-[var(--brand-text)] capitalize font-mono mt-0.5">
                    Current Style: {avatarStyle}
                  </span>
                </div>
              </div>

              <Button
                type="button"
                variant="outline"
                size="sm"
                class="h-8 gap-1.5 text-xs bg-[var(--surface-3)] border-[var(--brand)]/30 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] hover:text-[var(--text-primary)]"
                onclick={() => (showAvatarPicker = !showAvatarPicker)}
              >
                <Palette class="size-3.5 text-[var(--brand-text)]" />
                {showAvatarPicker ? "Hide Picker" : "Change Look"}
              </Button>
            </div>

            <!-- Collapsible Avatar Picker -->
            {#if showAvatarPicker}
              <div class="p-3.5 rounded-2xl bg-[var(--surface-2)] border border-[var(--brand)]/30 animate-in fade-in zoom-in-95">
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
              <Label for="bot-name" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                Agent Name
              </Label>
              <Input id="bot-name" bind:value={name} placeholder="e.g. Bro, Chief, Analyst..." class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]" />
            </div>

            <div class="space-y-1.5">
              <Label for="bot-desc" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                Specialization & Mission
              </Label>
              <Input
                id="bot-desc"
                bind:value={description}
                placeholder="What does this agent specialize in?"
                class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
              />
            </div>

            <div class="p-3.5 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 flex items-center justify-between">
              <div class="space-y-0.5">
                <div class="flex items-center gap-1.5 font-bold text-xs text-white">
                  <Crown class="size-3.5 text-[var(--brand-text)]" />
                  Orchestrator Mode
                </div>
                <p class="text-[11px] text-[var(--text-tertiary)]">
                  Allows this agent to spawn sub-tasks, delegate work, and coordinate other bots
                </p>
              </div>
              <input
                type="checkbox"
                bind:checked={isOrchestrator}
                class="size-4 accent-[var(--brand)] rounded cursor-pointer"
              />
            </div>

            <!-- Approval mode: ask / auto / full (OpenMausBot-style gate) -->
            <div class="p-3.5 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 space-y-2.5">
              <div class="flex items-center gap-1.5 font-bold text-xs text-white">
                <ShieldCheck class="size-3.5 text-warning" />
                Approval Mode
              </div>
              <div class="grid grid-cols-3 gap-2">
                {#each [
                  { id: "ask", name: "Ask", desc: "Approve every risky action" },
                  { id: "auto", name: "Auto", desc: "Only high-stakes asks" },
                  { id: "full", name: "Full", desc: "Never ask (explicit)" },
                ] as opt}
                  <button
                    type="button"
                    onclick={() => (approvalMode = opt.id as ApprovalModeId)}
                    class="p-2.5 rounded-xl border text-left transition-all cursor-pointer {approvalMode === opt.id ? 'border-warning bg-warning/30 ring-1 ring-warning/40' : 'border-[var(--hairline)] bg-[var(--surface-2)] hover:border-warning/40'}"
                  >
                    <div class="text-xs font-bold text-white">{opt.name}</div>
                    <div class="text-[10px] text-[var(--text-tertiary)] mt-0.5">{opt.desc}</div>
                  </button>
                {/each}
              </div>
              {#if approvalMode === "full"}
                <p class="text-[11px] text-warning bg-warning/30 border border-warning/20 rounded-lg px-2.5 py-2">
                  Full access: shell, file writes, git, and delegation run without asking. Only for agents you fully trust.
                </p>
              {:else if approvalMode === "auto"}
                <p class="text-[11px] text-[var(--text-tertiary)]">Low-risk writes (notes, todos, memories) run free. Shell, file edits, git, and delegation still ask.</p>
              {:else}
                <p class="text-[11px] text-[var(--text-tertiary)]">Read-only tools run free. Everything else pauses for your Allow / Deny.</p>
              {/if}
            </div>

            <!-- Command isolation tier (real OS sandbox for shell/tools) -->
            <div class="p-3.5 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 space-y-2.5">
              <div class="flex items-center gap-1.5 font-bold text-xs text-white">
                <ShieldCheck class="size-3.5 text-[var(--brand-text)]" />
                Command Isolation
              </div>
              <div class="grid grid-cols-3 gap-2">
                {#each [
                  { id: "OsLevel", name: "OS Sandbox", desc: "Namespaces + limits" },
                  { id: "Docker", name: "Container", desc: "Namespaces (docker n/a)" },
                  { id: "Host", name: "Host", desc: "No isolation" },
                ] as opt}
                  <button
                    type="button"
                    onclick={() => { sandboxTier = opt.id as any; refreshSandboxReport(opt.id); }}
                    class="p-2.5 rounded-xl border text-left transition-all cursor-pointer {sandboxTier === opt.id ? 'border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/40' : 'border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--brand)]/40'}"
                  >
                    <div class="text-xs font-bold text-white">{opt.name}</div>
                    <div class="text-[10px] text-[var(--text-tertiary)] mt-0.5">{opt.desc}</div>
                  </button>
                {/each}
              </div>
              {#if sandboxReport}
                <div class="text-[11px] rounded-lg px-2.5 py-2 flex items-start gap-2 {sandboxReport.filesystem_isolated ? 'text-success bg-success/20 border border-success/20' : 'text-warning bg-warning/30 border border-warning/20'}">
                  <span class="font-mono shrink-0">[{sandboxReport.backend}]</span>
                  <span>{sandboxReport.note}</span>
                </div>
              {/if}
            </div>
          </div>
        {:else if activeTab === "prompt"}
          <div class="space-y-3">
            <div class="space-y-1.5">
              <div class="flex items-center justify-between">
                <Label for="system-prompt" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                  Custom System Directive
                </Label>
                <span class="text-[10px] text-[var(--text-muted)] font-mono">
                  {customPrompt.length} chars
                </span>
              </div>
              <Textarea
                id="system-prompt"
                bind:value={customPrompt}
                placeholder="Enter custom instructions, behavioral guidelines, constraints, and system persona..."
                rows={8}
                class="font-mono text-xs leading-relaxed bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
              />
              <p class="text-[11px] text-[var(--text-muted)]">
                Leave blank to use default RAVEN sovereign desktop agent instructions.
              </p>
            </div>
          </div>
        {/if}
      </div>

      <!-- Fixed Footer -->
      <div class="px-6 py-3.5 border-t border-[var(--hairline)] bg-[var(--surface-1)] flex items-center justify-between shrink-0">
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
          {#if dirty}
            <span class="text-[10px] text-warning flex items-center gap-1" title="You have unsaved edits">
              <span class="size-1.5 rounded-full bg-warning"></span> Unsaved changes
            </span>
          {/if}
          <Button variant="outline" size="sm" class="bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={requestClose}>
            Cancel
          </Button>
          <Button size="sm" class="gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-white font-medium shadow-md " onclick={save} disabled={isSaving || !name.trim() || !dirty}>
            <Save class="size-3.5" />
            {isSaving ? "Saving..." : "Save Changes"}
          </Button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Delete Confirm Dialog -->
  <Dialog.Root open={showDeleteConfirm} onOpenChange={(o) => (!o && (showDeleteConfirm = false))}>
    <Dialog.Content class="sm:max-w-md bg-[var(--surface-1)] border-[var(--hairline)]">
      <Dialog.Header class="gap-2">
        <div class="size-12 rounded-full bg-red-950/60 text-red-400 flex items-center justify-center mx-auto ring-8 ring-red-900/20 border border-red-800/40">
          <AlertTriangle class="size-6" />
        </div>
        <Dialog.Title class="text-center text-lg font-bold text-white">
          Delete "{bot.name}"?
        </Dialog.Title>
        <Dialog.Description class="text-center text-xs text-[var(--text-tertiary)]">
          This will permanently remove this agent, its memory configurations, and all associated chat threads.
        </Dialog.Description>
      </Dialog.Header>

      <Dialog.Footer class="gap-2 sm:gap-0 mt-2 pt-2 border-t border-[var(--hairline)]">
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
