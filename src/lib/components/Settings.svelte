<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Tabs from "$lib/components/ui/tabs";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl } from "$lib/utils";
  import { getStoredTheme, applyTheme, type ThemeDefinition, THEMES } from "$lib/theme";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
  import ConnectorCenter from "$lib/components/ConnectorCenter.svelte";
  import {
    Key,
    Palette,
    Server,
    Info,
    Check,
    Eye,
    EyeOff,
    ShieldCheck,
    Save,
    User,
    Wrench,
    Sparkles,
    Plus,
    Layers,
  } from "@lucide/svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    bots?: any[];
    initialTab?: string;
  }

  let { open, onClose, bots = [], initialTab = "keys" }: Props = $props();

  let activeTab = $state("keys");
  let openrouterKey = $state("");
  let anthropicKey = $state("");
  let openaiKey = $state("");
  let ollamaUrl = $state("http://localhost:11434");
  let showOpenrouter = $state(false);
  let showAnthropic = $state(false);
  let showOpenai = $state(false);
  let openrouterSaved = $state(false);
  let anthropicSaved = $state(false);
  let openaiSaved = $state(false);
  let ollamaSaved = $state(false);

  // Budgets (real enforcement — see get/set_bot_budget IPC)
  let budgetBotId = $state("");
  let budgetState = $state<any>(null);
  let budgetKind = $state<"unlimited" | "tokens" | "cost">("unlimited");
  let budgetMax = $state<number | null>(null);
  let budgetPeriod = $state("total");

  $effect(() => {
    if (budgetBotId && open) {
      loadBudget();
    }
  });

  async function loadBudget() {
    if (!budgetBotId) return;
    budgetState = null;
    try {
      const res: any = await invoke("get_bot_budget", { botId: budgetBotId });
      budgetState = res;
      const limit = res?.budget?.limit;
      budgetKind = limit?.kind || "unlimited";
      budgetMax = limit?.kind && limit.kind !== "unlimited" ? limit.max : null;
      budgetPeriod = res?.budget?.period || "total";
    } catch (e) {
      console.error("Failed to load budget:", e);
    }
  }

  async function saveBudget() {
    if (!budgetBotId) return;
    try {
      await invoke("set_bot_budget", {
        botId: budgetBotId,
        kind: budgetKind,
        max: budgetKind === "unlimited" ? 0 : Number(budgetMax || 0),
        period: budgetPeriod,
      });
      await loadBudget();
    } catch (e) {
      alert("Failed to save budget: " + String(e));
    }
  }

  async function resetBudget() {
    if (!budgetBotId) return;
    try {
      await invoke("reset_bot_budget", { botId: budgetBotId });
      await loadBudget();
    } catch (e) {
      alert("Failed to reset usage: " + String(e));
    }
  }
  let currentTheme = $state<ThemeDefinition>(getStoredTheme());
  let userAvatarUrl = $state<string | null>(null);
  let userAvatarStyle = $state("micah");
  let showUserAvatarPicker = $state(false);
  let userName = $state("");

  $effect(() => {
    if (open) {
      if (initialTab) {
        activeTab = initialTab;
      }
      if (typeof localStorage !== "undefined") {
        userAvatarUrl = localStorage.getItem("ravenbot_user_avatar");
        const s = localStorage.getItem("ravenbot_user_avatar_style");
        if (s) userAvatarStyle = s;
        userName = localStorage.getItem("ravenbot_user_name") || "";
      }
    }
  });

  function saveUserAvatar(url: string, style: string) {
    userAvatarUrl = url;
    userAvatarStyle = style;
    localStorage.setItem("ravenbot_user_avatar", url);
    localStorage.setItem("ravenbot_user_avatar_style", style);
    window.dispatchEvent(new Event("user-avatar-changed"));
    showUserAvatarPicker = false;
  }

  function pickTheme(t: ThemeDefinition) {
    currentTheme = t;
    applyTheme(t.id);
  }

  async function saveKey(provider: string, key: string) {
    try {
      await invoke("set_api_key", { provider, apiKey: key });
      if (provider === "openrouter") openrouterSaved = true;
      if (provider === "anthropic") anthropicSaved = true;
      if (provider === "openai") openaiSaved = true;
      if (provider === "ollama") ollamaSaved = true;
      setTimeout(() => {
        openrouterSaved = false;
        anthropicSaved = false;
        openaiSaved = false;
        ollamaSaved = false;
      }, 2500);
    } catch (e) {
      console.error(e);
    }
  }
