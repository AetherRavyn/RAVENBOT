<script lang="ts">
  import { onMount } from "svelte";
  import { THEMES, getStoredTheme, applyTheme, subscribeTheme, type ThemeDefinition } from "$lib/theme";
  import { t } from "$lib/i18n";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
  import RavenLogo from "$lib/components/RavenLogo.svelte";
  import {
    Minus,
    Square,
    Copy,
    X,
    Palette,
    Sparkles,
    Check,
    PanelLeft,
  } from "@lucide/svelte";

  interface Props {
    sidebarCollapsed?: boolean;
    onToggleSidebar?: () => void;
  }

  let { sidebarCollapsed = false, onToggleSidebar }: Props = $props();

  let isMaximized = $state(false);
  let currentTheme = $state<ThemeDefinition>(getStoredTheme());
  let showThemeDropdown = $state(false);
  let isTauriEnv = $state(false);

  onMount(() => {
    const unsubTheme = subscribeTheme((t) => {
      currentTheme = t;
    });

    let unlisten: (() => void) | undefined;

    (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const appWindow = getCurrentWindow();
        isTauriEnv = true;
        isMaximized = await appWindow.isMaximized();

        unlisten = await appWindow.onResized(async () => {
          isMaximized = await appWindow.isMaximized();
        });
      } catch {
        isTauriEnv = false;
      }
    })();

    return () => {
      unsubTheme();
      if (unlisten) unlisten();
    };
  });

  async function handleMinimize() {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().minimize();
    } catch (e) {
      console.log("Minimize window (browser preview mode)");
    }
  }

  async function handleToggleMaximize() {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const appWindow = getCurrentWindow();
      await appWindow.toggleMaximize();
      isMaximized = await appWindow.isMaximized();
    } catch (e) {
      console.log("Toggle maximize window (browser preview mode)");
      isMaximized = !isMaximized;
    }
  }

  async function handleClose() {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().close();
    } catch (e) {
      console.log("Close window (browser preview mode)");
    }
  }

  function selectTheme(t: ThemeDefinition) {
    currentTheme = t;
    applyTheme(t.id);
    showThemeDropdown = false;
  }
</script>

<svelte:window onclick={() => (showThemeDropdown = false)} />

<!-- Custom Window TitleBar -->
<header
  data-tauri-drag-region
  class="h-[38px] border-b border-[var(--hairline)] flex items-center justify-between px-3 select-none z-50 shrink-0 text-xs bg-[var(--surface-0)]"
