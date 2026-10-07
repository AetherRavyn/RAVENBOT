<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Tabs from "$lib/components/ui/tabs";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl, DEFAULT_AVATAR_STYLE } from "$lib/utils";
  import { notify } from "$lib/toast";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import ModelPicker from "$lib/components/ModelPicker.svelte";
  import WorkspaceBrowser from "$lib/components/workspace/WorkspaceBrowser.svelte";
  import {
    getCatalog, providerById, modelsFor, temperatureMax, modelMetaFor, modelSummary,
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
    FolderTree,
    KeyRound,
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
  let avatarStyle = $state(DEFAULT_AVATAR_STYLE);
  let showAvatarPicker = $state(false);

  // Seed from the owner's global default (Settings → Models) — never a
  // hard-coded provider/model pair.
  let modelProvider = $state("");
  let modelId = $state("");
  let globalDefault = $state<{ provider: string; model: string } | null>(null);
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

  /**
   * Capability narrowing.
   *
   * An agent's `permissions` list narrows what its equipped skills may do — it
   * is not a grant, and an empty list means the skill list decides. Before this
   * the field was written to the database and never read by anything, so a
   * permission a user narrowed here changed nothing at all. The two modes are
   * shown explicitly because "not narrowed" must never be mistaken for
   * "unsandboxed": the workspace boundary is enforced separately and always.
   */
  type CapabilityKey =
    | "FileSystem" | "Network" | "Shell" | "Screenshot"
    | "InputControl" | "AudioCapture" | "AudioPlayback" | "Clipboard" | "Delegation";

  /** Skills, with the capabilities each declares, so the toggles can show coverage. */
  let allSkills = $state<{ id: string; name: string; permissions?: any[] }[]>([]);

  async function loadSkills() {
    try {
      const rows = await invoke<any[]>("list_all_skills");
      // `permissions` arrives as the real `Permission` wire shape (one object
      // per variant), so the variant name is readable directly. It used to
      // arrive as `format!("{:?}", p)` and had to be taken apart by hand here.
      allSkills = rows ?? [];
    } catch (e) {
      // Coverage hints are a nicety; their absence must not block the dialog.
      allSkills = [];
    }
  }
  let narrowing = $state(false);
  let capabilityToggles = $state<Record<CapabilityKey, boolean>>({
    FileSystem: false, Network: false, Shell: false, Screenshot: false,
    InputControl: false, AudioCapture: false, AudioPlayback: false,
    Clipboard: false, Delegation: false,
  });
  /** The scope values attached to the two capabilities that carry one. */
  let fsPaths = $state("/");
  let netDomains = $state("*");

  const CAPABILITY_ROWS: {
    key: CapabilityKey;
    label: string;
    help: string;
    scoped: boolean;
  }[] = [
    { key: "FileSystem", label: "File system", help: "Read, write and list files", scoped: true },
    { key: "Shell", label: "Shell", help: "Run commands", scoped: false },
    { key: "Network", label: "Network", help: "Reach the network", scoped: true },
    { key: "Screenshot", label: "Screen capture", help: "Capture the screen", scoped: false },
    { key: "InputControl", label: "Input control", help: "Move the mouse and type", scoped: false },
    { key: "Clipboard", label: "Clipboard", help: "Read and write the clipboard", scoped: false },
    { key: "AudioCapture", label: "Microphone", help: "Record audio", scoped: false },
    { key: "AudioPlayback", label: "Audio output", help: "Play audio", scoped: false },
    { key: "Delegation", label: "Delegation", help: "Hand work to another agent", scoped: false },
  ];

  /** The Rust `Permission` wire shape, so the toggle cannot invent a variant. */
  function permissionFor(key: string): any {
    if (key === "FileSystem") {
      return { FileSystem: { paths: fsPaths.split(",").map((s) => s.trim()).filter(Boolean) } };
    }
    if (key === "Network") {
      return { Network: { domains: netDomains.split(",").map((s) => s.trim()).filter(Boolean) } };
    }
    return { [key]: null };
  }

  const permissionsPayload = $derived.by(() => {
    if (!narrowing) return [];
    return CAPABILITY_ROWS.filter((r) => capabilityToggles[r.key as CapabilityKey])
      .map((r) => permissionFor(r.key));
  });

  /** What this agent's enabled skills need, so the toggles can show coverage. */
  const neededBySkills = $derived.by(() => {
    const needed = new Set<string>();
    for (const s of allSkills) {
      if (!bot?.skills?.includes(s.id)) continue;
      for (const p of s.permissions ?? []) {
        const key = Object.keys(p)[0];
        if (key) needed.add(key);
      }
    }
    return needed;
  });

  type WsKey =
    | "bot.workspaceIsolated"
    | "bot.workspaceInherits"
    | "bot.workspaceDefault";

  /**
   * Which isolation line to show under the folder field.
   *
   * `resolve_working_dirs` tries the per-bot override, then the thread's
   * folders, then the office's, then a default. Whichever wins, the agent is
   * confined to it — so this is not a claim about what *might* happen, it is
   * the boundary the runtime will actually enforce.
   */
  const workspaceStatus = $derived.by((): { key: WsKey; path: string } => {
    if (workingFolder.trim()) return { key: "bot.workspaceIsolated", path: workingFolder.trim() };
    if (bot) return { key: "bot.workspaceInherits", path: "" };
    return { key: "bot.workspaceDefault", path: "" };
  });

  /**
   * The folder to browse for this agent.
   *
   * The override if there is one, otherwise the folder the runtime would create
   * for it. Resolving the default rather than showing nothing is the point: the
   * automatically created workspace is the one a user is least able to inspect,
   * because the only way to learn its path was to read the source.
   */
  let defaultWorkspace = $state("");
  async function resolveDefaultWorkspace() {
    if (defaultWorkspace || !bot) return;
    try {
      defaultWorkspace = await invoke<string>("default_workspace_for", { name: bot.name || name });
    } catch {
      // The path is a convenience; the panel simply offers nothing to browse.
      defaultWorkspace = "";
    }
  }

  const browsePath = $derived(workingFolder.trim() || defaultWorkspace);
  let showWorkspace = $state(false);;
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

  // Load the catalog once per dialog open (+ the global default seed).
  let catalogRequested = $state(false);
  $effect(() => {
    // Same guard-as-consequence trap as the skills fetch below. This one has
    // only been saved by `getCatalog`'s built-in fallback to a minimal catalog,
    // so it happens to terminate today — but it terminates by accident, and a
    // fetch that ever returned `[]` would spin exactly the way the skills one
    // does.
    if (open && !catalogRequested) {
      catalogRequested = true;
      getCatalog().then((list) => { catalog = list; }).catch(() => {});
      invoke<any>("get_default_model")
        .then((d) => { if (d?.provider) globalDefault = { provider: d.provider, model: d.model }; })
        .catch(() => {});
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

  // Honest facts for the selected model (context window, $/1M) — empty when
  // we genuinely have no data rather than a guess.
  let currentModelFacts = $derived.by(() => {
    if (!modelId) return "";
    const meta = availableModels.find((m) => m.id === modelId) ?? modelMetaFor(modelProvider, modelId);
    return meta ? modelSummary(meta) : "";
  });

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
        title: t("bot.selectFolder"),
        parent: true,
      });
      if (typeof picked === "string") workingFolder = picked;
    } catch (e) {
      notify(t("bot.folderPickerFailed") + String(e), "error");
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
      // An empty list means "not narrowed", which is the default for an agent
      // that has never been given one. The runtime reads this on every tool
      // call, so it is not decorative.
      permissions: permissionsPayload,
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
      notify(t("bot.saved"), "success");
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

  /**
   * "Have I asked for this yet?", as a fact rather than as a consequence.
   *
   * ## The bug this replaces
   *
   * The skills fetch was guarded with `allSkills.length === 0`. An effect that
   * reads a value it also causes to be written re-runs when that value changes,
   * so the moment `list_all_skills` returned an **empty array** — which it does
   * for a fresh install, a user with no skills enabled, or any IPC hiccup where
   * the catch sets `allSkills = []` — the guard was still true, the fetch ran
   * again, and again, forever.
   *
   * The whole loop runs in microtasks: `loadSkills` awaits, its `.then` writes,
   * the effect re-runs, it awaits again. Nothing ever yields to a macrotask, so
   * the event loop is starved completely — timers, layout and input stop. From
   * the outside that is not a hang and not a crash. The dialog is simply
   * unresponsive, and the first thing anyone tries to do in it is pick a model.
   * That is the "model selection is not working".
   *
   * So the intent is recorded explicitly and the fetch happens at most once per
   * open, whether it returned rows, nothing, or failed.
   */
  let skillsRequested = $state(false);

  $effect(() => {
    invoke<EngineInfo[]>("list_engines")
      .then((list) => { engineOptions = list; })
      .catch(() => { engineOptions = []; });
    if (open && !skillsRequested) {
      skillsRequested = true;
      void loadSkills();
    }
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
          ttsVoices = [{ id: "default", name: t("bot.engineDefault"), provider: "auto" }];
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
      voiceId, autoRead, hostControl, workingFolder,
      narrowing, capabilityToggles, fsPaths, netDomains,
    }),
  );
  let dirty = $derived(savedSnapshot !== "" && currentSnapshot !== savedSnapshot);

  function requestClose() {
    if (dirty && !confirm(t("bot.discardConfirm"))) return;
    onClose();
  }

  $effect(() => {
    if (bot) {
      name = bot.name || "";
      description = bot.description || "";
      avatarUrl = bot.avatar_url || null;
      avatarStyle = bot.avatar_style || DEFAULT_AVATAR_STYLE;
      modelProvider = bot.config?.model_provider || globalDefault?.provider || "ollama";
      modelId = bot.config?.model_id || (bot.config?.model_provider ? "" : globalDefault?.model) || "";
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
      // Hydrate the capability narrowing from the stored list. An empty list is
      // the "not narrowed" case, and must round-trip as one.
      const stored: any[] = bot.permissions ?? [];
      narrowing = stored.length > 0;
      capabilityToggles = {
        FileSystem: false, Network: false, Shell: false, Screenshot: false,
        InputControl: false, AudioCapture: false, AudioPlayback: false,
        Clipboard: false, Delegation: false,
      };
      for (const p of stored) {
        const key = Object.keys(p ?? {})[0] as CapabilityKey | undefined;
        if (key && key in capabilityToggles) capabilityToggles[key] = true;
        if (key === "FileSystem") {
          const paths = (p as any).FileSystem?.paths ?? [];
          fsPaths = paths.length ? paths.join(", ") : "/";
        }
        if (key === "Network") {
          const domains = (p as any).Network?.domains ?? [];
          netDomains = domains.length ? domains.join(", ") : "*";
        }
      }
      // ── Everything below is deliberately untracked ──────────────────────
      //
      // This effect exists to re-hydrate when `bot` changes. It must not
      // re-run for anything else, and two calls inside it read state the effect
      // has just written:
      //
      //   refreshSandboxReport(sandboxTier)   reads the tier we assigned above
      //   resolveDefaultWorkspace()           reads `defaultWorkspace`, which
      //                                        this effect resets to "" first
      //
      // An effect that reads what it writes re-runs when the value changes, so
      // the second one ping-ponged: set the path, re-run, clear the path,
      // re-run, resolve it again — two IPC calls per lap, forever.
      //
      // The whole loop runs in microtasks, so nothing ever reaches a timer:
      // layout, input and paint stop. From outside it is not a crash and not a
      // hang. The dialog is simply inert, and the first thing anyone does in an
      // agent builder is pick a model. That is the bug.
      //
      // `untrack` says what the effect already means: re-hydrate on `bot`,
      // and at no other time.
      untrack(() => refreshSandboxReport(sandboxTier));
      // Resolved here rather than on click, because the Files button is
      // disabled until there is a path to browse — resolving it from the click
      // handler meant the button could never be pressed for an agent with no
      // override.
      defaultWorkspace = "";
      untrack(() => resolveDefaultWorkspace());
      // Baseline for dirty tracking (after the fields are assigned).
      queueMicrotask(() => {
        savedSnapshot = currentSnapshot;
      });
    }
  });
</script>

{#if open && bot}
  <Dialog.Root {open} onOpenChange={(o) => !o && requestClose()}>
    <Dialog.Content class="sm:max-w-2xl max-h-[85vh] flex flex-col bg-[var(--surface-1)] border-[var(--hairline-strong)] rounded-xl p-0 overflow-hidden">
      <!-- Fixed Header -->
      <div class="px-6 pt-5 pb-3 border-b border-[var(--hairline)] shrink-0">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-3">
            <button
              type="button"
              class="size-12 rounded-2xl overflow-hidden bg-[var(--surface-3)] border border-[var(--brand)]/40 p-0.5 shadow-md group hover:border-[var(--hairline-strong)] transition-all cursor-pointer relative shrink-0"
              onclick={() => {
                activeTab = "identity";
                showAvatarPicker = true;
              }}
              title={t("bot.changeAvatar")}
            >
              <img
                src={currentAvatarUrl}
                alt={name || bot.name}
                class="size-full rounded-xl object-cover"
              />
            </button>
            <div>
              <Dialog.Title class="text-base font-bold flex items-center gap-2 text-[var(--text-primary)]">
                <span>{name || bot.name}</span>
                {#if isOrchestrator}
                  <span class="text-[11px] font-bold text-[var(--brand-text)] bg-[var(--brand-soft)] border border-[var(--brand)]/50 px-1.5 py-0.5 rounded-md font-mono flex items-center gap-1">
                    <Crown class="size-3" />
                    {t("bot.orchestrator")}
                  </span>
                {/if}
              </Dialog.Title>
              <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
                {t("bot.configHint")}
              </Dialog.Description>
            </div>
          </div>
        </div>

        <Tabs.Root bind:value={activeTab} class="w-full mt-3">
          <Tabs.List class="grid w-full grid-cols-3 bg-[var(--surface-2)] border border-[var(--hairline)] p-1 rounded-xl">
            <Tabs.Trigger value="model" class="gap-1.5 text-xs font-medium data-[state=active]:bg-[var(--brand)] data-[state=active]:text-[var(--text-on-light)]">
              <Cpu class="size-3.5" />
              {t("bot.tabModel")}
            </Tabs.Trigger>
            <Tabs.Trigger value="identity" class="gap-1.5 text-xs font-medium data-[state=active]:bg-[var(--brand)] data-[state=active]:text-[var(--text-on-light)]">
              <Bot class="size-3.5" />
              {t("bot.tabIdentity")}
            </Tabs.Trigger>
            <Tabs.Trigger value="prompt" class="gap-1.5 text-xs font-medium data-[state=active]:bg-[var(--brand)] data-[state=active]:text-[var(--text-on-light)]">
              <FileCode class="size-3.5" />
              {t("bot.tabPrompt")}
            </Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
      </div>

      <!-- Scrollable Tabs Content -->
      <div class="flex-1 overflow-y-auto no-scrollbar px-6 py-4 pb-10">
        {#if activeTab === "model"}
          <div class="space-y-4">
            <!-- Execution engine: native loop vs an installed agent CLI -->
            <div class="space-y-2">
              <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                {t("bot.execEngine")}
              </Label>
              <div class="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  aria-pressed={engine === 'native'}
                  class="flex flex-col text-left p-3 rounded-xl border transition-all text-xs {engine === 'native' ? 'border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/50' : 'border-[var(--hairline)] bg-[var(--surface-1)]/80 hover:border-[var(--brand)]/40'} cursor-pointer"
                  onclick={() => (engine = "native")}
                >
                  <span class="font-bold text-[var(--text-primary)] flex items-center gap-1.5"><Server class="size-3.5" /> {t("bot.native")}</span>
                  <span class="text-[11px] text-[var(--text-tertiary)] mt-1">{t("bot.builtInLoop")}</span>
                  <span class="text-[11px] font-mono mt-1 text-success">{t("bot.alwaysAvailable")}</span>
                </button>
                {#each engineOptions as e (e.id)}
                  <button
                    type="button"
                    aria-pressed={engine === e.id}
                    disabled={!e.available}
                    title={e.available ? `${e.command} ${e.version ?? ''}` : (e.install_hint ?? t("bot.notInstalled"))}
                    class="flex flex-col text-left p-3 rounded-xl border transition-all text-xs {engine === e.id ? 'border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/50' : 'border-[var(--hairline)] bg-[var(--surface-1)]/80 hover:border-[var(--brand)]/40'} cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                    onclick={() => (engine = e.id)}
                  >
                    <span class="flex items-center justify-between w-full gap-2">
                      <span class="font-bold text-[var(--text-primary)] flex items-center gap-1.5 min-w-0">
                        <Cpu class="size-3.5 shrink-0" />
                        <span class="truncate">{e.display_name}</span>
                      </span>
                      {#if engine === e.id}<Check class="size-3.5 text-[var(--brand-text)] shrink-0" />{/if}
                    </span>
                    <span class="text-[11px] text-[var(--text-tertiary)] mt-1 line-clamp-2">{e.available ? (e.version || e.command) : (e.install_hint || t("bot.notInstalled"))}</span>
                    <span class="text-[11px] font-mono mt-1 {e.available ? 'text-success' : 'text-warning'}">
                      {e.available ? t("bot.detected") : t("bot.notInstalled")}
                    </span>
                  </button>
                {/each}
              </div>
              {#if engine !== "native"}
                {@const activeEngine = engineOptions.find((e) => e.id === engine)}
                <div class="space-y-2.5 rounded-xl border border-[var(--brand)]/25 bg-[var(--brand-soft)] p-3">
                  <p class="text-[11px] text-[var(--brand-text)] leading-relaxed">
                    {t("bot.runsOnCli1")}<span class="font-mono">{activeEngine?.display_name ?? engine}</span>{t("bot.runsOnCli2")}
                    {#if activeEngine?.sign_in_hint}
                      <span class="block text-[var(--text-tertiary)] mt-1">{activeEngine.sign_in_hint}</span>
                    {/if}
                  </p>
                  <div class="space-y-1">
                    <Label for="engine-model" class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                      {t("bot.engineModel")}
                    </Label>
                    <Input
                      id="engine-model"
                      bind:value={engineModel}
                      placeholder={activeEngine?.models?.length ? activeEngine.models[0] : t("bot.engineDefault")}
                      class="h-8 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
                    />
                    <p class="text-[11px] text-[var(--text-muted)]">{t("bot.cliDefaultHint")}</p>
                  </div>
                </div>
              {/if}
            </div>

            <div class="space-y-2 {engine !== 'native' ? 'opacity-50 pointer-events-none' : ''}">
              <div class="flex items-center justify-between">
                <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">{t("bot.provider")}</Label>
                <span class="text-[11px] font-mono text-[var(--text-muted)]">{t("bot.providersN", { n: visibleProviders.length })}</span>
              </div>

              <ModelPicker
                providers={visibleProviders}
                provider={modelProvider}
                model={modelId}
                models={availableModels}
                configured={configuredIds}
                loading={isLoadingModels}
                error={missingKey
                  ? t("bot.unlockKey", { name: activeProvider?.name ?? modelProvider })
                  : (modelLoadError
                      ? (activeProvider?.id === "ollama"
                          ? t("bot.ollamaDown")
                          : t("bot.fetchFailed"))
                      : null)}
                onSelectProvider={selectProvider}
                onSelectModel={(id) => (modelId = id)}
                onRefresh={refreshModels}
                onAddKey={openProvidersSettings}
                footerNote={discoveredModels.length > 0
                  ? t("bot.liveList", { n: discoveredModels.length })
                  : t("bot.catalogFallbacks")}
              />

              {#if modelId && !availableModels.some((m) => m.id === modelId)}
                <p class="text-[11px] text-[var(--text-muted)]">
                  {t("model.notListed", { id: modelId })}
                </p>
              {/if}
              {#if modelId && currentModelFacts}
                <p class="text-[11px] font-mono text-[var(--text-tertiary)] flex items-center gap-1.5">
                  <span class="text-[var(--brand-text)]">{modelId}</span>
                  <span>·</span>
                  <span>{currentModelFacts}</span>
                </p>
              {/if}
            </div>

            <!-- Voice & auto-read (per-bot TTS voice) -->
            <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
              <div class="flex items-center justify-between">
                <Label class="text-xs font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
                  <Volume2 class="size-3.5 text-[var(--brand-text)]" /> {t("bot.voiceTitle")}
                </Label>
                <span class="text-[11px] font-mono {ttsInfo?.multi_voice ? 'text-success' : 'text-[var(--text-muted)]'}">
                  {t("bot.engineActive", { engine: ttsInfo?.engine || "local" })}
                </span>
              </div>
              <SimpleSelect
                value={voiceId}
                options={ttsVoices.map((v) => ({
                  value: v.id,
                  label: v.provider !== "auto" ? `${v.name} · ${v.provider}` : v.name,
                }))}
                onValueChange={(v) => (voiceId = v)}
                placeholder={t("bot.engineDefault")}
                class="h-9 rounded-xl"
              />
              {#if ttsInfo && !ttsInfo.multi_voice}
                <p class="text-[11px] text-warning leading-relaxed">
                  {t("bot.singleVoice")}
                </p>
              {/if}
              <label class="flex items-center gap-2 text-xs text-[var(--text-secondary)] cursor-pointer">
                <input type="checkbox" bind:checked={autoRead} class="accent-[var(--brand)]" />
                {t("bot.autoRead")}
              </label>
            </div>

            <!-- Host computer control -->
            <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
              <Label class="text-xs font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
                <Monitor class="size-3.5 text-[var(--brand-text)]" /> {t("bot.computerControl")}
              </Label>
              <label class="flex items-start gap-2 text-xs text-[var(--text-secondary)] cursor-pointer">
                <input type="checkbox" bind:checked={hostControl} class="mt-0.5 accent-[var(--brand)]" />
                <span>
                  {t("bot.allowControl")}
                  <span class="block text-[11px] text-warning/90 mt-0.5">
                    {t("bot.controlWarn")}
                  </span>
                </span>
              </label>
            </div>

            <!-- Project folder (per-agent override) -->
            <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
              <Label for="working-folder" class="text-xs font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
                <FolderOpen class="size-3.5 text-[var(--brand-text)]" /> {t("bot.projectFolder")}
              </Label>
              <div class="flex items-center gap-2">
                <Input
                  id="working-folder"
                  bind:value={workingFolder}
                  placeholder={t("bot.folderPh")}
                  class="h-8 text-xs font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
                />
                <Button type="button" size="sm" variant="outline" class="h-8 shrink-0 gap-1.5 border-[var(--hairline)]" onclick={browseWorkingFolder}>
                  <FolderOpen class="size-3.5" /> {t("bot.browse")}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  class="h-8 shrink-0 gap-1.5 border-[var(--hairline)]"
                  disabled={!browsePath}
                  onclick={() => (showWorkspace = true)}
                  title={t("wsb.listLabel")}
                >
                  <FolderTree class="size-3.5" /> {t("wsb.browseFiles")}
                </Button>
              </div>
              <p class="text-[11px] text-[var(--text-muted)]">
                {t("bot.folderDesc")}
              </p>
              <!--
                The boundary, stated plainly. A user who has set a folder should
                not have to know that `confined()` is what keeps the agent in it,
                and a user who has not should see the agent is still confined
                rather than be left to assume it has the whole disk.
              -->
              <p class="text-[11px] flex items-start gap-1.5 text-[var(--ok)]">
                <ShieldCheck class="size-3 shrink-0 mt-px" />
                <span>
                  {t(workspaceStatus.key)}
                  {#if workspaceStatus.path}
                    <span class="font-mono text-[var(--text-secondary)] break-all">{workspaceStatus.path}</span>
                  {/if}
                </span>
              </p>

              {#if showWorkspace && browsePath}
                <div class="pt-1">
                  <WorkspaceBrowser paths={[browsePath]} subject={name} onClose={() => (showWorkspace = false)} />
                </div>
              {/if}
            </div>

            <!--
              Capability narrowing.

              The `permissions` list used to be written to the database and read
              by nothing, so a user who narrowed an agent here saw no effect at
              all. It is now checked before every tool call. The toggle is off by
              default and the state is spelled out, because "not narrowed" is the
              common case and must not be mistaken for "unsandboxed" — the
              workspace boundary above is separate, and always enforced.
            -->
            <div class="space-y-2.5 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
              <Label class="text-xs font-bold text-[var(--text-secondary)] flex items-center gap-1.5">
                <KeyRound class="size-3.5 text-[var(--brand-text)]" /> {t("bot.capabilities")}
              </Label>
              <p class="text-[11px] text-[var(--text-muted)] leading-relaxed">
                {t("bot.capabilitiesHelp")}
              </p>

              <label class="flex items-start gap-2 text-xs text-[var(--text-primary)] cursor-pointer">
                <input type="checkbox" bind:checked={narrowing} class="mt-0.5 accent-[var(--brand)]" />
                <span>
                  <span class="font-medium">{t("bot.narrowOn")}</span>
                  <span class="block text-[11px] text-[var(--text-muted)] leading-relaxed mt-0.5">
                    {t("bot.narrowOnHelp")}
                  </span>
                </span>
              </label>

              {#if narrowing}
                <p class="text-[11px] font-mono px-2 py-1 rounded-lg bg-[var(--surface-3)] text-[var(--text-secondary)] border border-[var(--hairline)]">
                  {t("bot.narrowedBy", { count: permissionsPayload.length, total: CAPABILITY_ROWS.length })}
                </p>
                <div class="space-y-1.5 pt-1">
                  {#each CAPABILITY_ROWS as row (row.key)}
                    <div class="space-y-1">
                      <label class="flex items-center gap-2 text-xs cursor-pointer px-2 py-1 rounded-lg hover:bg-[var(--surface-2)] transition-colors">
                        <input
                          type="checkbox"
                          checked={capabilityToggles[row.key]}
                          onchange={(e) => (capabilityToggles[row.key] = e.currentTarget.checked)}
                          class="accent-[var(--brand)]"
                        />
                        <span class="flex-1">
                          <span class="text-[var(--text-primary)]">{row.label}</span>
                          <span class="block text-[11px] text-[var(--text-muted)]">{row.help}</span>
                        </span>
                        {#if neededBySkills.has(row.key)}
                          <span
                            class="text-[11px] px-1.5 py-px rounded border whitespace-nowrap {capabilityToggles[row.key]
                              ? 'text-[var(--ok)] border-[var(--ok)]/30'
                              : 'text-[hsl(var(--warning))] border-[hsl(var(--warning))]/30'}"
                          >
                            {capabilityToggles[row.key] ? t("bot.neededBy") : t("bot.blockedBy")}
                          </span>
                        {/if}
                      </label>
                      {#if row.scoped && capabilityToggles[row.key]}
                        <div class="flex items-center gap-2 pl-7 pr-2">
                          <span class="text-[11px] text-[var(--text-muted)] shrink-0">{t("bot.scopeLabel")}</span>
                          {#if row.key === "FileSystem"}
                            <Input
                              bind:value={fsPaths}
                              placeholder={t("bot.scopePh")}
                              class="h-7 text-[11px] font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
                            />
                          {:else}
                            <Input
                              bind:value={netDomains}
                              placeholder={t("bot.scopePh")}
                              class="h-7 text-[11px] font-mono bg-[var(--surface-2)] border-[var(--hairline)]"
                            />
                          {/if}
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {:else}
                <p class="text-[11px] text-[var(--text-muted)]">{t("bot.unnarrowed")}</p>
              {/if}
            </div>

            <div class="grid grid-cols-2 gap-4 pt-1">
              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="temp-slider" class="font-bold text-[var(--text-secondary)]">{t("bot.temperature")}</Label>
                  <span class="font-mono text-[11px] px-1.5 py-[2px] rounded bg-[var(--surface-3)] text-[var(--brand-text)] border border-[var(--brand)]/25">{temperature.toFixed(2)}</span>
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
                <div class="flex justify-between text-[11px] text-[var(--text-muted)]">
                  <span>{t("bot.deterministic")}</span>
                  <span>{t("bot.creative", { max: tempMax.toFixed(1) })}{tempMax < 2 ? t("bot.cappedByProvider") : ''}</span>
                </div>
              </div>

              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="max-tokens-input" class="font-bold text-[var(--text-secondary)]">{t("bot.maxTokens")}</Label>
                  <span class="font-mono text-[11px] px-1.5 py-[2px] rounded bg-[var(--surface-3)] text-[var(--brand-text)] border border-[var(--brand)]/25">{maxTokens}</span>
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
                <div class="text-[11px] text-[var(--text-muted)]">
                  {t("bot.maxTokensDesc")}
                </div>
              </div>
            </div>

            <!-- Resilience: fallback provider + tool-round budget -->
            <div class="grid grid-cols-2 gap-4">
              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <Label for="fallback-provider" class="text-xs font-bold text-[var(--text-secondary)]">{t("bot.fallbackProvider")}</Label>
                <SimpleSelect
                  id="fallback-provider"
                  value={fallbackProvider}
                  options={[{ value: "", label: t("bot.none") }, ...catalog.map((p) => ({ value: p.id, label: p.name, icon: p.icon }))]}
                  onValueChange={(v) => (fallbackProvider = v)}
                  placeholder={t("bot.none")}
                  class="h-8"
                />
                <div class="text-[11px] text-[var(--text-muted)]">
                  {t("bot.fallbackDesc")}
                </div>
              </div>

              <div class="space-y-2 p-3.5 rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)]">
                <div class="flex items-center justify-between text-xs">
                  <Label for="max-rounds-input" class="font-bold text-[var(--text-secondary)]">{t("bot.maxToolRounds")}</Label>
                  <span class="font-mono text-[11px] px-1.5 py-[2px] rounded bg-[var(--surface-3)] text-[var(--brand-text)] border border-[var(--brand)]/25">{maxToolRounds}</span>
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
                <div class="text-[11px] text-[var(--text-muted)]">
                  {t("bot.roundsDesc")}
                </div>
              </div>
            </div>
          </div>
        {:else if activeTab === "identity"}
          <div class="space-y-4">
            <!-- Avatar Customization Box -->
            <div class="p-3.5 rounded-2xl bg-[var(--surface-2)] border border-[var(--brand)]/25 flex items-center justify-between">
              <div class="flex items-center gap-3.5">
                <div class="size-14 rounded-2xl overflow-hidden bg-[var(--surface-3)] border border-[var(--brand)]/40 p-0.5 shadow-md shrink-0">
                  <img
                    src={currentAvatarUrl}
                    alt={t("bot.avatarAlt")}
                    class="size-full rounded-xl object-cover"
                  />
                </div>
                <div class="flex flex-col">
                  <span class="text-sm font-bold text-[var(--text-primary)]">
                    {t("bot.agentAvatar")}
                  </span>
                  <span class="text-xs text-[var(--brand-text)] capitalize font-mono mt-0.5">
                    {t("bot.currentStyle", { style: avatarStyle })}
                  </span>
                </div>
              </div>

              <Button
                type="button"
                variant="outline"
                size="sm"
                class="h-8 gap-1.5 text-xs bg-[var(--surface-3)] border-[var(--brand)]/30 text-[var(--brand-text)] hover:bg-[var(--brand-soft)] hover:text-[var(--text-primary)]"
                aria-pressed={showAvatarPicker}
                onclick={() => (showAvatarPicker = !showAvatarPicker)}
              >
                <Palette class="size-3.5 text-[var(--brand-text)]" />
                {showAvatarPicker ? t("bot.hidePicker") : t("bot.changeLook")}
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
                {t("sidebar.agentName")}
              </Label>
              <Input id="bot-name" bind:value={name} placeholder={t("bot.namePlaceholder")} class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]" />
            </div>

            <div class="space-y-1.5">
              <Label for="bot-desc" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
                {t("sidebar.mission")}
              </Label>
              <Input
                id="bot-desc"
                bind:value={description}
                placeholder={t("bot.specialtyPlaceholder")}
                class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
              />
            </div>

            <div class="p-3.5 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 flex items-center justify-between">
              <div class="space-y-0.5">
                <div class="flex items-center gap-1.5 font-bold text-xs text-[var(--text-primary)]">
                  <Crown class="size-3.5 text-[var(--brand-text)]" />
                  {t("bot.orchestratorMode")}
                </div>
                <p class="text-[11px] text-[var(--text-tertiary)]">
                  {t("bot.orchestratorDesc")}
                </p>
              </div>
              <input
                type="checkbox"
                bind:checked={isOrchestrator}
                aria-label="Orchestrator mode"
                class="size-4 accent-[var(--brand)] rounded cursor-pointer"
              />
            </div>

            <!-- Approval mode: ask / auto / full (OpenMausBot-style gate) -->
            <div class="p-3.5 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 space-y-2.5">
              <div class="flex items-center gap-1.5 font-bold text-xs text-[var(--text-primary)]">
                <ShieldCheck class="size-3.5 text-warning" />
                {t("bot.approvalMode")}
              </div>
              <div class="grid grid-cols-3 gap-2">
                {#each [
                  { id: "ask", name: t("bot.modeAsk"), desc: t("bot.modeAskDesc") },
                  { id: "auto", name: t("bot.modeAuto"), desc: t("bot.modeAutoDesc") },
                  { id: "full", name: t("bot.modeFull"), desc: t("bot.modeFullDesc") },
                ] as opt}
                  <button
                    type="button"
                    aria-pressed={approvalMode === opt.id}
                    onclick={() => (approvalMode = opt.id as ApprovalModeId)}
                    class="p-2.5 rounded-xl border text-left transition-all cursor-pointer {approvalMode === opt.id ? 'border-warning bg-warning/30 ring-1 ring-warning/40' : 'border-[var(--hairline)] bg-[var(--surface-2)] hover:border-warning/40'}"
                  >
                    <div class="text-xs font-bold text-[var(--text-primary)]">{opt.name}</div>
                    <div class="text-[11px] text-[var(--text-tertiary)] mt-0.5">{opt.desc}</div>
                  </button>
                {/each}
              </div>
              {#if approvalMode === "full"}
                <p class="text-[11px] text-warning bg-warning/15 border border-warning/50 rounded-lg px-2.5 py-2">
                  {t("bot.fullAccessWarn")}
                </p>
              {:else if approvalMode === "auto"}
                <p class="text-[11px] text-[var(--text-tertiary)]">{t("bot.autoEditLow")}</p>
              {:else}
                <p class="text-[11px] text-[var(--text-tertiary)]">{t("bot.readOnlyFree")}</p>
              {/if}
            </div>

            <!-- Command isolation tier (real OS sandbox for shell/tools) -->
            <div class="p-3.5 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 space-y-2.5">
              <div class="flex items-center gap-1.5 font-bold text-xs text-[var(--text-primary)]">
                <ShieldCheck class="size-3.5 text-[var(--brand-text)]" />
                {t("bot.commandIsolation")}
              </div>
              <div class="grid grid-cols-3 gap-2">
                {#each [
                  { id: "OsLevel", name: t("bot.isoOsName"), desc: t("bot.isoOsDesc") },
                  { id: "Docker", name: t("bot.isoDockerName"), desc: t("bot.isoDockerDesc") },
                  { id: "Host", name: t("bot.isoHostName"), desc: t("bot.isoHostDesc") },
                ] as opt}
                  <button
                    type="button"
                    aria-pressed={sandboxTier === opt.id}
                    onclick={() => { sandboxTier = opt.id as any; refreshSandboxReport(opt.id); }}
                    class="p-2.5 rounded-xl border text-left transition-all cursor-pointer {sandboxTier === opt.id ? 'border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/40' : 'border-[var(--hairline)] bg-[var(--surface-2)] hover:border-[var(--brand)]/40'}"
                  >
                    <div class="text-xs font-bold text-[var(--text-primary)]">{opt.name}</div>
                    <div class="text-[11px] text-[var(--text-tertiary)] mt-0.5">{opt.desc}</div>
                  </button>
                {/each}
              </div>
              {#if sandboxReport}
                <div class="text-[11px] rounded-lg px-2.5 py-2 flex items-start gap-2 {sandboxReport.filesystem_isolated ? 'text-success bg-success/15 border border-success/50' : 'text-warning bg-warning/15 border border-warning/50'}" role="status">
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
                  {t("bot.directiveTitle")}
                </Label>
                <span class="text-[11px] text-[var(--text-muted)] font-mono">
                  {t("bot.charsN", { n: customPrompt.length })}
                </span>
              </div>
              <Textarea
                id="system-prompt"
                bind:value={customPrompt}
                placeholder={t("bot.instructionsPlaceholder")}
                rows={8}
                class="font-mono text-xs leading-relaxed bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
              />
              <p class="text-[11px] text-[var(--text-muted)]">
                {t("bot.directiveHint")}
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
          class="gap-1.5 text-xs"
          onclick={() => (showDeleteConfirm = true)}
        >
          <Trash2 class="size-3.5" />
          {t("bot.deleteAgent")}
        </Button>

        <div class="flex items-center gap-2">
          {#if dirty}
            <span class="text-[11px] text-warning flex items-center gap-1" title={t("bot.unsaved")}>
              <span class="size-1.5 rounded-full bg-warning"></span> {t("bot.unsavedChanges")}
            </span>
          {/if}
          <Button variant="outline" size="sm" class="bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={requestClose}>
            {t("ui.cancel")}
          </Button>
          <Button size="sm" class="gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium shadow-md " onclick={save} disabled={isSaving || !name.trim() || !dirty}>
            <Save class="size-3.5" />
            {isSaving ? t("bot.saving") : t("bot.saveChanges")}
          </Button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Delete Confirm Dialog -->
  <Dialog.Root open={showDeleteConfirm} onOpenChange={(o) => (!o && (showDeleteConfirm = false))}>
    <Dialog.Content class="sm:max-w-md bg-[var(--surface-1)] border-[var(--hairline)]">
      <Dialog.Header class="gap-2">
        <div class="size-12 rounded-full bg-destructive/15 text-destructive flex items-center justify-center mx-auto border border-destructive/40">
          <AlertTriangle class="size-6" />
        </div>
        <Dialog.Title class="text-center text-lg font-bold text-[var(--text-primary)]">
          {t("bot.deleteTitle", { name: bot.name })}
        </Dialog.Title>
        <Dialog.Description class="text-center text-xs text-[var(--text-tertiary)]">
          {t("bot.deleteDesc")}
        </Dialog.Description>
      </Dialog.Header>

      <Dialog.Footer class="gap-2 sm:gap-0 mt-2 pt-2 border-t border-[var(--hairline)]">
        <Button variant="outline" size="sm" onclick={() => (showDeleteConfirm = false)}>
          {t("ui.cancel")}
        </Button>
        <Button variant="destructive" size="sm" class="gap-1.5 font-medium" onclick={deleteBot}>
          <Trash2 class="size-3.5" />
          {t("bot.confirmDelete")}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
{/if}