</script>

{#if open}
  <Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
    <Dialog.Content
      class="{activeTab === 'mcp' ? 'sm:max-w-6xl max-w-6xl h-[90vh] overflow-hidden' : 'sm:max-w-2xl max-h-[85vh] overflow-y-auto'} w-[96vw] backdrop-blur-2xl rounded-3xl border shadow-2xl transition-all duration-200 flex flex-col p-6"
      style="background-color: {currentTheme.cardHex}f5; border-color: {currentTheme.borderHex};"
    >
      <Dialog.Header class="pb-3 border-b shrink-0" style="border-color: {currentTheme.borderHex};">
        <Dialog.Title class="text-base font-bold flex items-center gap-2.5 text-white">
          <div
            class="size-8 rounded-xl flex items-center justify-center border"
            style="background-color: {currentTheme.primaryColor}25; border-color: {currentTheme.primaryColor}50; color: {currentTheme.accentColor};"
          >
            {#if activeTab === "mcp"}
              <Layers class="size-4" />
            {:else}
              <Key class="size-4" />
            {/if}
          </div>
          <span>{activeTab === "mcp" ? "Connectors & Tools Command Center" : "Settings"}</span>
        </Dialog.Title>
        <Dialog.Description class="text-xs text-zinc-400">
          Manage API keys, vector-connected MCP tools (135+), multi-agent assignments, themes, and sovereign AI engine.
        </Dialog.Description>
      </Dialog.Header>

      <Tabs.Root bind:value={activeTab} class="w-full mt-3 flex-1 min-h-0 flex flex-col overflow-hidden">
        <Tabs.List class="grid w-full grid-cols-6 bg-black/40 border p-1 rounded-xl shrink-0" style="border-color: {currentTheme.borderHex};">
          <Tabs.Trigger value="keys" class="gap-1 text-xs data-[state=active]:bg-white/10"><Key class="size-3.5" /> Keys</Tabs.Trigger>
          <Tabs.Trigger value="mcp" class="gap-1 text-xs data-[state=active]:bg-purple-600/40 text-purple-300 data-[state=active]:text-white font-bold"><Layers class="size-3.5 text-purple-400" /> Connectors</Tabs.Trigger>
          <Tabs.Trigger value="profile" class="gap-1 text-xs data-[state=active]:bg-white/10"><User class="size-3.5" /> You</Tabs.Trigger>
          <Tabs.Trigger value="themes" class="gap-1 text-xs data-[state=active]:bg-white/10"><Palette class="size-3.5" /> Themes</Tabs.Trigger>
          <Tabs.Trigger value="local" class="gap-1 text-xs data-[state=active]:bg-white/10"><Server class="size-3.5" /> Local</Tabs.Trigger>
          <Tabs.Trigger value="about" class="gap-1 text-xs data-[state=active]:bg-white/10"><Info class="size-3.5" /> About</Tabs.Trigger>
        </Tabs.List>

        <!-- Tab 1: Connectors Center (135+ Tools & Multi-Agent Matrix) -->
        <Tabs.Content value="mcp" class="flex-1 min-h-0 overflow-hidden mt-3 flex flex-col rounded-2xl border border-white/5 bg-black/30">
          <ConnectorCenter {bots} />
        </Tabs.Content>

        <!-- Tab 2: Keys -->
        <Tabs.Content value="keys" class="space-y-4 py-3 flex-1 overflow-y-auto">
          <div class="space-y-3">
            <div class="p-4 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <div class="flex items-center justify-between gap-3 mb-2">
                <Label for="openrouter" class="text-xs font-bold flex items-center gap-2">
                  <span>OpenRouter API Key</span>
                  <span class="text-[10px] text-purple-400 font-mono font-normal">Multi-Model Cloud Gateway</span>
                </Label>
                {#if openrouterSaved}
                  <span class="text-xs text-emerald-400 font-bold flex items-center gap-1"><Check class="size-3.5" /> Saved</span>
                {/if}
              </div>
              <div class="flex gap-2">
                <div class="relative flex-1">
                  <Input
                    id="openrouter"
                    type={showOpenrouter ? "text" : "password"}
                    bind:value={openrouterKey}
                    placeholder="sk-or-v1-..."
                    class="pr-9 text-xs font-mono"
                  />
                  <button
                    type="button"
                    class="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
                    onclick={() => (showOpenrouter = !showOpenrouter)}
                  >
                    {#if showOpenrouter}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
                  </button>
                </div>
                <Button size="sm" class="gap-1.5" onclick={() => saveKey("openrouter", openrouterKey)}>
                  <Save class="size-3.5" /> Save
                </Button>
              </div>
            </div>

            <div class="p-4 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <div class="flex items-center justify-between gap-3 mb-2">
                <Label for="anthropic" class="text-xs font-bold flex items-center gap-2">
                  <span>Anthropic API Key</span>
                  <span class="text-[10px] text-amber-400 font-mono font-normal">Claude 3.5 Sonnet Direct</span>
                </Label>
                {#if anthropicSaved}
                  <span class="text-xs text-emerald-400 font-bold flex items-center gap-1"><Check class="size-3.5" /> Saved</span>
                {/if}
              </div>
              <div class="flex gap-2">
                <div class="relative flex-1">
                  <Input
                    id="anthropic"
                    type={showAnthropic ? "text" : "password"}
                    bind:value={anthropicKey}
                    placeholder="sk-ant-api03-..."
                    class="pr-9 text-xs font-mono"
                  />
                  <button
                    type="button"
                    class="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
                    onclick={() => (showAnthropic = !showAnthropic)}
                  >
                    {#if showAnthropic}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
                  </button>
                </div>
                <Button size="sm" class="gap-1.5" onclick={() => saveKey("anthropic", anthropicKey)}>
                  <Save class="size-3.5" /> Save
                </Button>
              </div>
            </div>

            <div class="p-4 rounded-2xl border bg-card/60" style="border-color: {currentTheme.borderHex};">
              <div class="flex items-center justify-between gap-3 mb-2">
                <Label for="openai" class="text-xs font-bold flex items-center gap-2">
                  <span>OpenAI API Key</span>
                  <span class="text-[10px] text-emerald-400 font-mono font-normal">GPT-4o & Embeddings</span>
                </Label>
                {#if openaiSaved}
                  <span class="text-xs text-emerald-400 font-bold flex items-center gap-1"><Check class="size-3.5" /> Saved</span>
                {/if}
              </div>
              <div class="flex gap-2">
                <div class="relative flex-1">
                  <Input
                    id="openai"
                    type={showOpenai ? "text" : "password"}
                    bind:value={openaiKey}
                    placeholder="sk-proj-..."
                    class="pr-9 text-xs font-mono"
                  />
                  <button
                    type="button"
                    class="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
                    onclick={() => (showOpenai = !showOpenai)}
                  >
                    {#if showOpenai}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
                  </button>
                </div>
                <Button size="sm" class="gap-1.5" onclick={() => saveKey("openai", openaiKey)}>
                  <Save class="size-3.5" /> Save
                </Button>
              </div>
            </div>
          </div>
        </Tabs.Content>

        <!-- Tab 3: Profile -->
        <Tabs.Content value="profile" class="space-y-4 py-3 flex-1 overflow-y-auto">
          <div class="p-4 rounded-2xl border bg-card" style="border-color: {currentTheme.borderHex};">
            <div class="flex items-center gap-4">
              <button type="button" onclick={() => (showUserAvatarPicker = !showUserAvatarPicker)} class="group shrink-0">
                <div
                  class="size-20 rounded-2xl overflow-hidden bg-muted border-2 p-1 shadow-md group-hover:scale-105 transition-transform"
                  style="border-color: {currentTheme.primaryColor}60;"
                >
                  <img
                    src={userAvatarUrl || getDiceBearUrl(userName || "You", userAvatarStyle)}
                    alt="You"
                    class="size-full rounded-xl object-cover animate-[float_3s_ease-in-out_infinite]"
                  />
                </div>
                <p class="text-[11px] text-muted-foreground mt-1 text-center">Tap to change</p>
              </button>
              <div class="flex-1 space-y-2">
                <Label for="user-name" class="text-xs font-bold">Your display name</Label>
                <Input
                  id="user-name"
                  bind:value={userName}
                  placeholder="e.g., Alex"
                  oninput={() => {
                    localStorage.setItem("ravenbot_user_name", userName);
                    window.dispatchEvent(new Event("user-avatar-changed"));
                  }}
                />
                <p class="text-xs text-muted-foreground">Seed for dicebear — change name → new animated avatar if you don't pick custom.</p>
              </div>
            </div>
            {#if showUserAvatarPicker}
              <div class="mt-4 pt-4 border-t" style="border-color: {currentTheme.borderHex};">
                <AvatarPicker seed={userName || "You"} style={userAvatarStyle} customUrl={userAvatarUrl} onSelect={saveUserAvatar} />
              </div>
            {/if}
          </div>
        </Tabs.Content>

        <!-- Tab 4: Themes -->
        <Tabs.Content value="themes" class="space-y-3 py-3 flex-1 overflow-y-auto">
          <div class="grid grid-cols-2 gap-3">
            {#each THEMES as t}
              {@const isSelected = currentTheme.id === t.id}
              <button
                type="button"
                class="flex flex-col text-left p-3.5 rounded-2xl border {isSelected ? 'shadow-xl ring-1' : 'hover:bg-white/5'}"
                style={isSelected
                  ? `background-color: ${t.cardHex}; border-color: ${t.primaryColor};`
                  : `background-color: ${t.cardHex}90; border-color: ${t.borderHex};`}
                onclick={() => pickTheme(t)}
              >
                <div class="flex items-center gap-3">
                  <div class="size-8 flex items-center justify-center">
                    <ThemeLogo theme={t} size="sm" class="!size-8" />
                  </div>
                  <div>
                    <span class="font-bold text-xs text-white block">{t.name}</span>
                    <span class="text-[10px] font-mono text-zinc-400">{t.brand.badgeLabel}</span>
                  </div>
                </div>
                <p class="text-[11px] text-zinc-400 mt-2">{t.description}</p>
              </button>
            {/each}
          </div>
        </Tabs.Content>

        <!-- Tab 5: Local AI -->
        <Tabs.Content value="local" class="space-y-4 py-3 flex-1 overflow-y-auto">
          <div class="p-4 rounded-2xl border bg-card" style="border-color: {currentTheme.borderHex};">
            <Label for="ollama-url" class="text-xs font-bold flex items-center justify-between mb-2">
              <span>Ollama Endpoint URL</span>
              {#if ollamaSaved}
                <span class="text-xs text-emerald-400 font-bold flex items-center gap-1"><Check class="size-3.5" /> Saved</span>
              {/if}
            </Label>
            <div class="flex gap-2">
              <Input id="ollama-url" bind:value={ollamaUrl} placeholder="http://localhost:11434" class="text-xs font-mono flex-1" />
              <Button size="sm" class="gap-1.5" onclick={() => saveKey("ollama", ollamaUrl)}>
                <Save class="size-3.5" /> Save
              </Button>
            </div>
            <p class="text-xs text-muted-foreground mt-2">
              Zero cloud dependency — run Llama 3.3, DeepSeek-R1, Qwen 2.5 locally on CPU/GPU.
              <span class="block mt-1 text-[11px] text-zinc-500">
                Note: the "Local (candle/llama.cpp)" provider is an experimental, unlinked engine —
                <strong>Ollama is the supported local path today.</strong>
              </span>
            </p>
          </div>

          <!-- Budgets: real per-bot enforcement -->
          <div class="p-4 rounded-2xl border bg-card" style="border-color: {currentTheme.borderHex};">
            <div class="flex items-center justify-between mb-2">
              <Label class="text-xs font-bold flex items-center gap-2">
                Agent Budgets
              </Label>
              {#if budgetBotId}
                <Button size="sm" variant="outline" class="h-6 px-2 text-[10px]" onclick={() => resetBudget()} title="Reset accumulated usage">
                  Reset usage
                </Button>
              {/if}
            </div>

            <select bind:value={budgetBotId} class="w-full h-8 px-2 mb-3 rounded-lg bg-[#12121a] border text-xs text-zinc-200" style="border-color: {currentTheme.borderHex};">
              <option value="">Select an agent…</option>
              {#each bots as b (b.id)}
                <option value={b.id}>{b.name}</option>
              {/each}
            </select>

            {#if budgetBotId && budgetState}
              <!-- Usage bar -->
              <div class="mb-3">
                <div class="flex items-center justify-between text-[10px] font-mono text-zinc-400 mb-1">
                  <span>Tokens: {budgetState.tokens_used.toLocaleString()} · Cost: ${budgetState.cost_used.toFixed(4)}</span>
                  <span class={budgetState.allowed ? "" : "text-rose-400"}>
                    {budgetState.percentage_used.toFixed(0)}% used {budgetState.allowed ? "" : "— BLOCKED"}
                  </span>
                </div>
                <div class="h-1.5 rounded-full bg-black/40 overflow-hidden">
                  <div class="h-full rounded-full transition-all {budgetState.should_warn ? 'bg-amber-400' : budgetState.allowed ? 'bg-emerald-400' : 'bg-rose-500'}"
                    style="width: {Math.min(budgetState.percentage_used, 100)}%"></div>
                </div>
              </div>

              <div class="grid grid-cols-3 gap-2">
                <select bind:value={budgetKind} class="h-8 px-2 rounded-lg bg-[#12121a] border text-xs text-zinc-200" style="border-color: {currentTheme.borderHex};">
                  <option value="unlimited">No limit</option>
                  <option value="tokens">Max tokens</option>
                  <option value="cost">Max cost ($)</option>
                </select>
                <Input type="number" bind:value={budgetMax} placeholder="limit" class="h-8 text-xs font-mono" disabled={budgetKind === "unlimited"} />
                <select bind:value={budgetPeriod} class="h-8 px-2 rounded-lg bg-[#12121a] border text-xs text-zinc-200" style="border-color: {currentTheme.borderHex};">
                  <option value="total">Lifetime</option>
                  <option value="daily">Daily</option>
                  <option value="weekly">Weekly</option>
                  <option value="monthly">Monthly</option>
                </select>
              </div>
              <div class="flex justify-end mt-2">
                <Button size="sm" class="h-7 gap-1.5 text-xs" onclick={() => saveBudget()}>
                  <Save class="size-3.5" /> Save budget
                </Button>
              </div>
            {:else if budgetBotId}
              <p class="text-[11px] text-zinc-500 font-mono">Loading budget…</p>
            {:else}
              <p class="text-[11px] text-zinc-500">
                Cap what each agent can spend. Runs are <strong>refused</strong> once the limit is hit —
                enforcement is real: check before every run, usage recorded after.
              </p>
            {/if}
          </div>
        </Tabs.Content>

        <!-- Tab 6: About -->
        <Tabs.Content value="about" class="space-y-4 py-3 flex-1 overflow-y-auto">
          <div class="p-4 rounded-2xl border bg-card space-y-3" style="border-color: {currentTheme.borderHex};">
            <div class="flex items-center gap-3">
              <ThemeLogo theme={currentTheme} size="md" />
              <div>
                <h4 class="font-bold text-sm text-white">{currentTheme.brand.brandTitle}{currentTheme.brand.brandAccent}</h4>
                <p class="text-xs text-zinc-400">{currentTheme.brand.tagline}</p>
              </div>
            </div>
            <div class="grid grid-cols-3 gap-2 pt-2 border-t text-xs" style="border-color: {currentTheme.borderHex};">
              <div class="p-2 rounded-xl bg-black/30 text-center">
                <span class="text-purple-400 font-bold block">135+</span>
                <span class="text-[10px] text-zinc-500 font-mono uppercase">MCP Tools</span>
              </div>
              <div class="p-2 rounded-xl bg-black/30 text-center">
                <span class="text-cyan-400 font-bold block">1000+</span>
                <span class="text-[10px] text-zinc-500 font-mono uppercase">Plugins</span>
              </div>
              <div class="p-2 rounded-xl bg-black/30 text-center">
                <span class="text-emerald-400 font-bold block">100%</span>
                <span class="text-[10px] text-zinc-500 font-mono uppercase">Sovereign</span>
              </div>
            </div>
          </div>
        </Tabs.Content>
      </Tabs.Root>

      <div class="flex justify-end pt-3 border-t mt-auto shrink-0" style="border-color: {currentTheme.borderHex};">
        <Button size="sm" variant="outline" class="h-8 text-xs border-[#29293e] text-zinc-300" onclick={onClose}>
          Close
        </Button>
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}
