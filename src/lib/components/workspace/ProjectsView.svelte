<script lang="ts">
  /**
   * Projects: the codebases agents work in, and what they did to them.
   *
   * The workspace was always there — agents write into one every run — but the
   * only way to look at it was a browser buried in a settings screen, and the
   * only record of what changed was a tool line in the browser that vanished on
   * reload. So "expand the codebase" and "who changed what" were both things the
   * app could not do, which is an odd state for a thing whose entire job is
   * writing files.
   *
   * Three things on one screen, because they answer one question:
   *
   *  - **the tree**, which is what exists;
   *  - **the viewer**, which is what it says;
   *  - **the change strip**, which is who put it there and by how much.
   *
   * Read-only, for the same reason the settings browser is: an editor inside
   * the app is a second, unaudited way to change a workspace that agents are
   * actively writing into. To edit, the folder opens in the user's own tools.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { notify } from "$lib/toast";
  import { workspace } from "$lib/workspace.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { buildTree, fmtBytes, type Entry, type Tree, type TreeNode } from "$lib/workspace/tree";
  import {
    Folder, FolderOpen, FileText, File, ChevronRight, Loader2, RefreshCw,
    Building2, Bot as BotIcon, ExternalLink, Plus, Minus, GitBranch, AlertTriangle,
  } from "@lucide/svelte";

  interface Project {
    id: string;
    name: string;
    kind: "office" | "agent";
    paths: string[];
  }

  interface FileChange {
    id: string;
    bot_id: string;
    run_id?: string | null;
    thread_id?: string | null;
    path: string;
    skill: string;
    lines_added: number;
    lines_deleted: number;
    created_at: string;
  }

  interface Totals {
    files: number;
    writes: number;
    lines_added: number;
    lines_deleted: number;
  }

  let projects = $state<Project[]>([]);
  let totals = $state<Record<string, Totals>>({});
  let changes = $state<FileChange[]>([]);
  let loading = $state(true);

  /** Trees by `projectIndex:path`, so one project with two folders keeps both. */
  let trees = $state<Record<string, Tree>>({});
  let loadingTree = $state<Record<string, boolean>>({});
  let openProjects = $state<Set<string>>(new Set());
  /** Open directories inside each tree, keyed by `treeKey|dirPath`. */
  let openDirs = $state<Set<string>>(new Set());

  let selected = $state<{ treeKey: string; root: string; entry: Entry } | null>(null);
  let fileText = $state("");
  let fileMeta = $state<{ sizeBytes: number; truncated: boolean } | null>(null);
  let loadingFile = $state(false);

  /** Agent whose changes are listed at the bottom. `null` means all of them. */
  let focusBot = $state<string | null>(null);

  onMount(() => {
    void load();
  });

  async function load() {
    loading = true;
    try {
      const [ps, ts, cs] = await Promise.all([
        invoke<Project[]>("list_projects"),
        invoke<[string, Totals][]>("file_change_totals"),
        invoke<FileChange[]>("list_recent_file_changes", { limit: 500 }),
      ]);
      projects = ps;
      totals = Object.fromEntries(ts);
      changes = cs;
      if (!openProjects.size && ps.length) {
        // Expand the first project rather than landing on an empty list: the
        // whole reason this screen exists is to look at a codebase, and making
        // someone click twice to see their first folder is a coin flip they
        // should not have to make.
        void expand(ps[0]);
      }
    } catch (e) {
      notify(t("projects.loadFailed") + String(e), "error");
    } finally {
      loading = false;
    }
  }

  const treeKey = (p: Project, path: string) => `${p.kind}:${p.id}:${path}`;

  async function expand(project: Project) {
    const next = new Set(openProjects);
    if (next.has(project.id)) {
      next.delete(project.id);
      openProjects = next;
      return;
    }
    next.add(project.id);
    openProjects = next;
    for (const path of project.paths) await loadTree(project, path);
  }

  async function loadTree(project: Project, path: string) {
    const key = treeKey(project, path);
    if (trees[key] || loadingTree[key]) return;
    loadingTree = { ...loadingTree, [key]: true };
    try {
      trees = { ...trees, [key]: await invoke<Tree>("browse_workspace", { path, showHidden: false }) };
    } catch (e) {
      notify(t("projects.treeFailed") + String(e), "error");
    } finally {
      loadingTree = { ...loadingTree, [key]: false };
    }
  }

  async function openFile(treeKey: string, root: string, entry: Entry) {
    if (entry.isDir) return;
    selected = { treeKey, root, entry };
    loadingFile = true;
    fileText = "";
    fileMeta = null;
    try {
      const full = `${root.replace(/\/+$/, "")}/${entry.path}`;
      const res = await invoke<{ text: string; sizeBytes: number; truncated: boolean }>(
        "read_workspace_file",
        { path: full },
      );
      fileText = res.text;
      fileMeta = { sizeBytes: res.sizeBytes, truncated: res.truncated };
    } catch (e) {
      notify(t("projects.readFailed") + String(e), "error");
    } finally {
      loadingFile = false;
    }
  }

  function toggleDir(treeKey: string, path: string) {
    const id = `${treeKey}|${path}`;
    const next = new Set(openDirs);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    openDirs = next;
  }

  /**
   * A change's path, made comparable to a tree entry's.
   *
   * The two are written by different things and neither is wrong: `file_write`
   * reports the absolute path it resolved, `code_edit` reports what the patch
   * said, and a tree entry is relative to the root it was listed from. Stripping
   * the root when it is present is the only step that reconciles all three, and
   * the `./` trim covers the patch case.
   */
  function relativeTo(root: string, path: string): string {
    const normRoot = root.replace(/\/+$/, "");
    if (path.startsWith(`${normRoot}/`)) return path.slice(normRoot.length + 1);
    return path.replace(/^\.\//, "");
  }

  /** Per-path +/− across every agent, for the badges in the tree. */
  const pathStats = $derived.by(() => {
    const map: Record<string, { added: number; deleted: number }> = {};
    for (const c of changes) {
      const cur = map[c.path] ?? { added: 0, deleted: 0 };
      cur.added += c.lines_added;
      cur.deleted += c.lines_deleted;
      map[c.path] = cur;
    }
    return map;
  });

  /** Which tree a change belongs to, so a badge only shows in its own project. */
  function statsFor(root: string, entryPath: string) {
    // Absolute form first — that is what `file_write` writes — then the bare
    // relative form for patches.
    return (
      pathStats[`${root.replace(/\/+$/, "")}/${entryPath}`] ??
      pathStats[entryPath] ??
      null
    );
  }

  /** Changes for whichever agent is focused, or all of them. */
  const shownChanges = $derived(focusBot ? changes.filter((c) => c.bot_id === focusBot) : changes);

  const botName = $derived((id: string) => workspace.bots.find((b: any) => b.id === id)?.name || id.slice(0, 8));

  /** Agents with changes, ordered by how much they changed. */
  const ranked = $derived.by(() => {
    const rows = Object.entries(totals).map(([id, tl]) => ({ id, ...tl }));
    rows.sort(
      (a, b) =>
        b.lines_added + b.lines_deleted - (a.lines_added + a.lines_deleted) ||
        botName(a.id).localeCompare(botName(b.id)),
    );
    return rows;
  });

  const isMarkdown = (name: string) => /\.mdx?$/i.test(name);

  /** Open every directory down to the file the viewer is showing. */
  function revealSelection() {
    if (!selected) return;
    const parts = selected.entry.path.split("/");
    const next = new Set(openDirs);
    for (let i = 1; i < parts.length; i++) {
      next.add(`${selected.treeKey}|${parts.slice(0, i).join("/")}`);
    }
    openDirs = next;
  }
  $effect(() => {
    void selected;
    revealSelection();
  });
</script>

{#snippet badge(stats: { added: number; deleted: number } | null)}
  {#if stats && (stats.added || stats.deleted)}
    <!--
      The git convention, deliberately: green for what was added, red for what
      was removed. Anyone who has used a version control tool reads these
      without being told, and that is worth more than a label — a badge that
      needs a legend is a badge people stop seeing.
    -->
    <span class="proj__diff">
      <span class="proj__add">+{stats.added}</span>
      <span class="proj__del">−{stats.deleted}</span>
    </span>
  {/if}
{/snippet}

{#snippet node(n: TreeNode, root: string, key: string)}
  {@const isOpen = openDirs.has(`${key}|${n.entry.path}`)}
  {@const stats = n.entry.isDir ? null : statsFor(root, n.entry.path)}
  <li>
    <button
      type="button"
      class="proj__row"
      class:proj__row--sel={selected?.treeKey === key && selected?.entry.path === n.entry.path}
      data-dir={n.entry.isDir}
      style={`--depth:${n.depth}`}
      onclick={() => (n.entry.isDir ? toggleDir(key, n.entry.path) : openFile(key, root, n.entry))}
    >
      {#if n.entry.isDir}
        <ChevronRight class="proj__chev size-3.5 shrink-0 {isOpen ? 'proj__chev--open' : ''}" strokeWidth={2} />
        {#if isOpen}<FolderOpen class="proj__icon size-3.5 shrink-0" strokeWidth={1.75} />
        {:else}<Folder class="proj__icon size-3.5 shrink-0" strokeWidth={1.75} />{/if}
      {:else}
        <span class="proj__chev proj__chev--leaf"></span>
        {#if /\.(md|mdx|txt|rst)$/i.test(n.entry.name)}
          <FileText class="proj__icon size-3.5 shrink-0" strokeWidth={1.75} />
        {:else}
          <File class="proj__icon size-3.5 shrink-0" strokeWidth={1.75} />
        {/if}
      {/if}
      <span class="proj__name">{n.entry.name}</span>
      {#if !n.entry.isDir}
        <span class="proj__size">{fmtBytes(n.entry.sizeBytes)}</span>
        {@render badge(stats)}
      {/if}
    </button>
    {#if n.entry.isDir && isOpen && n.children.length}
      <ul class="proj__tree">
        {#each n.children as child (child.entry.path)}
          {@render node(child, root, key)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

<div class="proj">
  <!-- ── Header ─────────────────────────────────────────────────────────── -->
  <header class="proj__head">
    <div class="proj__title">
      <GitBranch class="size-4 shrink-0 text-[var(--brand-text)]" strokeWidth={2} />
      <h1>{t("projects.title")}</h1>
      <span class="proj__count">{t("projects.count", { count: projects.length })}</span>
    </div>
    <button type="button" class="proj__refresh" onclick={load} aria-label={t("projects.refresh")}>
      <RefreshCw class="size-3.5 {loading ? 'animate-spin' : ''}" strokeWidth={2} />
    </button>
  </header>

  <!-- ── Who changed what, by how much ──────────────────────────────────── -->
  {#if ranked.length}
    <section class="proj__agents" aria-label={t("projects.agentsLabel")}>
      <button
        type="button"
        class="proj__chip"
        class:proj__chip--on={focusBot === null}
        onclick={() => (focusBot = null)}
      >
        {t("projects.allAgents")}
      </button>
      {#each ranked as row (row.id)}
        <button
          type="button"
          class="proj__chip"
          class:proj__chip--on={focusBot === row.id}
          onclick={() => (focusBot = focusBot === row.id ? null : row.id)}
        >
          <span class="proj__chipname">{botName(row.id)}</span>
          <span class="proj__add">+{row.lines_added}</span>
          <span class="proj__del">−{row.lines_deleted}</span>
        </button>
      {/each}
    </section>
  {/if}

  <div class="proj__body">
    <!-- ── Projects → codebase ──────────────────────────────────────────── -->
    <aside class="proj__side" aria-label={t("projects.sideLabel")}>
      {#if loading}
        <div class="proj__empty"><Loader2 class="size-4 animate-spin" /></div>
      {:else if !projects.length}
        <div class="proj__empty">
          <AlertTriangle class="size-4" strokeWidth={1.75} />
          <p>{t("projects.none")}</p>
          <p class="proj__hint">{t("projects.noneHint")}</p>
        </div>
      {:else}
        <ul class="proj__list">
          {#each projects as project (project.id)}
            {@const isOpen = openProjects.has(project.id)}
            <li>
              <button
                type="button"
                class="proj__row proj__row--project"
                aria-expanded={isOpen}
                onclick={() => expand(project)}
              >
                <ChevronRight class="proj__chev size-3.5 shrink-0 {isOpen ? 'proj__chev--open' : ''}" strokeWidth={2} />
                {#if project.kind === "office"}
                  <Building2 class="proj__icon size-3.5 shrink-0 text-[var(--brand-text)]" strokeWidth={1.75} />
                {:else}
                  <BotIcon class="proj__icon size-3.5 shrink-0 text-[var(--brand-text)]" strokeWidth={1.75} />
                {/if}
                <span class="proj__name">{project.name}</span>
                <span class="proj__kind">{t(project.kind === "office" ? "projects.office" : "projects.agent")}</span>
              </button>

              {#if isOpen}
                {#each project.paths as path (path)}
                  {@const key = treeKey(project, path)}
                  {@const tree = trees[key]}
                  <div class="proj__folder">
                    <div class="proj__folderhead">
                      {#if loadingTree[key]}
                        <Loader2 class="size-3 animate-spin shrink-0" />
                      {:else}
                        <span class="proj__path">{path}</span>
                      {/if}
                    </div>
                    {#if tree}
                      {#if tree.missing}
                        <p class="proj__hint proj__hint--pad">{t("projects.missing")}</p>
                      {:else if !tree.entries.length}
                        <p class="proj__hint proj__hint--pad">{t("projects.empty")}</p>
                      {:else}
                        <ul class="proj__tree">
                          {#each buildTree(tree.entries) as n (n.entry.path)}
                            {@render node(n, tree.root, key)}
                          {/each}
                        </ul>
                        {#if tree.truncated && tree.note}
                          <p class="proj__hint proj__hint--pad">{tree.note}</p>
                        {/if}
                      {/if}
                    {/if}
                  </div>
                {/each}
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </aside>

    <!-- ── The file, or the ledger ──────────────────────────────────────── -->
    <section class="proj__view" aria-label={t("projects.viewLabel")}>
      {#if selected}
        {@const viewRoot = selected.root}
        <div class="proj__filehead">
          <div class="proj__filetitle">
            <FileText class="size-3.5 shrink-0" strokeWidth={1.75} />
            <span class="proj__filepath">{selected.entry.path}</span>
            {#if fileMeta}
              <span class="proj__size">{fmtBytes(fileMeta.sizeBytes)}</span>
            {/if}
            {@render badge(statsFor(selected.root, selected.entry.path))}
          </div>
          <button
            type="button"
            class="proj__refresh"
            title={t("projects.openExternal")}
            aria-label={t("projects.openExternal")}
            onclick={() => invoke("open_workspace_in_file_manager", { path: viewRoot }).catch(() => undefined)}
          >
            <ExternalLink class="size-3.5" strokeWidth={1.75} />
          </button>
        </div>

        {#if loadingFile}
          <div class="proj__empty"><Loader2 class="size-4 animate-spin" /></div>
        {:else if selected.entry.isBinary}
          <div class="proj__empty">
            <p>{t("projects.binary")}</p>
          </div>
        {:else if fileMeta?.truncated}
          <p class="proj__hint proj__hint--pad">{t("projects.truncated")}</p>
        {/if}

        {#if fileText && !selected.entry.isBinary}
          <div class="proj__filebody">
            {#if isMarkdown(selected.entry.name)}
              <MarkdownRenderer content={fileText} />
            {:else}
              <pre class="proj__pre">{fileText}</pre>
            {/if}
          </div>
        {/if}
      {:else}
        <div class="proj__ledger">
          <h2>
            <GitBranch class="size-4" strokeWidth={2} />
            {t("projects.changesTitle")}
          </h2>
          <p class="proj__hint">{t("projects.changesHint")}</p>

          {#if !shownChanges.length}
            <div class="proj__empty">
              <p>{t("projects.noChanges")}</p>
              <p class="proj__hint">{t("projects.noChangesHint")}</p>
            </div>
          {:else}
            <ul class="proj__changes">
              {#each shownChanges as c (c.id)}
                <li class="proj__change" data-skill={c.skill}>
                  <span class="proj__changebot">{botName(c.bot_id)}</span>
                  <span class="proj__changepath">{c.path}</span>
                  <span class="proj__diff">
                    <span class="proj__add">+{c.lines_added}</span>
                    <span class="proj__del">−{c.lines_deleted}</span>
                  </span>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </section>
  </div>
</div>
