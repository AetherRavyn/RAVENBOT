<script lang="ts">
  /**
   * The right-hand workspace panel: Browser, Files, and the attention inbox.
   *
   * BrowserDock stays mounted even when another tab is showing, because it is
   * the listener for an agent's `browser-dock:request`. The aside itself is
   * hidden while closed, so the native webview has no bounds and stays out of
   * the way until someone asks for it.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { workspace } from "$lib/workspace.svelte";
  import { sidePanel } from "$lib/workspace/sidePanel.svelte";
  import { browserDock } from "$lib/browserDock.svelte";
  import BrowserDock from "$lib/components/workspace/BrowserDock.svelte";
  import WorkspaceBrowser from "$lib/components/workspace/WorkspaceBrowser.svelte";
  import AttentionInbox from "$lib/components/workspace/AttentionInbox.svelte";
  import ArtifactPanel from "$lib/components/ArtifactPanel.svelte";
  import VaultPanel from "$lib/components/workspace/VaultPanel.svelte";
  import { artifactStore } from "$lib/artifacts.svelte";
  import { Code2, FolderTree, Globe, Inbox, KeyRound, X } from "@lucide/svelte";

  interface Props {
    bots?: any[];
    /** Narrow windows render the panel above the app instead of in a column. */
    overlay?: boolean;
  }

  let { bots = [], overlay = false }: Props = $props();

  let scope = $derived(workspace.selectedRoomId ?? workspace.selectedBotId ?? "home");
  let filePaths = $state<string[]>([]);
  let fileSubject = $state("");
  let pathSeq = 0;

  async function resolvePaths(seq: number) {
    const room = workspace.selectedRoom;
    const bot = workspace.selectedBot;
    if (room) {
      let paths: string[] = Array.isArray(room.project_folders)
        ? room.project_folders.filter((path: unknown) => typeof path === "string" && path.trim())
        : [];
      if (paths.length === 0) {
        const fallback = await invoke<string>("default_workspace_for", { name: room.name }).catch(() => "");
        if (fallback.trim()) paths = [fallback];
      }
      if (seq !== pathSeq) return;
      filePaths = paths;
      fileSubject = room.name;
      return;
    }
    if (bot) {
      let paths: string[] = [];
      const configured = bot.config?.working_folder;
      if (typeof configured === "string" && configured.trim()) paths = [configured];
      if (paths.length === 0) {
        const fallback = await invoke<string>("default_workspace_for", { name: bot.name }).catch(() => "");
        if (fallback.trim()) paths = [fallback];
      }
      if (seq !== pathSeq) return;
      filePaths = paths;
      fileSubject = bot.name;
      return;
    }
    if (seq !== pathSeq) return;
    filePaths = [];
    fileSubject = "";
  }

  $effect(() => {
    // Read the selection so the effect reruns when the scope changes.
    void workspace.selectedRoomId;
    void workspace.selectedBotId;
    const seq = ++pathSeq;
    void resolvePaths(seq);
  });

  $effect(() => {
    sidePanel.restore(scope);
  });

  $effect(() => {
    // The browser's persistent store follows the selected scope. Switching
    // agents should not silently expose the previous agent's logged-in sites.
    const profile = workspace.selectedRoomId
      ? `office-${workspace.selectedRoomId}`
      : workspace.selectedBotId
        ? `agent-${workspace.selectedBotId}`
        : "shared";
    browserDock.setScope(scope, profile);
  });

  $effect(() => {
    // Read the store state directly so the effect is re-run when an agent
    // opens the dock. Calling the method alone hides the dependency behind a
    // class boundary in some compiler versions.
    if (browserDock.open) sidePanel.revealBrowser();
  });

  // The native page is hidden by BrowserDock's own open/close effect. A hide
  // effect here would race the reveal effect (the panel opens one tick after
  // the dock), so the dock owns that transition.

  const tabs = [
    { id: "browser" as const, label: "sidePanel.browser", icon: Globe },
    { id: "files" as const, label: "sidePanel.files", icon: FolderTree },
    { id: "inbox" as const, label: "sidePanel.inbox", icon: Inbox },
    { id: "artifacts" as const, label: "sidePanel.artifacts", icon: Code2 },
    { id: "vault" as const, label: "sidePanel.vault", icon: KeyRound },
  ] as const;
</script>

<aside
  class="side-panel"
  class:side-panel--overlay={overlay}
  hidden={!sidePanel.open}
  aria-label={t("sidePanel.ariaLabel")}
>
  <header class="side-panel__bar">
    <div class="side-panel__tabs" role="group" aria-label={t("sidePanel.ariaLabel")}>
      {#each tabs as tab (tab.id)}
        {@const Icon = tab.icon}
        <button
          type="button"
          class="side-panel__tab"
          class:side-panel__tab--active={sidePanel.active === tab.id}
          aria-pressed={sidePanel.active === tab.id}
          onclick={() => sidePanel.select(tab.id, scope)}
          title={t(tab.label)}
        >
          <Icon class="size-3.5" strokeWidth={1.8} />
          <span>{t(tab.label)}</span>
        </button>
      {/each}
    </div>
    <button
      type="button"
      class="side-panel__close"
      aria-label={t("sidePanel.close")}
      title={t("sidePanel.close")}
      onclick={() => sidePanel.close(scope)}
    >
      <X class="size-3.5" />
    </button>
  </header>

  <div class="side-panel__body">
    <div class="side-panel__pane" class:is-hidden={sidePanel.active !== "browser"}>
      <BrowserDock {bots} onClose={() => sidePanel.close(scope)} />
    </div>
    {#if sidePanel.active === "files"}
      <div class="side-panel__pane side-panel__files">
        <WorkspaceBrowser
          paths={filePaths}
          subject={fileSubject}
          onClose={() => sidePanel.close(scope)}
        />
      </div>
    {:else if sidePanel.active === "inbox"}
      <div class="side-panel__pane">
        <AttentionInbox />
      </div>
    {:else if sidePanel.active === "vault"}
      <div class="side-panel__pane">
        <VaultPanel />
      </div>
    {:else if sidePanel.active === "artifacts"}
      <div class="side-panel__pane">
        {#if artifactStore.current}
          <ArtifactPanel
            artifact={artifactStore.current}
            onClose={() => artifactStore.close()}
          />
        {:else}
          <div class="side-panel__empty">
            <Code2 class="size-6 text-[var(--text-muted)]" strokeWidth={1.6} />
            <p>{t("artifact.empty")}</p>
            {#if artifactStore.history.length > 0}
              <div class="side-panel__recent">
                <span>{t("artifact.recent")}</span>
                {#each artifactStore.history as item (item.id)}
                  <button type="button" onclick={() => artifactStore.select(item)}>
                    <span>{item.title}</span>
                    <small>{item.language}</small>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</aside>
