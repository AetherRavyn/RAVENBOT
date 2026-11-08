<script lang="ts">
  import RavenAvatar from "$lib/components/RavenAvatar.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import AvatarPicker from "$lib/components/AvatarPicker.svelte";
  import { getDiceBearUrl, OFFICE_TEMPLATES, DEFAULT_AVATAR_STYLE, type OfficeTemplateKey } from "$lib/utils";
  import { officeTemplateName, officeTemplateDesc } from "$lib/catalogI18n";
  import { cn } from "$lib/utils.js";
  import {
    Building2,
    Sparkles,
    Users,
    Laptop,
    TrendingUp,
    Briefcase,
    Palette,
    Plus,
    CheckCircle2,
    Wand2,
    Layers,
    FolderOpen,
    Loader2,
    RefreshCw,
    Brain,
    AlertTriangle,
  } from "@lucide/svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    onCreated: (room: any) => void;
    bots: any[];
  }

  let { open, onClose, onCreated, bots = [] }: Props = $props();

  let name = $state("");
  let description = $state("");
  let officeTemplate: OfficeTemplateKey = $state("it-office");
  let roomAvatarUrl = $state("");
  let roomAvatarStyle = $state(DEFAULT_AVATAR_STYLE);
  let showAvatarPicker = $state(false);
  let selectedMembers: { botId: string; rank: string; specialty: string }[] = $state([]);
  let isCreating = $state(false);
  // Provision the template's full org (CEO + specialists) automatically. This
  // is what turns a new office into a real team without hand-creating bots.
  let autoStaff = $state(true);

  // The AI-designed charter + roster. Loading it before the office exists means
  // the user sees who is about to be hired, and the create call has something
  // better than the template to provision.
  let blueprint = $state<any | null>(null);
  let blueprintLoading = $state(false);
  let blueprintError = $state<string | null>(null);
  let createError = $state<string | null>(null);
  let lastBlueprintKey = "";
  const blueprintKey = () =>
    `${name.trim()}|${description.trim()}|${officeTemplate}|${goal.trim()}`;

  // The office's folder, its mission, and its rules. All three are handed to
  // every agent in the room: an agent told only its job title works in the
  // abstract, while one told where the folder is and what the rules are works
  // in a place.
  let goal = $state("");
  let policy = $state("");
  let workspaceChoice = $state<"default" | "custom">("default");
  let customFolder = $state("");
  let pickingFolder = $state(false);

  /** The folder the office will use, for display before it is created. */
  let workspacePreview = $derived(
    workspaceChoice === "custom" && customFolder.trim()
      ? customFolder.trim()
      : `~/RAVENBOT/projects/${(name || "office")
          .trim()
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-+|-+$/g, "") || "workspace"}`,
  );

  async function pickFolder() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ directory: true, multiple: false, title: "Choose the office workspace" });
      if (typeof picked === "string" && picked.trim()) {
        customFolder = picked.trim();
        workspaceChoice = "custom";
      }
    } catch (e) {
      console.error("Folder picker failed:", e);
    } finally {
      pickingFolder = false;
    }
  }

  // Wizard: Office → Template → Staffing
  let step = $state(0);
  $effect(() => {
    if (!open) step = 0;
  });

  let blueprintSignature = $derived(
    `${name.trim()}|${description.trim()}|${officeTemplate}|${goal.trim()}`,
  );
  let blueprintSeq = 0;
  let blueprintElapsed = $state(0);
  let blueprintTimer: ReturnType<typeof setInterval> | null = null;

  async function loadBlueprint(sig: string) {
    const seq = ++blueprintSeq;
    blueprintLoading = true;
    blueprintError = null;
    blueprintElapsed = 0;
    if (blueprintTimer) clearInterval(blueprintTimer);
    blueprintTimer = setInterval(() => {
      blueprintElapsed += 1;
    }, 1000);

    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    try {
      // A model call can hang (unreachable provider, a hosted gateway that
      // accepts the connection and never answers). The backend has its own
      // per-candidate timeout; this is the UI's guarantee that the dialog can
      // always recover even if the IPC call itself never settles.
      const timeout = new Promise<never>((_, reject) => {
        timeoutId = setTimeout(
          () =>
            reject(
              new Error(
                "Office design timed out after 150 seconds. The model may be slow or unreachable — use the template team or retry.",
              ),
            ),
          150_000,
        );
      });
      const res = await Promise.race([
        invoke<any>("draft_office_blueprint", {
          name: name.trim(),
          mission: description.trim(),
          officeTemplate,
          brief: goal.trim(),
        }),
        timeout,
      ]);
      if (seq !== blueprintSeq) return;
      blueprint = res;
      if (res?.error) blueprintError = String(res.error);
      lastBlueprintKey = sig;
    } catch (e) {
      if (seq !== blueprintSeq) return;
      blueprint = null;
      blueprintError = String(e);
      // A rejected design must not re-trigger the effect forever. Mark this
      // signature as attempted; the user can press Redesign to try again.
      lastBlueprintKey = sig;
    } finally {
      if (blueprintTimer) {
        clearInterval(blueprintTimer);
        blueprintTimer = null;
      }
      if (timeoutId) clearTimeout(timeoutId);
      if (seq === blueprintSeq) blueprintLoading = false;
    }
  }

  function regenerateBlueprint() {
    if (blueprintLoading || !name.trim()) return;
    lastBlueprintKey = "";
    void loadBlueprint(blueprintSignature);
  }

  /**
   * Stop waiting and let the create button use the template roster.
   *
   * The IPC call is not cancellable, so invalidate its sequence and ignore the
   * result if it arrives later. This is the difference between "the model is
   * slow" and "the dialog is stuck".
   */
  function useTemplateNow() {
    blueprintSeq++;
    if (blueprintTimer) {
      clearInterval(blueprintTimer);
      blueprintTimer = null;
    }
    blueprintLoading = false;
    blueprint = null;
    blueprintError = "AI design skipped — the template team will be used.";
    lastBlueprintKey = blueprintSignature;
  }

  // Load the blueprint when the staffing step opens, and again if the name,
  // mission, template or goal changed while it was open. Debouncing is not
  // needed: the inputs are not a stream, and each change is one deliberate
  // action from the user.
  $effect(() => {
    const sig = blueprintSignature;
    if (!open || step !== 2 || !autoStaff || !name.trim()) return;
    if (blueprintLoading || sig === lastBlueprintKey) return;
    void loadBlueprint(sig);
  });

  const templateIcons: Record<string, any> = {
 "it-office": Laptop,
 "marketing": TrendingUp,
 "sales": Briefcase,
 "design": Palette,
 "custom": Building2,
  };

  let templates = Object.entries(OFFICE_TEMPLATES);
  let currentTemplate = $derived(OFFICE_TEMPLATES[officeTemplate]);

  function toggleMember(bot: any, rank: string, specialty: string) {
    const idx = selectedMembers.findIndex((m) => m.botId === bot.id);
    if (idx >= 0) selectedMembers.splice(idx, 1);
    else selectedMembers.push({ botId: bot.id, rank, specialty });
    selectedMembers = [...selectedMembers];
  }

  function autoAssign() {
    selectedMembers = [];
    const ranks = currentTemplate.ranks;
    if (!ranks.length) return;
    bots.slice(0, ranks.length).forEach((bot, i) => {
      const r = ranks[i % ranks.length];
      selectedMembers.push({ botId: bot.id, rank: r.rank, specialty: r.specialty });
    });
    selectedMembers = [...selectedMembers];
  }

  async function create() {
    if (!name.trim()) return;
    isCreating = true;
    createError = null;
    try {
      const url = roomAvatarUrl || getDiceBearUrl(name, roomAvatarStyle);
      // Prefer what the user typed; fall back to the charter the model wrote
      // from the name and mission. An office created from a name alone still
      // gets a real goal and binding policy.
      const chosenGoal = goal.trim() || blueprint?.goal?.trim() || null;
      const chosenPolicy = policy.trim() || blueprint?.policy?.trim() || null;
      const room = await invoke<any>("create_chatroom", {
        name,
        description,
        officeTemplate,
        avatarUrl: url,
        avatarStyle: roomAvatarStyle,
        // Omit to get a fresh room under ~/RAVENBOT/projects/. A folder the
        // user picks becomes the office instead, and the charter is written
        // there rather than into a second, unused directory.
        projectFolder: workspaceChoice === "custom" ? customFolder.trim() || null : null,
        goal: chosenGoal,
        policy: chosenPolicy,
      });
      const roomId = room.id;
      const workspace = room.project_folders?.[0] ?? null;

      if (autoStaff) {
        // The AI roster when it produced one; the template blueprint otherwise.
        // Existing fleet agents that match a role are reused, not duplicated.
        const roles =
          Array.isArray(blueprint?.roles) && blueprint.roles.length > 0
            ? blueprint.roles
            : await invoke<any[]>("default_office_org", { officeTemplate });
        if (roles.length > 0) {
          await invoke("provision_office_org", {
            chatroomId: roomId,
            roles,
            // The model that just designed the office is the one we know
            // answered a real request; make the agents run on it.
            provider: blueprint?.provider ?? null,
            model: blueprint?.model ?? null,
          });
          window.dispatchEvent(new CustomEvent("bots-changed"));
        }
      } else {
        for (const m of selectedMembers) {
          await invoke("add_member_to_chatroom", {
            chatroomId: roomId,
            botId: m.botId,
            rank: m.rank,
            specialty: m.specialty,
          });
        }
      }

      onCreated({ ...room, workspace });
      onClose();
      name = "";
      description = "";
      goal = "";
      policy = "";
      customFolder = "";
      workspaceChoice = "default";
      selectedMembers = [];
      blueprint = null;
      blueprintError = null;
      lastBlueprintKey = "";
      step = 0;
    } catch (e) {
      createError = String(e);
      console.error("Failed to create office:", e);
    } finally {
      isCreating = false;
    }
  }

  let roomPreviewUrl = $derived(roomAvatarUrl || getDiceBearUrl(name || "office", roomAvatarStyle));
