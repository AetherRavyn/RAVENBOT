<script lang="ts">
  /**
   * See what the agents have actually produced.
   *
   * The confinement is enforced on the way in — `resolve_path` plus
   * `confined()` — and until now nothing in the app showed the result of it. A
   * user set a folder path, watched agents run, and had no way to find out
   * whether they produced anything, wrote somewhere else, or did nothing at
   * all. Setting a string and never seeing the tree is how a workspace stops
   * feeling like a room and starts feeling like a guess.
   *
   * This is read-only on purpose. An office workspace is where agents work, and
   * a file manager the user can write into is a second, conflicting way for the
   * workspace to be modified — with none of the audit trail. To change
   * something, the file manager button hands the folder to the OS, which is
   * where the user's own tools live and where their intent is clear.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { notify } from "$lib/toast";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { buildTree, fmtBytes, type Entry, type Tree, type TreeNode } from "$lib/workspace/tree";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import {
    FolderOpen, FileText, File, Link2, Eye, EyeOff, RefreshCw,
    ExternalLink, ChevronRight, ChevronDown, Loader2, FolderTree, AlertTriangle,
  } from "@lucide/svelte";

  interface Props {
    /** Folders to offer: an office's, or a single bot's. */
    paths: string[];
    /** Shown above the folder list — the office name, or the agent's name. */
    subject?: string;
    onClose?: () => void;
  }

  let { paths, subject = "", onClose }: Props = $props();

  const realPaths = $derived(paths.map((p) => p.trim()).filter(Boolean));
  let active = $state(0);
  // Clamp rather than reset, so removing a folder cannot leave the browser
  // pointing past the end of the list.
  let current = $derived(realPaths[Math.min(active, Math.max(realPaths.length - 1, 0))] ?? "");

  let tree = $state<Tree | null>(null);
  let loading = $state(false);
  let showHidden = $state(false);
  let selected = $state<Entry | null>(null);
  /** Directories the user has opened. A workspace can be wide, so nothing is
   *  expanded until it is asked for. */
  let expanded = $state<Set<string>>(new Set());
  /** Whether the workspace is laid out as a tree or as one flat list. */
  let flat = $state(false);

  async function load() {
    if (!current) {
      tree = null;
      return;
    }
    loading = true;
    try {
      tree = await invoke<Tree>("browse_workspace", { path: current, showHidden });
      // A selection from a different folder is not a selection any more.
      if (selected && !tree.entries.some((e) => e.path === selected!.path)) selected = null;
    } catch (e) {
      tree = null;
      notify(t("wsb.loadFailed") + String(e), "error");
    } finally {
      loading = false;
    }
  }

  // Reload when the folder or the hidden-files switch changes. Both change what
  // the answer is, not just how it is drawn.
  $effect(() => {
    void current;
    void showHidden;
    void load();
  });

  function toggle(path: string) {
    const next = new Set(expanded);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    expanded = next;
  }

  function openInFileManager() {
    if (!current) return;
    invoke("open_workspace_in_file_manager", { path: current }).catch((e) =>
      notify(t("wsb.openFailed") + String(e), "error"),
    );
  }

  /** Only markdown gets the markdown renderer; everything else is plain text. */
  const isMarkdown = (name: string) => /\.mdx?$/i.test(name);

  const ICON_SIZE = 13;

  /**
   * A tree from a flat list.
   *
   * `browse_workspace` returns a flat list with relative paths because that is
   * what is cheap to build and to keep sorted; the hierarchy is reconstructed
   * here so the depth cap and the entry cap are enforced in one place.
   */
  const treeNodes = $derived.by(() => (tree ? buildTree(tree.entries) : ([] as TreeNode[])));
  /** Kept as the local name the template already uses. */
  type Node = TreeNode;

  /** The flat view, filtered to what is currently open. */
  const visibleFlat = $derived.by(() => {
    if (!tree) return [] as Entry[];
    return tree.entries.filter(
      (e) => !e.isDir || e.path.split("/").slice(0, -1).every((p) => expanded.has(p)),
    );
  });