>
  <!-- Left Branding & Drag Region -->
  <div data-tauri-drag-region class="flex items-center gap-2">
    {#if onToggleSidebar}
      <button
        type="button"
        class="size-6 rounded-md flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-white/[0.06] transition-colors cursor-pointer"
        onclick={onToggleSidebar}
        title={t("titlebar.toggleSidebar")}
      >
        <PanelLeft class="size-3.5" />
      </button>
    {/if}
    <!-- The mark is interactive: the eyes follow the pointer and a click gets a
         reaction. It is the app's most persistent element, so it is also the
         one place a little personality earns its keep. -->
    <RavenLogo size="sm" interactive class="mr-1.5" />
    <span data-tauri-drag-region class="font-semibold tracking-tight text-[11px] text-[var(--text-primary)]">
      {currentTheme.brand.brandTitle}<span class="text-[var(--brand-text)]">{currentTheme.brand.brandAccent}</span>
    </span>
  </div>

  <!-- Center Draggable Window Zone -->
  <div data-tauri-drag-region class="flex-1 h-full flex items-center justify-center text-[10px] text-[var(--text-muted)] font-mono pointer-events-auto">
    <button
      type="button"
      class="opacity-70 hover:opacity-100 transition-opacity flex items-center gap-1.5 cursor-pointer bg-transparent border-0 text-[10px] text-[var(--text-tertiary)] font-mono py-0.5 px-2 rounded hover:bg-white/[0.06]"
      onclick={() => (showThemeDropdown = !showThemeDropdown)}
    >
      <span class="size-1.5 rounded-full" style="background-color: {currentTheme.primaryColor}"></span>
      {currentTheme.name}
    </button>
  </div>

  <!-- Right Actions: Theme Picker + Window Controls (Minimize, Maximize, Close) -->
  <div class="flex items-center gap-1">
    <!-- Theme Palette Selector Trigger -->
    <div class="relative">
      <button
        type="button"
        class="h-6 px-2 rounded-md flex items-center gap-1.5 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-white/[0.06] transition-colors cursor-pointer text-[11px]"
        onclick={(e) => {
          e.stopPropagation();
          showThemeDropdown = !showThemeDropdown;
        }}
        title={t("titlebar.switchTheme")}
      >
        <span class="size-2.5 rounded-full ring-1 ring-white/30" style="background-color: {currentTheme.primaryColor}"></span>
        <span class="hidden md:inline text-[10px] font-medium text-[var(--text-secondary)]">{t("titlebar.theme")}</span>
      </button>

      <!-- Theme Switcher Popover -->
      {#if showThemeDropdown}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute right-0 top-8 z-50 w-72 border border-[var(--hairline)] rounded-xl shadow-xl p-2.5 space-y-1.5 animate-in fade-in zoom-in-95 bg-[var(--surface-1)]"
          onclick={(e) => e.stopPropagation()}
        >
          <div class="flex items-center justify-between pb-1.5 border-b px-1" style="border-color: {currentTheme.borderHex};">
            <span class="text-xs font-bold text-[var(--text-primary)] flex items-center gap-1.5">
              <Palette class="size-3.5" style="color: {currentTheme.primaryColor}" />
              {t("titlebar.visualWorld")}
            </span>
            <span class="text-[10px] text-[var(--text-muted)] font-mono">{t("titlebar.worlds", { n: THEMES.length })}</span>
          </div>

          <div class="space-y-1 max-h-72 overflow-y-auto pr-0.5">
            {#each THEMES as t}
              {@const isSelected = currentTheme.id === t.id}
              <button
                type="button"
                class="w-full flex items-center justify-between p-2 rounded-xl text-left transition-all cursor-pointer {isSelected
                  ? 'border shadow-md'
                  : 'hover:bg-white/[0.06] border border-transparent'}"
                style={isSelected ? `background-color: ${t.primaryColor}20; border-color: ${t.primaryColor}80;` : ""}
                onclick={() => selectTheme(t)}
              >
                <div class="flex items-center gap-2.5 min-w-0">
                  <div class="size-6 shrink-0 flex items-center justify-center">
                    <ThemeLogo theme={t} size="sm" class="!size-6" />
                  </div>
                  <div class="min-w-0">
                    <div class="font-bold text-xs text-[var(--text-primary)] truncate">{t.name}</div>
                    <div class="text-[10px] text-[var(--text-muted)] truncate">{t.category}</div>
                  </div>
                </div>

                {#if isSelected}
                  <Check class="size-3.5 shrink-0" style="color: {t.accentColor}" />
                {/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- Window Controls Separator -->
    <div class="h-3.5 w-px bg-[var(--hairline)] mx-1"></div>

    <!-- Window Minimize Button -->
    <button
      type="button"
      class="size-6 rounded-md flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-white/[0.06] active:scale-90 transition-all duration-150 cursor-pointer"
      onclick={handleMinimize}
      title={t("titlebar.minimize")}
      aria-label={t("titlebar.minimize")}
    >
      <Minus class="size-3.5 stroke-[2.5]" />
    </button>

    <!-- Window Maximize / Restore Button -->
    <button
      type="button"
      class="size-6 rounded-md flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-white/[0.06] active:scale-90 transition-all duration-150 cursor-pointer"
      onclick={handleToggleMaximize}
      title={isMaximized ? t("titlebar.restore") : t("titlebar.maximize")}
      aria-label={isMaximized ? t("titlebar.restore") : t("titlebar.maximize")}
    >
      {#if isMaximized}
        <Copy class="size-3 stroke-[2.5]" />
      {:else}
        <Square class="size-3 stroke-[2.5]" />
      {/if}
    </button>

    <!-- Window Close Button -->
    <button
      type="button"
      class="size-6 rounded-md flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--status-danger)] active:scale-90 transition-all duration-150 cursor-pointer"
      onclick={handleClose}
      title={t("titlebar.close")}
      aria-label={t("titlebar.close")}
    >
      <X class="size-3.5 stroke-[2.5]" />
    </button>
  </div>
</header>