</script>

<Dialog.Root {open} onOpenChange={(o) => !o && onClose()}>
  <Dialog.Content class="sm:max-w-2xl max-h-[88vh] overflow-y-auto">
    <Dialog.Header class="pb-3 border-b border-[var(--hairline)]">
      <Dialog.Title class="text-base font-bold flex items-center gap-2.5 text-[var(--text-primary)]">
        <div class="size-8 rounded-xl bg-[var(--brand-soft)] border border-[var(--brand)]/50 flex items-center justify-center text-[var(--brand-text)]">
          <Building2 class="size-4.5" />
        </div>
        {t("office.createTitle")}
      </Dialog.Title>
      <Dialog.Description class="text-xs text-[var(--text-tertiary)]">
        {t("office.createHint")}
      </Dialog.Description>
    </Dialog.Header>

    <!-- Wizard stepper -->
    <div class="flex items-center gap-1.5 px-1 pt-3">
      {#each [{ label: t("office.stepOffice"), Icon: Building2 }, { label: t("office.stepTemplate"), Icon: Layers }, { label: t("office.stepStaffing"), Icon: Users }] as s, si}
        {@const Icon = s.Icon}
        <button
          type="button"
          disabled={si > 0 && !name.trim()}
          onclick={() => {
            if (si <= step || name.trim()) step = si;
          }}
          class={cn(
            "flex items-center gap-2 h-8 px-2.5 rounded-xl border text-[11px] font-bold uppercase tracking-wider font-mono transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-40",
            si === step
              ? "border-[var(--brand)] bg-[var(--brand-soft)] text-[var(--brand-text)]"
              : "border-[var(--hairline)] bg-[var(--surface-1)] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
          )}
        >
          <span
            class={cn(
              "size-5 rounded-lg flex items-center justify-center text-[11px] font-bold font-mono shrink-0",
              si < step
                ? "bg-success/15 text-success"
                : si === step
                  ? "bg-[var(--brand)] text-[var(--text-on-light)]"
                  : "bg-[var(--surface-3)] text-[var(--text-muted)]"
            )}
          >
            {#if si < step}<CheckCircle2 class="size-3" />{:else}{si + 1}{/if}
          </span>
          <Icon class="size-3 opacity-70" />
          {s.label}
        </button>
        {#if si < 2}<span class="flex-1 h-px bg-[var(--hairline)]"></span>{/if}
      {/each}
    </div>

    <div class="grid gap-5 py-3">
      <!-- STEP 1: Room Identity -->
      {#if step === 0}
      <div class="p-4 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 flex flex-col md:flex-row gap-4 items-start">
        <button
          type="button"
          onclick={() => (showAvatarPicker = !showAvatarPicker)}
          class="flex flex-col items-center gap-1.5 shrink-0 group focus:outline-none cursor-pointer"
        >
          <div class="size-16 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--brand)]/40 p-0.5 group-hover:scale-105 transition-transform">
            <img src={roomPreviewUrl} alt={name || t("office.fallbackName")} class="size-full rounded-full object-cover" />
          </div>
          <span class="text-[11px] text-[var(--brand-text)] flex items-center gap-1 group-hover:underline font-medium">
            <Sparkles class="size-3" />
            {t("office.changeIcon")}
          </span>
        </button>

        <div class="flex-1 space-y-3 w-full">
          <div class="space-y-1">
            <Label for="room-name" class="text-xs font-semibold text-[var(--text-tertiary)]">
              {t("room.name")}
            </Label>
            <Input
              id="room-name"
              bind:value={name}
              placeholder={t("office.createNamePh")}
              class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)]"
            />
          </div>
          <div class="space-y-1">
            <Label for="room-desc" class="text-xs font-semibold text-[var(--text-tertiary)]">
              {t("office.createMission")}
            </Label>
            <Textarea
              id="room-desc"
              bind:value={description}
              placeholder={t("office.createMissionPh")}
              rows={2}
              class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] resize-none"
            />
          </div>

          <div class="space-y-1">
            <Label for="room-goal" class="text-xs font-semibold text-[var(--text-tertiary)]">
              {t("office.goalLabel")}
            </Label>
            <Textarea
              id="room-goal"
              bind:value={goal}
              placeholder={t("office.goalPh")}
              rows={2}
              class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] resize-none"
            />
            <p class="text-[11px] text-[var(--text-muted)] leading-relaxed">
              {t("office.goalHint")}
            </p>
          </div>

          <div class="space-y-1.5">
            <Label class="text-xs font-semibold text-[var(--text-tertiary)]">
              {t("office.workspaceLabel")}
            </Label>
            <div class="flex flex-wrap items-center gap-1.5">
              <Button
                variant={workspaceChoice === "default" ? "default" : "ghost"}
                size="xs"
                onclick={() => (workspaceChoice = "default")}
              >
                {t("office.workspaceDefault")}
              </Button>
              <Button
                variant={workspaceChoice === "custom" ? "default" : "ghost"}
                size="xs"
                onclick={() => (workspaceChoice = "custom")}
              >
                {t("office.workspaceExisting")}
              </Button>
            </div>
            {#if workspaceChoice === "custom"}
              <div class="flex items-center gap-1.5">
                <Input
                  bind:value={customFolder}
                  placeholder={t("office.workspacePh")}
                  oninput={() => (workspaceChoice = "custom")}
                  class="h-8 flex-1 min-w-0 text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] font-mono"
                />
                <Button variant="ghost" size="xs" onclick={pickFolder} disabled={pickingFolder}>
                  <FolderOpen class="size-3" />
                </Button>
              </div>
            {/if}
            <div class="flex items-start gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2 py-1.5">
              <FolderOpen class="size-3 mt-0.5 shrink-0 text-[var(--text-muted)]" />
              <code class="min-w-0 flex-1 break-all text-[11px] text-[var(--text-secondary)]">{workspacePreview}</code>
            </div>
            <p class="text-[11px] text-[var(--text-muted)] leading-relaxed">
              {t("office.workspaceHint")}
            </p>
          </div>

          <div class="space-y-1">
            <Label for="room-policy" class="text-xs font-semibold text-[var(--text-tertiary)]">
              {t("office.policyLabel")}
            </Label>
            <Textarea
              id="room-policy"
              bind:value={policy}
              placeholder={t("office.policyPh")}
              rows={3}
              class="text-xs bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] resize-none font-mono"
            />
            <p class="text-[11px] text-[var(--text-muted)] leading-relaxed">
              {t("office.policyHint")}
            </p>
          </div>
        </div>
      </div>

      {#if showAvatarPicker}
        <div class="p-4 rounded-2xl border border-[var(--brand)]/40 bg-[var(--surface-1)] shadow-2xl min-w-0">
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-bold text-[var(--text-primary)]">{t("office.createAvatar")}</span>
            <Button variant="ghost" size="xs" onclick={() => (showAvatarPicker = false)}>{t("ui.done")}</Button>
          </div>
          <AvatarPicker
            seed={name || "office"}
            style={roomAvatarStyle}
            customUrl={roomAvatarUrl}
            onSelect={(url, style) => {
              roomAvatarUrl = url;
              roomAvatarStyle = style;
              showAvatarPicker = false;
            }}
          />
        </div>
      {/if}
      {/if}

      <!-- STEP 2: Template Selection -->
      {#if step === 1}
      <div class="space-y-2">
        <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
          {t("office.templateLabel")}
        </Label>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-2.5">
          {#each templates as [key, tmpl]}
            {@const IconComponent = templateIcons[key] || Building2}
            {@const isSelected = officeTemplate === key}
            <button
              type="button"
              onclick={() => (officeTemplate = key as OfficeTemplateKey)}
              aria-pressed={isSelected}
              class={cn(
 "text-left rounded-2xl border p-3 transition-all flex flex-col justify-between focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--brand)]/60 cursor-pointer",
                isSelected
                  ? "border-[var(--brand)] bg-[var(--brand-soft)] ring-1 ring-[var(--brand)]/60"
                  : "border-[var(--hairline)] bg-[var(--surface-1)] hover:border-[var(--brand)]/40 hover:bg-[var(--surface-2)]"
              )}
            >
              <div>
                <div class="flex items-center justify-between">
                  <div class="size-8 rounded-xl bg-[var(--surface-3)] border border-[var(--hairline)] flex items-center justify-center text-[var(--brand-text)]">
                    <IconComponent class="size-4" />
                  </div>
                  {#if isSelected}
                    <CheckCircle2 class="size-4 text-[var(--brand-text)]" />
                  {/if}
                </div>
                <div class="font-bold text-xs text-[var(--text-primary)] mt-2">{officeTemplateName(key)}</div>
                <p class="text-[11px] text-[var(--text-tertiary)] line-clamp-2 mt-0.5">{officeTemplateDesc(key)}</p>
              </div>

              {#if tmpl.ranks.length}
                <div class="flex flex-wrap gap-1 mt-2.5 pt-2 border-t border-[var(--hairline)]">
                  {#each tmpl.ranks.slice(0, 3) as r}
                    <span class="text-[11px] px-1.5 py-0.5 rounded bg-[var(--surface-2)] border border-[var(--hairline)] font-mono text-[var(--text-tertiary)]">
                      {r.rank}
                    </span>
                  {/each}
                  {#if tmpl.ranks.length > 3}
                    <span class="text-[11px] px-1.5 py-0.5 rounded bg-[var(--surface-3)] text-[var(--text-muted)] font-mono">
                      +{tmpl.ranks.length - 3}
                    </span>
                  {/if}
                </div>
              {/if}
            </button>
          {/each}
        </div>

        {#if currentTemplate.ranks.length}
          <div class="rounded-2xl bg-[var(--surface-1)]/80 border border-[var(--hairline)] p-3 space-y-1.5 mt-2">
            <div class="text-[11px] font-bold text-[var(--text-primary)] flex items-center gap-1.5">
              <Sparkles class="size-3.5 text-[var(--brand-text)]" />
              {t("office.rolesFor", { name: officeTemplateName(officeTemplate) })}
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-1.5 pt-1">
              {#each currentTemplate.ranks as r}
                <div class="flex items-center gap-2 text-xs bg-[var(--surface-2)] px-2.5 py-1.5 rounded-xl border border-[var(--hairline)]">
                  <span class="size-2 rounded-full shrink-0" style="background-color: {r.color}"></span>
                  <span class="font-bold text-[var(--text-primary)] text-[11px]">{r.rank}</span>
                  <span class="text-[var(--text-tertiary)] text-[11px] truncate">— {r.specialty}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
      {/if}

      <!-- STEP 3: Staffing -->
      {#if step === 2}
      <div class="space-y-2.5">
        <label class="flex items-start gap-3 p-3 rounded-2xl border border-[var(--brand)]/30 bg-[var(--brand-soft)] cursor-pointer">
          <input type="checkbox" bind:checked={autoStaff} class="mt-0.5 size-4 accent-[var(--brand)] cursor-pointer" />
          <span class="space-y-0.5">
            <span class="text-xs font-bold text-[var(--text-primary)] block">{t("office.autoStaff")}</span>
            <span class="text-[11px] text-[var(--text-tertiary)] block">
              The team and charter are designed from the office name and mission. Agents run
              autonomously inside the workspace; machine-destructive commands are blocked.
            </span>
          </span>
        </label>

        {#if autoStaff}
          <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)]/80 p-3 space-y-2" aria-live="polite">
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2 min-w-0">
                <span class="size-7 rounded-lg bg-[var(--brand-soft)] border border-[var(--brand)]/30 flex items-center justify-center text-[var(--brand-text)] shrink-0">
                  {#if blueprintLoading}<Loader2 class="size-3.5 animate-spin" />{:else}<Brain class="size-3.5" />{/if}
                </span>
                <div class="min-w-0">
                  <div class="text-xs font-bold text-[var(--text-primary)] truncate">
                    {blueprintLoading ? "Designing this office…" : "The office it will create"}
                  </div>
                  <div class="text-[11px] text-[var(--text-muted)]">
                    {#if blueprint?.source === "ai"}
                      Designed by {blueprint.designer_id ? blueprint.designer : "your default model"}
                    {:else if blueprint}
                      Template team (AI design unavailable)
                    {:else if blueprintError}
                      Template team (AI design failed)
                    {:else}
                      Reading the name and mission · {blueprintElapsed}s
                    {/if}
                  </div>
                </div>
              </div>
              <Button
                variant="ghost"
                size="xs"
                class="h-7 gap-1 text-[11px] text-[var(--text-tertiary)] shrink-0"
                onclick={regenerateBlueprint}
                disabled={blueprintLoading || !name.trim()}
              >
                <RefreshCw class="size-3" />
                Redesign
              </Button>
            </div>

            {#if blueprintLoading}
              <div class="space-y-1.5 pt-1">
                <div class="h-2.5 rounded bg-[var(--surface-3)] animate-pulse w-11/12"></div>
                <div class="h-2.5 rounded bg-[var(--surface-3)] animate-pulse w-9/12"></div>
                <div class="h-2.5 rounded bg-[var(--surface-3)] animate-pulse w-10/12"></div>
              </div>
              <div class="flex items-center justify-between gap-2 pt-1">
                <span class="text-[11px] text-[var(--text-muted)] leading-snug">
                  Usually 20–60s. A slow model falls through to the next provider.
                </span>
                <Button
                  variant="ghost"
                  size="xs"
                  class="h-7 text-[11px] text-[var(--text-tertiary)] shrink-0"
                  onclick={useTemplateNow}
                >
                  Use template now
                </Button>
              </div>
            {:else if blueprint}
              {#if blueprint.capabilities}
                <div class="text-[11px] font-mono text-[var(--text-muted)]">
                  {blueprint.capabilities.skills} skills · {blueprint.capabilities.connectors} connectors · {blueprint.capabilities.plugins} plugins available
                </div>
              {/if}
              {#if blueprint.goal}
                <p class="text-[11px] leading-relaxed text-[var(--text-secondary)] border-l-2 border-[var(--brand)]/40 pl-2 line-clamp-3">
                  {blueprint.goal}
                </p>
              {/if}
              {#if blueprint.question}
                <p class="text-[11px] text-warning leading-snug flex items-start gap-1.5">
                  <AlertTriangle class="size-3 mt-0.5 shrink-0" />
                  <span>{blueprint.question}</span>
                </p>
              {/if}
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-1.5 pt-0.5">
                {#each blueprint.roles as role}
                  <div class="flex items-center gap-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 py-1.5 min-w-0">
                    <span class="size-1.5 rounded-full shrink-0" style="background-color: {role.is_lead ? '#8b5cf6' : '#79b8ff'}"></span>
                    <span class="text-[11px] font-bold text-[var(--text-primary)] truncate">{role.name}</span>
                    <span class="text-[11px] text-[var(--text-tertiary)] truncate">— {role.rank}</span>
                    <span class="ml-auto text-[11px] font-mono text-[var(--text-muted)] shrink-0 text-right">
                      {role.skills?.length ?? 0} skills{#if role.mcp_servers?.length} · {role.mcp_servers.length} MCP{/if}{#if role.plugins?.length} · {role.plugins.length} plugin{/if}
                    </span>
                  </div>
                {/each}
              </div>
            {:else if blueprintError}
              <p class="text-[11px] text-[var(--text-muted)] leading-snug">
                Could not reach the model ({blueprintError}). The template team will be hired instead — you can change it after creation.
              </p>
            {/if}
          </div>
        {/if}
      </div>

      {#if !autoStaff}
      <!-- Team Members -->
      <div class="space-y-2.5">
        <div class="flex items-center justify-between">
          <Label class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">
            {t("office.staffing", { n: selectedMembers.length })}
          </Label>
          {#if bots.length > 0}
            <Button variant="outline" size="xs" class="h-7 text-xs gap-1 bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={autoAssign}>
              <Wand2 class="size-3 text-[var(--brand-text)]" />
              {t("office.autoFill")}
            </Button>
          {/if}
        </div>

        {#if bots.length === 0}
          <div class="p-8 text-center border border-dashed border-[var(--hairline)] rounded-2xl">
            <Users class="size-8 text-[var(--text-muted)] mx-auto mb-2" />
            <p class="text-xs text-[var(--text-muted)]">{t("office.noBotsYet")}</p>
          </div>
        {:else}
          <div class="grid gap-2 max-h-56 overflow-y-auto pr-1">
            {#each bots as bot}
              {@const mem = selectedMembers.find((m) => m.botId === bot.id)}
              {@const isSelected = Boolean(mem)}
              {@const rankInfo = currentTemplate.ranks.find((r) => r.rank === mem?.rank)}
              <div
                class={cn(
 "flex items-center justify-between gap-3 rounded-2xl border p-2.5 transition-all",
                  isSelected
                    ? "border-[var(--brand)]/70 bg-[var(--brand-soft)] shadow-sm"
                    : "border-[var(--hairline)] bg-[var(--surface-1)]"
                )}
              >
                <div class="flex items-center gap-3 min-w-0">
                  <div class="size-9 rounded-full overflow-hidden bg-[var(--surface-3)] border border-[var(--hairline)] shrink-0">
                    <RavenAvatar name={bot.name} imageUrl={bot.avatar_url} style={bot.avatar_style} />
                  </div>
                  <div class="min-w-0">
                    <div class="font-bold text-xs text-[var(--text-primary)] truncate">{bot.name}</div>
                    {#if isSelected && mem}
                      <div class="flex items-center gap-1.5 mt-0.5">
                        <span
                          class="text-[11px] py-0 px-1.5 rounded text-[var(--text-primary)] font-mono font-bold"
                          style="background-color: {rankInfo?.color || '#79b8ff'}"
                        >
                          {mem.rank}
                        </span>
                        <span class="text-[11px] text-[var(--text-tertiary)] truncate">{mem.specialty}</span>
                      </div>
                    {:else}
                      <span class="text-[11px] text-[var(--text-muted)] truncate block">
                        {bot.description || t("office.generalAgent")}
                      </span>
                    {/if}
                  </div>
                </div>

                {#if isSelected && mem}
                  <Button
                    variant="ghost"
                    size="xs"
                    class="h-7 text-xs text-red-400 hover:bg-red-500/10"
                    onclick={() => toggleMember(bot, mem.rank, mem.specialty)}
                  >
                    {t("office.removeMember")}
                  </Button>
                {:else}
                  {@const autoR = currentTemplate.ranks[selectedMembers.length % (currentTemplate.ranks.length || 1)]}
                  <Button
                    variant="outline"
                    size="xs"
                    class="h-7 text-xs gap-1 shrink-0 bg-[var(--surface-3)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]"
                    onclick={() => toggleMember(bot, autoR?.rank || "Member", autoR?.specialty || "Generalist")}
                  >
                    <Plus class="size-3" />
                    {t("office.assignRank", { rank: autoR?.rank || t("ui.fallbackMember") })}
                  </Button>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      {/if}
      {/if}
    </div>

    {#if createError}
      <p class="text-[11px] text-danger leading-snug break-words px-1" role="alert">{createError}</p>
    {/if}

    <div class="flex items-center justify-between gap-2 pt-3 border-t border-[var(--hairline)]">
      <Button variant="outline" size="sm" class="bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)]" onclick={onClose} disabled={isCreating}>
        {t("ui.cancel")}
      </Button>
      <div class="flex items-center gap-2">
        {#if step > 0}
          <Button variant="outline" size="sm" class="h-9 bg-[var(--surface-2)] border-[var(--hairline)] text-[var(--text-secondary)] hover:bg-[var(--surface-3)] gap-1.5 cursor-pointer" onclick={() => (step -= 1)} disabled={isCreating}>
            {t("office.back")}
          </Button>
        {/if}
        {#if step < 2}
          <Button
            size="sm"
            class="h-9 gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium shadow-md cursor-pointer disabled:opacity-50"
            onclick={() => (step += 1)}
            disabled={!name.trim()}
          >
            {t("office.next")}
            <span class="font-mono">→</span>
          </Button>
        {:else}
          <Button
            size="sm"
            class="h-9 gap-1.5 bg-[var(--brand)] hover:bg-[var(--brand-hover)] text-[var(--text-on-light)] font-medium shadow-md cursor-pointer"
            onclick={create}
            disabled={isCreating || blueprintLoading}
          >
            {#if blueprintLoading}
              <Loader2 class="size-3.5 animate-spin" />
              Designing…
            {:else}
              <Building2 class="size-3.5" />
              {isCreating ? t("office.establishing") : t("office.createBtn", { name: name || t("office.fallbackName") })}
            {/if}
          </Button>
        {/if}
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