</script>

<div class="raven-wsb">
  <!-- Which workspace, and the one-line state of it. -->
  <header class="raven-wsb__bar">
    <div class="flex items-center gap-2 min-w-0 flex-1">
      <FolderTree class="size-4 text-[var(--brand-text)] shrink-0" />
      {#if realPaths.length > 1}
        <div class="flex items-center gap-1 min-w-0 overflow-x-auto">
          {#each realPaths as p, i (p)}
            <button
              type="button"
              class="raven-wsb__tab" class:raven-wsb__tab--on={i === active}
              title={p}
              onclick={() => { active = i; selected = null; expanded = new Set(); }}
            >
              <span class="truncate">{p.split("/").filter(Boolean).pop() || p}</span>
            </button>
          {/each}
        </div>
      {:else if current}
        <span class="text-xs font-mono text-[var(--text-secondary)] truncate" title={current}>{current}</span>
      {/if}
    </div>

    <div class="flex items-center gap-1 shrink-0">
      <button
        type="button"
        class="raven-wsb__icon"
        aria-pressed={showHidden}
        title={showHidden ? t("wsb.hideHidden") : t("wsb.showHidden")}
        onclick={() => (showHidden = !showHidden)}
      >
        {#if showHidden}<Eye class="size-3.5" />{:else}<EyeOff class="size-3.5" />{/if}
      </button>
      <button
        type="button"
        class="raven-wsb__icon"
        aria-pressed={flat}
        title={flat ? t("wsb.treeView") : t("wsb.flatView")}
        onclick={() => (flat = !flat)}
      >
        <FolderTree class="size-3.5" />
      </button>
      <button type="button" class="raven-wsb__icon" title={t("wsb.refresh")} onclick={() => void load()}>
        {#if loading}<Loader2 class="size-3.5 animate-spin" />{:else}<RefreshCw class="size-3.5" />{/if}
      </button>
      <Button size="sm" variant="outline" class="h-7 gap-1.5 border-[var(--hairline)] text-[11px]" onclick={openInFileManager}>
        <ExternalLink class="size-3" /> {t("wsb.openInFileManager")}
      </Button>
      {#if onClose}
        <Button size="sm" variant="ghost" class="h-7 text-[11px]" onclick={onClose}>{t("ui.close")}</Button>
      {/if}
    </div>
  </header>

  {#if !realPaths.length}
    <p class="raven-wsb__empty">{t("wsb.noFolders")}</p>
  {:else if loading && !tree}
    <p class="raven-wsb__empty flex items-center gap-2 justify-center">
      <Loader2 class="size-3.5 animate-spin" /> {t("wsb.loading")}
    </p>
  {:else if tree?.missing}
    <!--
      Not created yet. Distinct from empty, and it needs saying plainly: the
      folder appears when an agent first works there, so an empty path here is
      the normal state of a brand new office, not a fault.
    -->
    <div class="raven-wsb__empty space-y-1.5">
      <FolderOpen class="size-5 text-[var(--text-muted)]" />
      <p class="font-medium text-[var(--text-secondary)]">{t("wsb.notCreated")}</p>
      <p class="text-[11px] max-w-sm">{tree.note}</p>
    </div>
  {:else if tree && tree.entries.length === 0}
    <div class="raven-wsb__empty space-y-1.5">
      <FolderOpen class="size-5 text-[var(--text-muted)]" />
      <p class="font-medium text-[var(--text-secondary)]">{t("wsb.empty")}</p>
      <p class="text-[11px] max-w-sm">{showHidden ? "" : t("wsb.emptyHidden")}</p>
    </div>
  {:else if tree}
    <div class="raven-wsb__body">
      <div class="raven-wsb__list" role="tree" aria-label={t("wsb.listLabel")}>
        <!--
          One row renderer, used by both views and at every depth.

          There were two copies before, and the top-level one had an `{#if
          e.isDir}` with no `{:else}` — so a *file* at the workspace root was
          rendered as nothing at all. That is not an edge case: `OFFICE.md`,
          `README.md` and `POLICY.md` are the three files that make a folder an
          office's workspace, they all sit at the root, and none of them appeared.
        -->
        {#snippet rowFor(e: Entry, depth: number)}
          <button
            type="button"
            class="raven-wsb__row"
            class:raven-wsb__row--on={selected?.path === e.path}
            class:font-medium={e.isDir}
            style:padding-left="{8 + depth * 12}px"
            onclick={() => (e.isDir ? toggle(e.path) : (selected = e))}
          >
            {#if e.isDir}
              {#if expanded.has(e.path)}<ChevronDown size={ICON_SIZE} />{:else}<ChevronRight size={ICON_SIZE} />{/if}
            {:else}
              <span class="w-[13px] shrink-0"></span>
            {/if}
            {#if e.isLink}
              <Link2 class="size-3 text-[var(--text-muted)] shrink-0" />
            {:else if e.isDir}
              <FolderOpen class="size-3 text-[var(--brand-text)] shrink-0" />
            {:else if isMarkdown(e.name)}
              <FileText class="size-3 text-[var(--text-secondary)] shrink-0" />
            {:else}
              <File class="size-3 text-[var(--text-muted)] shrink-0" />
            {/if}
            <span class="flex-1 text-left truncate">{e.name}</span>
            {#if !e.isDir}<span class="raven-wsb__size">{fmtBytes(e.sizeBytes)}</span>{/if}
          </button>
        {/snippet}

        <!-- Recursive, because the listing is up to three levels deep and a
             two-level version would silently drop the third. -->
        {#snippet branch(nodes: Node[])}
          {#each nodes as node (node.entry.path)}
            {@render rowFor(node.entry, node.depth)}
            {#if node.entry.isDir && expanded.has(node.entry.path) && node.children.length}
              {@render branch(node.children)}
            {/if}
          {/each}
        {/snippet}

        {#if flat}
          {#each visibleFlat as e (e.path)}
            {@render rowFor(e, e.isDir ? e.path.split("/").length - 1 : 0)}
          {/each}
        {:else}
          {@render branch(treeNodes)}
        {/if}
      </div>

      <div class="raven-wsb__preview">
        {#if selected}
          <div class="raven-wsb__preview-bar">
            <span class="font-mono text-[11px] text-[var(--text-secondary)] truncate flex-1" title={selected.path}>
              {selected.path}
            </span>
            <Badge variant="outline" class="text-[11px] shrink-0">{fmtBytes(selected.sizeBytes)}</Badge>
          </div>
          {#if selected.isLink}
            <p class="raven-wsb__empty text-[11px]">{t("wsb.linkNotRead")}</p>
          {:else if selected.isBinary}
            <p class="raven-wsb__empty text-[11px]">{t("wsb.binaryNotShown")}</p>
          {:else if selected.preview === null}
            <p class="raven-wsb__empty text-[11px]">{t("wsb.tooLarge")}</p>
          {:else if isMarkdown(selected.name)}
            <div class="raven-wsb__md"><MarkdownRenderer content={selected.preview} /></div>
          {:else}
            <pre class="raven-wsb__text">{selected.preview}</pre>
          {/if}
        {:else}
          <p class="raven-wsb__empty">{t("wsb.selectFile")}</p>
        {/if}
      </div>
    </div>

    <footer class="raven-wsb__foot">
      <span class="font-mono text-[11px] text-[var(--text-muted)]">
        {tree.fileCount} {t("wsb.files")} · {tree.dirCount} {t("wsb.folders")} · {fmtBytes(tree.totalBytes)}
      </span>
      {#if tree.truncated}
        <span class="flex items-center gap-1 text-[11px] text-[hsl(var(--warning))]">
          <AlertTriangle class="size-3" /> {tree.note}
        </span>
      {/if}
    </footer>
  {/if}
</div>
