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
   * The viewer is a real code view: syntax highlighting, line numbers, and a
   * Markdown preview for prose. The editor is deliberately narrow rather than a
   * second filesystem: it writes only inside a configured office/agent folder,
   * hashes what it opened, and refuses to silently overwrite an agent's newer
   * write. That is what makes editing here safe instead of a race nobody can see.
   */
  import { onMount, tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { notify } from "$lib/toast";
  import { workspace } from "$lib/workspace.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import DiffView from "$lib/components/workspace/DiffView.svelte";
  import { buildTree, fmtBytes, type Entry, type Tree, type TreeNode } from "$lib/workspace/tree";
  import { hl, hlLanguage } from "$lib/highlight";
  import {
    Folder, FolderOpen, FileText, File, ChevronRight, ChevronLeft, Loader2, RefreshCw,
    Building2, Bot as BotIcon, ExternalLink, GitBranch, AlertTriangle,
    Pencil, Save, Eye, Code2, RotateCcw, X, Check,
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
    /** Absent on purpose: see `file_change_diff`. Fetched when a row opens. */
    diff?: string | null;
    created_at: string;
  }

  interface Totals {
    files: number;
    writes: number;
    lines_added: number;
    lines_deleted: number;
  }

  interface FileMeta {
    sizeBytes: number;
    truncated: boolean;
    /** Version of the bytes the editor started from, for conflict detection. */
    sha256: string | null;
    modifiedMs: number | null;
  }

  interface ReadFileResult {
    text: string;
    sizeBytes: number;
    truncated: boolean;
    sha256: string | null;
    modifiedMs: number | null;
  }

  interface WriteFileResult {
    status: "saved" | "conflict";
    sha256: string | null;
    sizeBytes?: number;
    modifiedMs?: number | null;
    text?: string | null;
    message?: string;
  }

  type MediaKind = "image" | "video" | "audio" | "pdf";

  interface MediaResult {
    mime: string;
    base64: string;
    sizeBytes: number;
  }

  interface OpenFile {
    treeKey: string;
    root: string;
    entry: Entry;
  }

  type ViewMode = "preview" | "source" | "edit";

  /** Why the user cannot edit the open file, or empty when they can. */
  type PendingIntent =
    | { type: "open"; target: OpenFile }
    | { type: "reload" }
    | { type: "back" }
    | { type: "cancel" };

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

  let selected = $state<OpenFile | null>(null);
  let fileText = $state("");
  let fileMeta = $state<FileMeta | null>(null);
  let loadingFile = $state(false);
  let fileError = $state("");
  /** Blob URL for an in-app image/video/audio/PDF preview. */
  let mediaUrl = $state<string | null>(null);
  let mediaKind = $state<MediaKind | null>(null);
  let mediaError = $state("");

  let viewMode = $state<ViewMode>("source");
  let draft = $state("");
  let saving = $state(false);
  let saveError = $state("");
  let justSaved = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let conflict = $state<{
    sha256: string | null;
    text: string | null;
    sizeBytes: number;
    message: string;
  } | null>(null);
  let pendingIntent = $state<PendingIntent | null>(null);

  let editorEl = $state<HTMLTextAreaElement | null>(null);
  let editGutterEl = $state<HTMLElement | null>(null);
  let dialogEl = $state<HTMLElement | null>(null);
  /** Guards against an old read landing after the user picked another file. */
  let fileRequest = 0;

  /** Agent whose changes are listed at the bottom. `null` means all of them. */
  let focusBot = $state<string | null>(null);

  /**
   * The ledger row currently opened into a diff.
   *
   * One at a time, because these are full files' worth of lines: three open
   * rows is three scroll containers competing for the same eye, and the list
   * they came from is the thing the reader was comparing them against.
   */
  let expandedChange = $state<string | null>(null);
  /** `undefined` = not asked for yet, `null` = asked and there is nothing. */
  const diffs = $state<Record<string, string | null | undefined>>({});
  const diffLoading = $state<Record<string, boolean>>({});
  const diffError = $state<Record<string, string>>({});

  onMount(() => {
    void load();
  });

  $effect(() => {
    return () => {
      if (saveTimer) clearTimeout(saveTimer);
    };
  });

  // A reload or window close should not silently drop an editor draft. Tauri's
  // close path may bypass this; where the webview honours it, the user gets a
  // confirmation instead of losing the file.
  $effect(() => {
    if (!dirty) return;
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  });

  // Focus the discard dialog as soon as it opens. Escape closes it; the editor
  // never keeps typing behind an overlay.
  $effect(() => {
    if (pendingIntent) void tick().then(() => dialogEl?.focus());
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
      // A refresh is a request for what is on disk now, so the cached trees
      // cannot be reused. Keeping them would make the reload button update the
      // ledger while a file an agent just created stayed invisible.
      trees = {};
      // A project that was deleted elsewhere must not stay "open" forever.
      const alive = new Set(ps.map((p) => p.id));
      openProjects = new Set([...openProjects].filter((id) => alive.has(id)));
      if (!openProjects.size && ps.length) {
        // Expand the first project rather than landing on an empty list: the
        // whole reason this screen exists is to look at a codebase, and making
        // someone click twice to see their first folder is a coin flip they
        // should not have to make.
        void expand(ps[0]);
      } else {
        for (const project of ps) {
          if (!openProjects.has(project.id)) continue;
          for (const path of project.paths) void loadTree(project, path);
        }
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

  const fullPath = (target: OpenFile) =>
    `${target.root.replace(/\/+$/, "")}/${target.entry.path}`;

  const isMarkdown = (name: string) => /\.mdx?$/i.test(name);

  /** The previewable media type for a file name, or null for raw binary. */
  function mediaKindFor(name: string): MediaKind | null {
    const ext = name.toLowerCase().split(".").pop() ?? "";
    if (["png", "jpg", "jpeg", "gif", "webp", "avif", "svg"].includes(ext)) return "image";
    if (["mp4", "m4v", "webm", "ogv", "mov", "mkv"].includes(ext)) return "video";
    if (["mp3", "wav", "ogg", "m4a", "flac"].includes(ext)) return "audio";
    if (ext === "pdf") return "pdf";
    return null;
  }

  function clearMedia() {
    if (mediaUrl) URL.revokeObjectURL(mediaUrl);
    mediaUrl = null;
    mediaKind = null;
    mediaError = "";
  }

  const markdown = $derived(selected ? isMarkdown(selected.entry.name) : false);

  /**
   * Rendering Markdown is real work (lexing, tables, diagrams), unlike putting
   * text in a `<pre>`. A 4MB document is readable as source; rendering it on
   * open is how a viewer freezes the window, so preview starts off for the
   * largest files.
   */
  const PREVIEW_LIMIT = 512_000;
  const canPreview = $derived(markdown && fileText.length <= PREVIEW_LIMIT);

  /**
   * A language tag for the highlighter, from a file name.
   *
   * `hlLanguage` already knows the extensions; this adds the handful of files
   * that are entirely identified by their name and would otherwise render as
   * grey text (`Dockerfile`, `Makefile`, `.env`).
   */
  function languageFor(name: string): string {
    const base = name.toLowerCase();
    if (base === "dockerfile" || base.startsWith("dockerfile.")) return "docker";
    if (base === "makefile" || base === "gnumakefile") return "sh";
    if (base === ".env" || base.startsWith(".env.")) return "sh";
    const dot = base.lastIndexOf(".");
    const ext = dot >= 0 ? base.slice(dot + 1) : "";
    const known = hlLanguage(ext);
    if (known) return known;
    if (["svelte", "astro", "htm"].includes(ext)) return "markup";
    if (["toml", "ini", "conf", "properties"].includes(ext)) return "yaml";
    return "";
  }

  const codeLang = $derived(selected ? languageFor(selected.entry.name) : "");

  const codeTokens = $derived(
    !fileText || viewMode === "edit" ? [] : hl(fileText, codeLang),
  );

  // A gutter is a single text node, so it stays cheap; the cap only exists so a
  // huge minified file cannot turn one keystroke into a 100k-number join.
  const GUTTER_LIMIT = 200_000;
  function lineNumbers(text: string): string {
    if (!text || text.length > GUTTER_LIMIT) return "";
    const count = text.split("\n").length;
    if (count <= 1) return "1";
    const out = new Array<string>(count);
    for (let i = 1; i <= count; i++) out[i - 1] = String(i);
    return out.join("\n");
  }
  const codeGutter = $derived(lineNumbers(fileText));
  const editorGutter = $derived(lineNumbers(draft));

  const dirty = $derived(viewMode === "edit" && draft !== fileText);

  const editBlocked = $derived(
    !selected
      ? ""
      : selected.entry.isBinary
        ? t("projects.binary")
        : fileMeta?.truncated
          ? t("projects.largeFile")
          : loadingFile
            ? t("ui.loading")
            : fileError
              ? fileError
              : "",
  );

  const canEdit = $derived(Boolean(selected) && !editBlocked);

  async function requestOpen(treeKey: string, root: string, entry: Entry) {
    if (entry.isDir) return;
    const target: OpenFile = { treeKey, root, entry };
    if (dirty) {
      pendingIntent = { type: "open", target };
      return;
    }
    await openFile(target);
  }

  async function openFile(target: OpenFile) {
    const request = ++fileRequest;
    clearMedia();
    selected = target;
    viewMode = target.entry.isBinary ? "source" : isMarkdown(target.entry.name) ? "preview" : "source";
    fileText = "";
    fileMeta = null;
    fileError = "";
    draft = "";
    conflict = null;
    saveError = "";
    justSaved = false;

    // The tree already knows this is binary. Reading it as text would only turn
    // arbitrary bytes into replacement characters; images, video, audio, and
    // PDFs get their own blob preview instead.
    if (target.entry.isBinary) {
      const kind = mediaKindFor(target.entry.name);
      if (!kind) {
        loadingFile = false;
        return;
      }
      mediaKind = kind;
      loadingFile = true;
      try {
        const res = await invoke<MediaResult>("read_workspace_media", {
          path: fullPath(target),
        });
        if (request !== fileRequest) return;
        const binary = atob(res.base64);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
        mediaUrl = URL.createObjectURL(new Blob([bytes], { type: res.mime }));
        fileMeta = {
          sizeBytes: res.sizeBytes,
          truncated: false,
          sha256: null,
          modifiedMs: null,
        };
      } catch (e) {
        if (request !== fileRequest) return;
        mediaError = String(e);
        notify(t("projects.readFailed") + String(e), "error");
      } finally {
        if (request === fileRequest) loadingFile = false;
      }
      return;
    }

    loadingFile = true;
    try {
      const res = await invoke<ReadFileResult>("read_workspace_file", {
        path: fullPath(target),
      });
      if (request !== fileRequest) return;
      fileText = res.text;
      if (viewMode === "preview" && fileText.length > PREVIEW_LIMIT) {
        viewMode = "source";
      }
      fileMeta = {
        sizeBytes: res.sizeBytes,
        truncated: res.truncated,
        sha256: res.sha256 ?? null,
        modifiedMs: res.modifiedMs ?? null,
      };
    } catch (e) {
      if (request !== fileRequest) return;
      fileError = String(e);
      notify(t("projects.readFailed") + String(e), "error");
    } finally {
      if (request === fileRequest) loadingFile = false;
    }
  }

  /** Re-read the open file, optionally keeping the current view mode. */
  async function reloadFile() {
    if (!selected) return;
    const target: OpenFile = { ...selected };
    const mode = viewMode === "edit" ? "source" : viewMode;
    await openFile(target);
    const stillOpen =
      selected?.treeKey === target.treeKey &&
      selected?.entry.path === target.entry.path;
    if (stillOpen && !fileError) viewMode = mode;
  }

  function reload() {
    if (!selected) return;
    if (dirty) {
      pendingIntent = { type: "reload" };
      return;
    }
    void reloadFile();
  }

  async function startEdit() {
    if (!canEdit) return;
    draft = fileText;
    conflict = null;
    saveError = "";
    justSaved = false;
    viewMode = "edit";
    await tick();
    editorEl?.focus();
    if (editorEl) editorEl.selectionStart = editorEl.selectionEnd = 0;
  }

  function cancelEditNow() {
    draft = fileText;
    conflict = null;
    saveError = "";
    viewMode = markdown ? "preview" : "source";
    pendingIntent = null;
  }

  function requestCancel() {
    if (dirty) {
      pendingIntent = { type: "cancel" };
      return;
    }
    cancelEditNow();
  }

  function requestBack() {
    if (dirty) {
      pendingIntent = { type: "back" };
      return;
    }
    closeFile();
  }

  function closeFile() {
    fileRequest += 1;
    clearMedia();
    selected = null;
    fileText = "";
    fileMeta = null;
    fileError = "";
    draft = "";
    conflict = null;
    saveError = "";
    viewMode = "source";
    pendingIntent = null;
  }

  /**
   * Save the draft.
   *
   * `force` is only used by the conflict banner's explicit overwrite. The
   * normal path sends the hash it read, so an agent writing the file while the
   * user types produces a conflict instead of a silent last-writer-wins.
   */
  async function save(force = false): Promise<boolean> {
    if (!selected || saving) return false;
    if (!force && !dirty) return true;

    const target: OpenFile = { ...selected };
    const content = draft;
    const expected = force ? null : (fileMeta?.sha256 ?? null);
    saving = true;
    saveError = "";
    try {
      const res = await invoke<WriteFileResult>("write_workspace_file", {
        path: fullPath(target),
        content,
        expectedSha256: expected,
        force,
      });
      if (res.status === "conflict") {
        conflict = {
          sha256: res.sha256 ?? null,
          text: res.text ?? null,
          sizeBytes: res.sizeBytes ?? 0,
          message: res.message ?? "",
        };
        return false;
      }

      // The user may have navigated while the write was in flight. The file is
      // saved either way; only the open view should reflect it.
      if (
        selected?.treeKey === target.treeKey &&
        selected?.entry.path === target.entry.path
      ) {
        fileText = content;
        fileMeta = {
          sizeBytes: res.sizeBytes ?? new Blob([content]).size,
          truncated: false,
          sha256: res.sha256 ?? null,
          modifiedMs: res.modifiedMs ?? null,
        };
        conflict = null;
        justSaved = true;
        if (saveTimer) clearTimeout(saveTimer);
        saveTimer = setTimeout(() => (justSaved = false), 3000);
      }
      const size = res.sizeBytes ?? new Blob([content]).size;
      updateTreeSize(target.treeKey, target.entry.path, size);
      return true;
    } catch (e) {
      saveError = String(e);
      notify(t("projects.saveFailed") + String(e), "error");
      return false;
    } finally {
      saving = false;
    }
  }

  function updateTreeSize(key: string, path: string, size: number) {
    const tree = trees[key];
    if (!tree) return;
    trees = {
      ...trees,
      [key]: {
        ...tree,
        entries: tree.entries.map((entry) =>
          entry.path === path ? { ...entry, sizeBytes: size } : entry,
        ),
      },
    };
  }

  /** Adopt the version currently on disk, discarding the local draft. */
  function reloadConflict() {
    if (!conflict) return;
    if (conflict.text === null) {
      void reloadFile();
      return;
    }
    fileText = conflict.text;
    draft = conflict.text;
    if (fileMeta) {
      fileMeta = {
        ...fileMeta,
        sha256: conflict.sha256,
        sizeBytes: conflict.sizeBytes,
        truncated: false,
      };
    }
    conflict = null;
    saveError = "";
    void tick().then(() => editorEl?.focus());
  }

  async function overwriteConflict() {
    conflict = null;
    await save(true);
  }

  async function resolveIntent(choice: "discard" | "save") {
    const intent = pendingIntent;
    if (!intent) return;
    if (choice === "save") {
      const ok = await save();
      if (!ok) {
        // Surface the conflict or the error instead of leaving the overlay in
        // front of the banner that explains why the save did not happen.
        dismissPending();
        return;
      }
    }
    pendingIntent = null;
    if (intent.type === "open") await openFile(intent.target);
    else if (intent.type === "reload") await reloadFile();
    else if (intent.type === "back") closeFile();
    else cancelEditNow();
  }

  /** Close the dialog and put the caret back where the user left it. */
  function dismissPending() {
    pendingIntent = null;
    if (viewMode === "edit") void tick().then(() => editorEl?.focus());
  }

  function onEditorKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      requestCancel();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save();
      return;
    }
    if (e.key === "Tab") {
      e.preventDefault();
      indentSelection(e.shiftKey);
    }
  }

  /**
   * Tab inserts two spaces, Shift+Tab removes up to two.
   *
   * Multi-line selections indent every selected line, which is the difference
   * between an editor that is usable and a text box that fights you. The
   * selection is adjusted around the inserted characters so the same block
   * stays selected.
   */
  function indentSelection(outdent: boolean) {
    const el = editorEl;
    if (!el) return;
    const start = el.selectionStart ?? 0;
    const end = el.selectionEnd ?? start;
    const lineStart = draft.lastIndexOf("\n", Math.max(0, start - 1)) + 1;

    if (start === end && !outdent) {
      draft = draft.slice(0, start) + "  " + draft.slice(end);
      void tick().then(() => {
        el.selectionStart = el.selectionEnd = start + 2;
      });
      return;
    }

    const block = draft.slice(lineStart, end);
    const lines = block.split("\n");
    let removedFirst = 0;
    let removedTotal = 0;
    const changed = lines.map((line, index) => {
      if (!outdent) {
        removedTotal -= 2;
        if (index === 0) removedFirst = -2;
        return `  ${line}`;
      }
      const remove = line.startsWith("  ") ? 2 : line.startsWith(" ") ? 1 : 0;
      removedTotal += remove;
      if (index === 0) removedFirst = remove;
      return line.slice(remove);
    });
    if (outdent && removedTotal === 0) return;

    draft = draft.slice(0, lineStart) + changed.join("\n") + draft.slice(end);
    const nextStart = Math.max(lineStart, start - removedFirst);
    const nextEnd = Math.max(nextStart, end - removedTotal);
    void tick().then(() => {
      el.selectionStart = nextStart;
      el.selectionEnd = nextEnd;
    });
  }

  function syncEditorScroll() {
    if (editGutterEl && editorEl) editGutterEl.scrollTop = editorEl.scrollTop;
  }

  function onWindowKeydown(e: KeyboardEvent) {
    // The editor also handles Escape. It prevents the default action so this
    // handler does not immediately dismiss the dialog that key just opened.
    if (e.defaultPrevented) return;
    if (!pendingIntent) return;
    if (e.key === "Escape") {
      e.preventDefault();
      dismissPending();
    }
  }

  function revealInFileManager(path: string) {
    void invoke("open_workspace_in_file_manager", { path }).catch(() => undefined);
  }

  /**
   * Open a ledger row into the lines it counts.
   *
   * The counts travel with the list; the lines do not, because five hundred
   * writes each carrying a quarter-megabyte diff is 128MB pushed through IPC
   * to fill a badge two digits wide. So the diff is asked for the first time
   * its row opens and kept afterwards — a reader flipping back and forth
   * between two writes should not pay for it twice, and the second answer
   * cannot have changed in the meantime: the row records a write that is
   * already over.
   */
  async function toggleChange(id: string) {
    if (expandedChange === id) {
      expandedChange = null;
      return;
    }
    expandedChange = id;
    if (diffs[id] !== undefined || diffLoading[id]) return;

    diffLoading[id] = true;
    try {
      diffs[id] = await invoke<string | null>("file_change_diff", { id });
    } catch (e) {
      diffError[id] = String(e);
      // Cached as "asked and failed" rather than left undefined, so a failed
      // row does not refetch on every click and repeat the same error.
      diffs[id] = null;
    } finally {
      diffLoading[id] = false;
    }
  }

  function toggleDir(treeKey: string, path: string) {
    const id = `${treeKey}|${path}`;
    const next = new Set(openDirs);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    openDirs = next;
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
    // Depend on the selection only. The folders this opens are derived state,
    // and reading them back here would make the effect retrigger itself.
    if (!selected) return;
    untrack(revealSelection);
  });
</script>

<svelte:window onkeydown={onWindowKeydown} />

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
      onclick={() => (n.entry.isDir ? toggleDir(key, n.entry.path) : requestOpen(key, root, n.entry))}
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

  <div class="proj__body" class:proj__body--file={!!selected}>
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
          <button
            type="button"
            class="proj__back"
            title={t("projects.back")}
            aria-label={t("projects.back")}
            onclick={requestBack}
          >
            <ChevronLeft class="size-4" strokeWidth={2} />
          </button>

          <div class="proj__filetitle">
            {#if markdown}
              <FileText class="size-3.5 shrink-0" strokeWidth={1.75} />
            {:else}
              <File class="size-3.5 shrink-0" strokeWidth={1.75} />
            {/if}
            <span class="proj__filepath">{selected.entry.path}</span>
            {#if dirty}
              <span class="proj__dirty" title={t("projects.unsaved")} aria-label={t("projects.unsaved")}></span>
            {/if}
            {#if justSaved}
              <span class="proj__saved">
                <Check class="size-3" strokeWidth={2.5} />
                {t("projects.saved")}
              </span>
            {/if}
            {#if fileMeta}
              <span class="proj__size">{fmtBytes(fileMeta.sizeBytes)}</span>
            {/if}
            {@render badge(statsFor(selected.root, selected.entry.path))}
          </div>

          <div class="proj__filetools">
            {#if viewMode === "edit"}
              <span class="proj__editing">{t("projects.editing")}</span>
              <button
                type="button"
                class="proj__action proj__action--primary"
                disabled={!dirty || saving}
                onclick={() => void save()}
              >
                {#if saving}
                  <Loader2 class="size-3.5 animate-spin" strokeWidth={2} />
                {:else}
                  <Save class="size-3.5" strokeWidth={2} />
                {/if}
                {t("projects.save")}
              </button>
              <button type="button" class="proj__action" onclick={requestCancel}>
                <X class="size-3.5" strokeWidth={2} />
                {t("projects.cancel")}
              </button>
            {:else}
              {#if !selected.entry.isBinary}
                <div class="proj__modes" role="group" aria-label={t("projects.viewLabel")}>
                  {#if canPreview}
                    <button
                      type="button"
                      class="proj__mode"
                      class:proj__mode--on={viewMode === "preview"}
                      onclick={() => (viewMode = "preview")}
                    >
                      <Eye class="size-3" strokeWidth={2} />
                      {t("projects.preview")}
                    </button>
                  {/if}
                  <button
                    type="button"
                    class="proj__mode"
                    class:proj__mode--on={viewMode === "source"}
                    onclick={() => (viewMode = "source")}
                  >
                    <Code2 class="size-3" strokeWidth={2} />
                    {t("projects.source")}
                  </button>
                </div>
              {/if}
              <button
                type="button"
                class="proj__iconbtn"
                disabled={!canEdit}
                title={editBlocked || t("projects.edit")}
                aria-label={t("projects.edit")}
                onclick={() => void startEdit()}
              >
                <Pencil class="size-3.5" strokeWidth={2} />
              </button>
              <button
                type="button"
                class="proj__iconbtn"
                title={t("projects.reload")}
                aria-label={t("projects.reload")}
                onclick={reload}
              >
                <RotateCcw class="size-3.5" strokeWidth={2} />
              </button>
              <button
                type="button"
                class="proj__iconbtn"
                title={t("projects.openExternal")}
                aria-label={t("projects.openExternal")}
                onclick={() => revealInFileManager(viewRoot)}
              >
                <ExternalLink class="size-3.5" strokeWidth={1.75} />
              </button>
            {/if}
          </div>
        </div>

        {#if conflict}
          <div class="proj__banner proj__banner--conflict" role="alert">
            <AlertTriangle class="proj__bannericon size-4 shrink-0" strokeWidth={1.75} />
            <div class="proj__bannerbody">
              <strong>{t("projects.conflictTitle")}</strong>
              <span class="proj__bannertext">
                {conflict.text === null ? t("projects.conflictDeleted") : t("projects.conflictBody")}
              </span>
            </div>
            <div class="proj__banneractions">
              {#if conflict.text !== null}
                <button type="button" class="proj__action" onclick={reloadConflict}>
                  <RotateCcw class="size-3.5" strokeWidth={2} />
                  {t("projects.conflictReload")}
                </button>
              {/if}
              <button type="button" class="proj__action" onclick={() => void overwriteConflict()}>
                <Save class="size-3.5" strokeWidth={2} />
                {t("projects.conflictOverwrite")}
              </button>
            </div>
          </div>
        {/if}

        {#if saveError}
          <div class="proj__banner proj__banner--error" role="alert">
            <AlertTriangle class="proj__bannericon size-4 shrink-0" strokeWidth={1.75} />
            <div class="proj__bannerbody">
              <strong>{t("projects.saveFailed")}</strong>
              <span class="proj__bannertext">{saveError}</span>
            </div>
            <div class="proj__banneractions">
              <button type="button" class="proj__action" onclick={() => (saveError = "")}>
                {t("ui.dismiss")}
              </button>
            </div>
          </div>
        {/if}

        {#if loadingFile}
          <div class="proj__empty"><Loader2 class="size-4 animate-spin" /></div>
        {:else if fileError}
          <div class="proj__empty proj__empty--file">
            <AlertTriangle class="size-4" strokeWidth={1.75} />
            <p>{t("projects.readFailed")}</p>
            <p class="proj__hint">{fileError}</p>
            <button type="button" class="proj__action" onclick={reload}>
              <RotateCcw class="size-3.5" strokeWidth={2} />
              {t("projects.readAgain")}
            </button>
          </div>
        {:else if selected.entry.isBinary}
          {#if loadingFile}
            <div class="proj__empty proj__empty--file">
              <Loader2 class="size-4 animate-spin" />
              <p>{t("ui.loading")}</p>
            </div>
          {:else if mediaUrl && mediaKind === "video"}
            <div class="proj__filebody proj__filebody--media">
              <!-- svelte-ignore a11y_media_has_caption -->
              <video class="proj__media" src={mediaUrl} controls playsinline preload="metadata"></video>
            </div>
          {:else if mediaUrl && mediaKind === "audio"}
            <div class="proj__filebody proj__filebody--media">
              <audio class="proj__media proj__media--audio" src={mediaUrl} controls preload="metadata"></audio>
            </div>
          {:else if mediaUrl && mediaKind === "image"}
            <div class="proj__filebody proj__filebody--media">
              <img class="proj__media" src={mediaUrl} alt={selected.entry.name} />
            </div>
          {:else if mediaUrl && mediaKind === "pdf"}
            <iframe class="proj__media proj__media--pdf" src={mediaUrl} title={selected.entry.name}></iframe>
          {:else}
            <div class="proj__empty proj__empty--file">
              <File class="size-4" strokeWidth={1.75} />
              <p>{t("projects.binary")}</p>
              {#if mediaError}<p class="proj__hint">{mediaError}</p>{/if}
              <button type="button" class="proj__action" onclick={() => revealInFileManager(viewRoot)}>
                <ExternalLink class="size-3.5" strokeWidth={1.75} />
                {t("projects.openExternal")}
              </button>
            </div>
          {/if}
        {:else if viewMode === "edit"}
          <div class="proj__editwrap">
            {#if editorGutter}
              <!-- One text node of numbers, scrolled in step with the textarea. -->
              <pre class="proj__gutter proj__gutter--edit" bind:this={editGutterEl} aria-hidden="true">{editorGutter}</pre>
            {/if}
            <textarea
              class="proj__editor"
              bind:this={editorEl}
              bind:value={draft}
              aria-label={t("projects.editorLabel")}
              spellcheck="false"
              autocomplete="off"
              autocapitalize="off"
              wrap="off"
              onkeydown={onEditorKeydown}
              onscroll={syncEditorScroll}
            ></textarea>
          </div>
        {:else if !fileText}
          <div class="proj__empty proj__empty--file">
            <FileText class="size-4" strokeWidth={1.75} />
            <p>{t("projects.emptyFile")}</p>
            {#if canEdit}
              <button type="button" class="proj__action" onclick={() => void startEdit()}>
                <Pencil class="size-3.5" strokeWidth={2} />
                {t("projects.edit")}
              </button>
            {/if}
          </div>
        {:else if canPreview && viewMode === "preview"}
          <div class="proj__filebody proj__filebody--md">
            <MarkdownRenderer content={fileText} />
          </div>
        {:else}
          <div class="proj__filebody proj__filebody--code">
            {#if fileMeta?.truncated}
              <p class="proj__hint proj__hint--pad">{t("projects.truncated")}</p>
            {/if}
            <div class="proj__code">
              {#if codeGutter}
                <pre class="proj__gutter" aria-hidden="true">{codeGutter}</pre>
              {/if}
              <pre class="proj__pre"><code>{#each codeTokens as tok, i (i)}{#if tok.type === "text"}{tok.text}{:else if tok.type === "newline"}{"\n"}{:else}<span class="tok-{tok.type}">{tok.text}</span>{/if}{/each}</code></pre>
            </div>
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
                {@const isOpen = expandedChange === c.id}
                <li class="proj__change" data-skill={c.skill} data-open={isOpen}>
                  <button
                    type="button"
                    class="proj__changehead"
                    aria-expanded={isOpen}
                    onclick={() => void toggleChange(c.id)}
                  >
                    <ChevronRight
                      class="proj__chev size-3.5 shrink-0 {isOpen ? 'proj__chev--open' : ''}"
                      strokeWidth={2}
                    />
                    <span class="proj__changebot">{botName(c.bot_id)}</span>
                    <span class="proj__changepath">{c.path}</span>
                    <span class="proj__diff">
                      <span class="proj__add">+{c.lines_added}</span>
                      <span class="proj__del">−{c.lines_deleted}</span>
                    </span>
                  </button>
                  {#if isOpen}
                    <div class="proj__changediff">
                      {#if diffLoading[c.id]}
                        <div class="proj__empty"><Loader2 class="size-4 animate-spin" /></div>
                      {:else if diffError[c.id]}
                        <p class="proj__hint proj__hint--pad">
                          {t("projects.diffFailed")}{diffError[c.id]}
                        </p>
                      {:else if diffs[c.id]}
                        <DiffView diff={diffs[c.id]!} />
                      {:else}
                        <!--
                          Counts with no lines behind them: a binary rewrite, a
                          file past the diff cap, or a row from before the
                          column existed. Saying so beats opening a blank pane
                          that reads as a broken renderer.
                        -->
                        <p class="proj__hint proj__hint--pad">{t("projects.noDiff")}</p>
                      {/if}
                    </div>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </section>
  </div>
</div>

{#if pendingIntent}
  <div class="proj__overlay">
    <div
      class="proj__dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="proj-unsaved-title"
      tabindex="-1"
      bind:this={dialogEl}
    >
      <div class="proj__dialoghead">
        <AlertTriangle class="size-4 shrink-0" strokeWidth={1.75} />
        <h2 id="proj-unsaved-title">{t("projects.unsavedTitle")}</h2>
      </div>
      <p>{t("projects.unsavedBody")}</p>
      <div class="proj__dialogactions">
        <button type="button" class="proj__action" onclick={dismissPending}>
          {t("projects.keepEditing")}
        </button>
        <button type="button" class="proj__action" onclick={() => void resolveIntent("discard")}>
          {t("projects.discard")}
        </button>
        <button
          type="button"
          class="proj__action proj__action--primary"
          disabled={saving}
          onclick={() => void resolveIntent("save")}
        >
          {#if saving}
            <Loader2 class="size-3.5 animate-spin" strokeWidth={2} />
          {:else}
            <Save class="size-3.5" strokeWidth={2} />
          {/if}
          {t("projects.saveAndContinue")}
        </button>
      </div>
    </div>
  </div>
{/if}
