<script lang="ts">
  import { onMount } from "svelte";
  import { THEMES, getStoredTheme, applyTheme, subscribeTheme, type ThemeDefinition } from "$lib/theme";
  import ThemeLogo from "$lib/components/ThemeLogo.svelte";
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
  class="h-8 border-b flex items-center justify-between px-3 select-none z-50 shrink-0 text-xs transition-colors duration-300"
  style="background-color: {currentTheme.bgHex}; border-color: {currentTheme.borderHex};"
>
  <!-- Left Branding & Drag Region -->
  <div data-tauri-drag-region class="flex items-center gap-2">
    {#if onToggleSidebar}
      <button
        type="button"
        class="size-6 rounded-md flex items-center justify-center text-zinc-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
        onclick={onToggleSidebar}
        title="Toggle Sidebar (⌘B)"
      >
        <PanelLeft class="size-3.5" />
      </button>
    {/if}
    <div class="size-4.5 flex items-center justify-center pointer-events-none">
      <ThemeLogo theme={currentTheme} size="sm" class="!size-4.5" />
    </div>
    <span data-tauri-drag-region class="font-bold tracking-wider text-[11px] text-zinc-200">
      {currentTheme.brand.brandTitle}<span style="color: {currentTheme.primaryColor}">{currentTheme.brand.brandAccent}</span>
    </span>
    <span
      data-tauri-drag-region
      class="text-[9px] font-mono uppercase px-1.5 py-0.2 rounded border hidden sm:inline-block"
      style="color: {currentTheme.accentColor}; background-color: {currentTheme.primaryColor}15; border-color: {currentTheme.borderHex};"
    >
      {currentTheme.brand.badgeLabel}
    </span>
  </div>

  <!-- Center Draggable Window Zone -->
  <div data-tauri-drag-region class="flex-1 h-full flex items-center justify-center text-[10px] text-zinc-500 font-mono pointer-events-auto">
    <button
      type="button"
      class="opacity-70 hover:opacity-100 transition-opacity flex items-center gap-1.5 cursor-pointer bg-transparent border-0 text-[10px] text-zinc-400 font-mono py-0.5 px-2 rounded hover:bg-white/5"
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
        class="h-6 px-2 rounded-md flex items-center gap-1.5 text-zinc-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer text-[11px]"
        onclick={(e) => {
          e.stopPropagation();
          showThemeDropdown = !showThemeDropdown;
        }}
        title="Switch UI Theme"
      >
        <span class="size-2.5 rounded-full ring-1 ring-white/30" style="background-color: {currentTheme.primaryColor}"></span>
        <span class="hidden md:inline text-[10px] font-medium text-zinc-300">Theme</span>
      </button>

      <!-- Theme Switcher Popover -->
      {#if showThemeDropdown}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute right-0 top-8 z-50 w-72 border rounded-2xl shadow-2xl p-2.5 space-y-1.5 animate-in fade-in zoom-in-95 backdrop-blur-xl"
          style="background-color: {currentTheme.cardHex}; border-color: {currentTheme.borderHex};"
          onclick={(e) => e.stopPropagation()}
        >
          <div class="flex items-center justify-between pb-1.5 border-b px-1" style="border-color: {currentTheme.borderHex};">
            <span class="text-xs font-bold text-white flex items-center gap-1.5">
              <Palette class="size-3.5" style="color: {currentTheme.primaryColor}" />
              Switch Visual World
            </span>
            <span class="text-[10px] text-zinc-400 font-mono">{THEMES.length} Worlds</span>
          </div>

          <div class="space-y-1 max-h-72 overflow-y-auto pr-0.5">
            {#each THEMES as t}
              {@const isSelected = currentTheme.id === t.id}
              <button
                type="button"
                class="w-full flex items-center justify-between p-2 rounded-xl text-left transition-all cursor-pointer {isSelected
                  ? 'border shadow-md'
                  : 'hover:bg-white/5 border border-transparent'}"
                style={isSelected ? `background-color: ${t.primaryColor}20; border-color: ${t.primaryColor}80;` : ""}
                onclick={() => selectTheme(t)}
              >
                <div class="flex items-center gap-2.5 min-w-0">
                  <div class="size-6 shrink-0 flex items-center justify-center">
                    <ThemeLogo theme={t} size="sm" class="!size-6" />
                  </div>
                  <div class="min-w-0">
                    <div class="font-bold text-xs text-white truncate">{t.name}</div>
                    <div class="text-[10px] text-zinc-400 truncate">{t.category}</div>
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
    <div class="h-3.5 w-px bg-white/10 mx-1"></div>

    <!-- Window Minimize Button -->
    <button
      type="button"
      class="size-6 rounded flex items-center justify-center text-zinc-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
      onclick={handleMinimize}
      title="Minimize Window"
      aria-label="Minimize Window"
    >
      <Minus class="size-3.5 stroke-[2.5]" />
    </button>

    <!-- Window Maximize / Restore Button -->
    <button
      type="button"
      class="size-6 rounded flex items-center justify-center text-zinc-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
      onclick={handleToggleMaximize}
      title={isMaximized ? "Restore Window" : "Maximize Window"}
      aria-label={isMaximized ? "Restore Window" : "Maximize Window"}
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
      class="size-6 rounded flex items-center justify-center text-zinc-400 hover:text-white hover:bg-red-600 transition-colors cursor-pointer"
      onclick={handleClose}
      title="Close Window"
      aria-label="Close Window"
    >
      <X class="size-3.5 stroke-[2.5]" />
    </button>
  </div>
</header>
