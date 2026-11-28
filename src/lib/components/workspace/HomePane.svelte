<script lang="ts">
  import {
    Loader2,
    ArrowUp,
    Globe,
    Brain,
    Shield,
    Terminal,
    Paperclip,
    Mic,
  } from "@lucide/svelte";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
  import KillSwitch from "$lib/components/KillSwitch.svelte";
  import CreateChatRoom from "$lib/components/CreateChatRoom.svelte";
  import SetupChecklist from "$lib/components/workspace/SetupChecklist.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy, tick } from "svelte";
  import { workspace } from "$lib/workspace.svelte";
  import { t } from "$lib/i18n";
  import {
    nextSetupStep,
    setupSteps,
    type SetupStepId,
  } from "$lib/workspace/setupChecklist";
  import type { ThemeDefinition } from "$lib/theme";

  let { theme }: { theme: ThemeDefinition } = $props();

  let prompt = $state("");
  let promptEl = $state<HTMLTextAreaElement | null>(null);

  /**
   * First-run facts. The checklist is shown only after the probes finish, so a
   * person never sees "0 of 3" flash before a configured workspace loads.
   */
  let setupLoading = $state(true);
  let setupProviders = $state<string[]>([]);
  let setupAgent = $state<any | null>(null);
  let setupOllamaModels = $state<any[]>([]);
  let showFirstOffice = $state(false);
  let setupSeq = 0;
  let setupStepsState = $derived(
    setupSteps({
      providers: setupProviders,
      agent: setupAgent,
      ollamaModels: setupOllamaModels,
      officeCount: workspace.chatrooms.length,
    }),
  );
  let nextSetup = $derived(nextSetupStep(setupStepsState));

  async function loadSetupFacts() {
    const seq = ++setupSeq;
    const [providers, agent, local] = await Promise.all([
      invoke<string[]>("get_configured_providers").catch(() => []),
      invoke<any>("get_default_agent").catch(() => null),
      invoke<any>("probe_ollama").catch(() => null),
    ]);
    if (seq !== setupSeq) return;
    setupProviders = Array.isArray(providers) ? providers : [];
    setupAgent = agent ?? null;
    setupOllamaModels = Array.isArray(local?.models) ? local.models : [];
    setupLoading = false;
  }

  function handleSetupAction(id: SetupStepId) {
    if (id === "provider") {
      workspace.openSettings("keys");
      return;
    }
    if (id === "agent") {
      window.dispatchEvent(new CustomEvent("open-onboarding"));
      return;
    }
    showFirstOffice = true;
  }

  function onSetupFactsChanged() {
    void loadSetupFacts();
  }

  function handleTryLocalSkill(event: Event) {
    const text = (event as CustomEvent<{ prompt?: string }>).detail?.prompt;
    if (!text) return;
    prompt = text;
    void tick().then(() => promptEl?.focus());
  }

  onMount(() => {
    window.addEventListener("local-skill-try", handleTryLocalSkill);
    window.addEventListener("bots-changed", onSetupFactsChanged);
    window.addEventListener("settings-closed", onSetupFactsChanged);
    void loadSetupFacts();
  });
  onDestroy(() => {
    window.removeEventListener("local-skill-try", handleTryLocalSkill);
    window.removeEventListener("bots-changed", onSetupFactsChanged);
    window.removeEventListener("settings-closed", onSetupFactsChanged);
  });
  let deepSearch = $state(false);
  let think = $state(false);
  let targetBotId = $state<string | null>(null);
  let sending = $state(false);
  let listening = $state(false);
  let recognition: any = null;

  let selectedTarget = $derived(
    workspace.bots.find((b: any) => b.id === targetBotId) || workspace.bots[0],
  );

  async function send(preset?: string) {
    const raw = (preset || prompt).trim();
    if (!raw || sending) return;
    sending = true;
    let text = raw;
    if (deepSearch && !text.startsWith("[DeepSearch]")) text = `[DeepSearch] ${text}`;
    if (think && !text.startsWith("[Think]")) text = `[Think] ${text}`;
    const ok = await workspace.createAndSend(text, selectedTarget?.id);
    if (ok) prompt = "";
    sending = false;
  }

  function attachFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".txt,.md,.rs,.ts,.js,.py,.json,.toml,.yaml,.yml,.css,.html,.sh";
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (file) {
        const text = await file.text();
        const ext = file.name.split(".").pop() || "text";
        prompt =
          (prompt ? prompt + "\n\n" : "") +
          `Attached file [${file.name}]:\n\`\`\`${ext}\n${text}\n\`\`\`\n`;
      }
    };
    input.click();
  }

  function toggleVoice() {
    const SpeechRecognition =
      (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    if (!SpeechRecognition) {
      alert("Speech recognition is not supported in this environment. You can type directly in the composer.");
      return;
    }
    if (listening) {
      recognition?.stop();
      listening = false;
      return;
    }
    try {
      recognition = new SpeechRecognition();
      recognition.continuous = false;
      recognition.interimResults = true;
      recognition.lang = "en-US";
      recognition.onstart = () => (listening = true);
      recognition.onresult = (event: any) => {
        let text = "";
        for (let i = event.resultIndex; i < event.results.length; ++i) {
          text += event.results[i][0].transcript;
        }
        if (text) prompt = (prompt ? prompt + " " : "") + text.trim();
      };
      recognition.onerror = () => (listening = false);
      recognition.onend = () => (listening = false);
      recognition.start();
    } catch {
      listening = false;
    }
  }

  const suggestions = $derived([
    {
      icon: Terminal,
      title: t("home.s1Title"),
      text: t("home.s1Text"),
    },
    {
      icon: Brain,
      title: t("home.s2Title"),
      text: t("home.s2Text"),
    },
    {
      icon: Shield,
      title: t("home.s3Title"),
      text: t("home.s3Text"),
    },
    {
      icon: Globe,
      title: t("home.s4Title"),
      text: t("home.s4Text"),
    },
  ]);
</script>

<div class="flex-1 relative overflow-y-auto flex flex-col justify-between p-6 sm:p-10 select-none">
  <div class="max-w-2xl mx-auto w-full my-auto space-y-6 relative z-10 py-8">
    <div class="text-center space-y-4">
      <div class="flex justify-center">
        <ThemeLogo {theme} size="lg" class="!size-14 sm:!size-16" />
      </div>
      <h1 class="text-2xl font-semibold text-[var(--text-primary)] tracking-tight">
        {t("home.heading")}
      </h1>
    </div>

    {#if !setupLoading}
      <SetupChecklist steps={setupStepsState} next={nextSetup} onAction={handleSetupAction} />
    {/if}

    <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3.5 focus-within:border-[var(--brand)] transition-colors" style="transition-duration: var(--duration-normal)">
      <textarea
        bind:this={promptEl}
        bind:value={prompt}
        aria-label={t("a11y.message")}
        placeholder={t("home.placeholder")}
        rows={2}
        class="w-full bg-transparent text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] resize-none focus:outline-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 min-h-[52px] leading-relaxed font-sans"
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
          }
        }}
      ></textarea>

      <div class="flex items-center justify-between pt-3 border-t border-[var(--hairline)] mt-1">
        <div class="flex items-center gap-2 flex-wrap">
          <button
            type="button"
            class="h-7 px-2.5 rounded-md border text-xs font-medium flex items-center gap-1.5 cursor-pointer {deepSearch
              ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]'
              : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
            style="transition-duration: var(--duration-hover)"
            aria-pressed={deepSearch}
            onclick={() => (deepSearch = !deepSearch)}
            title={t("home.deepSearch")}
          >
            <Globe class="size-3" />
            <span>DeepSearch</span>
          </button>
          <button
            type="button"
            class="h-7 px-2.5 rounded-md border text-xs font-medium flex items-center gap-1.5 cursor-pointer {think
              ? 'bg-[var(--brand-soft)] text-[var(--brand-text)] border-[var(--brand)]'
              : 'border-[var(--hairline)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
            style="transition-duration: var(--duration-hover)"
            aria-pressed={think}
            onclick={() => (think = !think)}
            title={t("home.think")}
          >
            <Brain class="size-3" />
            <span>{t("home.thinkPill")}</span>
          </button>

          {#if workspace.bots.length > 0}
            <SimpleSelect
              value={selectedTarget?.id ?? ""}
              options={workspace.bots.map((bot: any) => ({ value: bot.id, label: bot.name }))}
              onValueChange={(v) => (targetBotId = v || null)}
              class="h-7 w-40 rounded-md text-xs"
            />
          {/if}

          <button
            type="button"
            aria-label={t("a11y.attachFile")}
            class="size-7 rounded-md text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)] flex items-center justify-center cursor-pointer"
            style="transition-duration: var(--duration-hover)"
            onclick={attachFile}
            title={t("home.attach")}
          >
            <Paperclip class="size-3.5" />
          </button>
          <button
            type="button"
            aria-label={listening ? "Stop listening" : "Voice input"}
            aria-pressed={listening}
            class="size-7 rounded-md flex items-center justify-center cursor-pointer {listening
              ? 'text-[var(--status-danger)] bg-[var(--brand-soft)] animate-pulse'
              : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-3)]'}"
            onclick={toggleVoice}
            title={listening ? t("home.listening") : t("home.voice")}
          >
            <Mic class="size-3.5" />
          </button>
        </div>

        <button
          type="button"
          aria-label={t("thread.send")}
          onclick={() => send()}
          disabled={!prompt.trim() || sending}
          class="size-8 rounded-md flex items-center justify-center cursor-pointer {prompt.trim() && !sending
            ? 'bg-[var(--surface-light)] text-[var(--text-on-light)] hover:bg-[var(--surface-light-hover)]'
            : 'bg-[var(--surface-3)] text-[var(--text-muted)] cursor-not-allowed opacity-60'}"
          style="transition-duration: var(--duration-hover)"
          title={t("home.send")}
        >
          {#if sending}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <ArrowUp class="size-4 stroke-[2.5]" />
          {/if}
        </button>
      </div>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 text-left">
      {#each suggestions as s (s.title)}
        {@const Icon = s.icon}
        <button
          type="button"
          class="px-3 py-2 rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)] cursor-pointer text-left flex items-center gap-2 group"
          style="transition-duration: var(--duration-hover)"
          onclick={() => send(s.text)}
        >
          <Icon class="size-3.5 shrink-0 text-[var(--text-tertiary)] group-hover:text-[var(--brand-text)]" style="transition-duration: var(--duration-hover)" />
          <span class="text-xs font-medium text-[var(--text-secondary)] group-hover:text-[var(--text-primary)]" style="transition-duration: var(--duration-hover)">{s.title}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="max-w-2xl mx-auto w-full relative z-10 pt-4">
    <KillSwitch />
  </div>
</div>

<CreateChatRoom
  open={showFirstOffice}
  bots={workspace.bots}
  onClose={() => (showFirstOffice = false)}
  onCreated={(room) => {
    showFirstOffice = false;
    workspace.handleRoomCreated(room);
  }}
/>
