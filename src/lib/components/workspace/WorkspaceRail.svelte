<script lang="ts">
  import { Building2, GitBranch, Sparkles, Store, Plug, Clock, Settings, Plus, Home } from "@lucide/svelte";
  import { workspace } from "$lib/workspace.svelte";
  import { fleetActivity } from "$lib/fleetActivity.svelte";
  import { t, type TranslationKey } from "$lib/i18n";

  let {
    onOpenSettings,
    onOpenSkills,
    onOpenMarketplace,
  }: {
    onOpenSettings: () => void;
    onOpenSkills: () => void;
    onOpenMarketplace: () => void;
  } = $props();

  const sectionItems = [
    { dest: "offices", icon: Building2, label: "rail.offices" },
    // Placed beside Offices rather than at the bottom: the codebase is the
    // *product* of an office, not a utility like the connector list, and
    // utilities are where you look when something is wrong.
    { dest: "projects", icon: GitBranch, label: "rail.projects" },
  ] as const;

  const bottomItems = [
    { dest: "connectors", icon: Plug, label: "rail.mcpsConnectors" },
    { dest: "routines", icon: Clock, label: "rail.routines" },
  ] as const;

  // Fleet activity rollup: attention (approvals/questions) outranks working.
  const attentionCount = $derived(
    Object.values(fleetActivity.states).filter((a) => a === "attention").length,
  );
  const workingCount = $derived(
    Object.values(fleetActivity.states).filter((a) => a === "working").length,
  );
</script>

<nav class="shrink-0 flex flex-col items-center gap-2 py-3 bg-[var(--surface-0)] overflow-y-auto" style="scrollbar-width: none" aria-label={t("rail.ariaLabel")}>
  <button
    type="button"
    class="relative size-11 rounded-xl flex items-center justify-center cursor-pointer shrink-0 {workspace.dest === 'home'
      ? 'bg-[var(--surface-2)] text-[var(--text-primary)]'
      : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)]'}"
    style="transition-duration: var(--duration-hover)"
    onclick={() => workspace.newChat()}
    title={t("rail.home")}
    aria-label={t("rail.home")}
  >
    {#if workspace.dest === "home"}
      <span class="absolute -left-2 top-1/2 -translate-y-1/2 h-5 w-[3px] rounded-full bg-[var(--rail-selected)]"></span>
    {/if}
    <Home class="size-5" strokeWidth={1.5} />
  </button>

  <div class="w-7 border-t border-[var(--hairline-faint)] shrink-0"></div>

  <!-- Section navigation: Offices, Skills, Marketplace -->
  {#each sectionItems as item (item.dest)}
    {@const Icon = item.icon}
    <button
      type="button"
      class="relative size-11 rounded-xl flex items-center justify-center cursor-pointer shrink-0 {workspace.dest === item.dest
        ? 'bg-[var(--surface-2)] text-[var(--text-primary)]'
        : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)]'}"
      style="transition-duration: var(--duration-hover)"
      onclick={() => workspace.goto(item.dest)}
      title={t(item.label as TranslationKey)}
      aria-label={attentionCount
        ? `${t(item.label as TranslationKey)} · ${attentionCount} ${t("fleet.attention")}`
        : t(item.label as TranslationKey)}
    >
      {#if workspace.dest === item.dest}
        <span class="absolute -left-2 top-1/2 -translate-y-1/2 h-5 w-[3px] rounded-full bg-[var(--rail-selected)]"></span>
      {/if}
      <Icon class="size-5" strokeWidth={1.5} />
      {#if item.dest === "offices" && attentionCount}
        <span
          class="absolute -top-0.5 -right-0.5 min-w-[16px] h-[16px] px-1 rounded-full bg-[var(--warning-text)] text-[var(--text-on-light)] text-[11px] font-bold flex items-center justify-center"
          aria-hidden="true"
        >{attentionCount > 9 ? "9+" : attentionCount}</span>
      {:else if item.dest === "offices" && workingCount}
        <span
          class="absolute top-1 right-1 size-2 rounded-full bg-[var(--brand)] animate-pulse"
          aria-hidden="true"
        ></span>
      {/if}
    </button>
  {/each}

  <button
    type="button"
    class="size-11 rounded-xl flex items-center justify-center cursor-pointer shrink-0 text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)]"
    style="transition-duration: var(--duration-hover)"
    onclick={onOpenSkills}
    title={t("rail.skills")}
    aria-label={t("rail.skills")}
  >
    <Sparkles class="size-5" strokeWidth={1.5} />
  </button>

  <button
    type="button"
    class="size-11 rounded-xl flex items-center justify-center cursor-pointer shrink-0 text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)]"
    style="transition-duration: var(--duration-hover)"
    onclick={onOpenMarketplace}
    title={t("rail.marketplace")}
    aria-label={t("rail.marketplace")}
  >
    <Store class="size-5" strokeWidth={1.5} />
  </button>

  <button
    type="button"
    class="size-11 rounded-xl shrink-0 flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)] border border-dashed border-[var(--hairline-strong)] cursor-pointer"
    style="transition-duration: var(--duration-hover)"
    onclick={() => workspace.showAgents()}
    title={t("rail.addAgent")}
    aria-label={t("rail.addAgent")}
  >
    <Plus class="size-5" strokeWidth={1.5} />
  </button>

  <div class="flex-1 shrink-0"></div>

  <div class="flex flex-col items-center gap-1 shrink-0">
    {#each bottomItems as item (item.dest)}
      {@const Icon = item.icon}
      <button
        type="button"
        class="size-10 rounded-lg flex items-center justify-center cursor-pointer {workspace.dest === item.dest
          ? 'bg-[var(--surface-2)] text-[var(--text-primary)]'
          : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)]'}"
        style="transition-duration: var(--duration-hover)"
        onclick={() => workspace.goto(item.dest)}
        title={t(item.label as TranslationKey)}
        aria-label={t(item.label as TranslationKey)}
      >
        <Icon class="size-[18px]" strokeWidth={1.5} />
      </button>
    {/each}
    <button
      type="button"
      class="size-10 rounded-lg flex items-center justify-center text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-1)] cursor-pointer"
      style="transition-duration: var(--duration-hover)"
      onclick={onOpenSettings}
      title={t("rail.settings")}
      aria-label={t("rail.settings")}
    >
      <Settings class="size-[18px]" strokeWidth={1.5} />
    </button>
  </div>
</nav>
