<script lang="ts">
  /**
   * Settings → Skills → Local skills.
   *
   * This is the read/install side of the library. Creation and revision are
   * agent tools on purpose: a skill is a document an agent can author and
   * test, and the UI's job is to show exactly what is installed before anyone
   * replaces it.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { Button } from "$lib/components/ui/button";
  import { t } from "$lib/i18n";
  import {
    AlertTriangle,
    Check,
    Download,
    FileText,
    Loader2,
    RefreshCw,
    Search,
    Sparkles,
    Store,
  } from "@lucide/svelte";

  interface Version {
    version: string;
    hash: string;
    created_at: string;
    note?: string | null;
  }

  interface Summary {
    id: string;
    name: string;
    description: string;
    example_prompt?: string | null;
    latest_version?: string | null;
    versions: Version[];
    installed_version?: string | null;
    installed_modified: boolean;
  }

  interface SkillView {
    id: string;
    name: string;
    description: string;
    example_prompt?: string | null;
    version: string;
    hash: string;
    source: "installed" | "library" | string;
    installed_version?: string | null;
    installed_modified: boolean;
    content: string;
    files: string[];
  }

  interface MarketplaceSkill {
    id: string;
    name: string;
    description: string;
    url: string;
    category: string;
  }

  interface Props {
    onClose?: () => void;
  }

  let { onClose }: Props = $props();

  let skills = $state<Summary[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let view = $state<SkillView | null>(null);
  let viewLoading = $state(false);
  let installError = $state<string | null>(null);
  let installing = $state<string | null>(null);
  let mode = $state<"installed" | "marketplace">("installed");
  let notice = $state<string | null>(null);
  let marketplaceQuery = $state("");
  let marketplaceResults = $state<MarketplaceSkill[]>([]);
  let marketplaceLoading = $state(false);
  let marketplaceError = $state<string | null>(null);
  let installingMarketplace = $state<string | null>(null);
  let marketplaceConflictId = $state<string | null>(null);

  const selected = $derived(skills.find((skill) => skill.id === selectedId) ?? null);

  async function load() {
    loading = true;
    error = null;
    try {
      const result = await invoke<{ skills: Summary[] }>("local_skills_list");
      skills = result.skills ?? [];
      if (!skills.some((skill) => skill.id === selectedId)) {
        selectedId = skills[0]?.id ?? null;
      }
      if (selectedId) await read(selectedId);
      else view = null;
    } catch (cause) {
      error = String(cause);
      skills = [];
      view = null;
    } finally {
      loading = false;
    }
  }

  async function read(id: string, version?: string) {
    selectedId = id;
    viewLoading = true;
    installError = null;
    try {
      view = await invoke<SkillView>("local_skills_read", {
        id,
        version: version ?? null,
      });
    } catch (cause) {
      view = null;
      error = String(cause);
    } finally {
      viewLoading = false;
    }
  }

  async function install(version: string, force = false) {
    if (!view) return;
    installing = version;
    installError = null;
    try {
      view = await invoke<SkillView>("local_skills_install", {
        id: view.id,
        version,
        force,
      });
      const result = await invoke<{ skills: Summary[] }>("local_skills_list");
      skills = result.skills ?? [];
    } catch (cause) {
      installError = String(cause);
    } finally {
      installing = null;
    }
  }

  function modifiedRefusal(): boolean {
    return (installError ?? "").toLowerCase().includes("local modifications");
  }

  function trySkill() {
    if (!view) return;
    const prompt =
      view.example_prompt?.trim() ||
      `Use the local skill "${view.name}" (${view.id}). Read it with read_local_skill and follow it.`;
    window.dispatchEvent(
      new CustomEvent("local-skill-try", { detail: { prompt, id: view.id } }),
    );
    onClose?.();
  }

  /**
   * The preview is the instructions, not the metadata header. The header is
   * already parsed into the title/description/example fields above; rendering
   * the raw `---` block again would read like the document repeats itself.
   */
  function documentBody(content: string): string {
    const normalized = content.replace(/\r\n/g, "\n");
    if (!normalized.startsWith("---\n")) return normalized;
    const end = normalized.indexOf("\n---", 4);
    return end < 0 ? normalized : normalized.slice(end + 4).trimStart();
  }

  function formatDate(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  function showMarketplace() {
    mode = "marketplace";
    notice = null;
    if (marketplaceResults.length === 0 && !marketplaceLoading) {
      void searchMarketplace();
    }
  }

  async function searchMarketplace(event?: SubmitEvent) {
    event?.preventDefault();
    marketplaceLoading = true;
    marketplaceError = null;
    try {
      marketplaceResults = await invoke<MarketplaceSkill[]>(
        "local_skills_marketplace_search",
        { query: marketplaceQuery.trim(), limit: 40 },
      );
    } catch (cause) {
      marketplaceResults = [];
      marketplaceError = `${t("localSkills.marketplaceFailed")} ${String(cause)}`;
    } finally {
      marketplaceLoading = false;
    }
  }

  async function installMarketplace(item: MarketplaceSkill) {
    installingMarketplace = item.id;
    marketplaceError = null;
    const updating = marketplaceConflictId === item.id;
    try {
      const result = await invoke<{ skill: SkillView }>(
        "local_skills_marketplace_install",
        { target: item.id, update: updating },
      );
      marketplaceConflictId = null;
      const installed = result.skill;
      await load();
      if (installed?.id) {
        selectedId = installed.id;
        await read(installed.id);
      }
      notice = t(
        updating ? "localSkills.updatingNotice" : "localSkills.installedNotice",
        { name: installed?.name || installed?.id || item.name },
      );
      mode = "installed";
    } catch (cause) {
      const text = String(cause);
      if (text.includes("update=true")) marketplaceConflictId = item.id;
      marketplaceError = text;
    } finally {
      installingMarketplace = null;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<div class="local-skills">
  <header class="local-skills__header">
    <div class="local-skills__heading">
      <span class="local-skills__mark"><Sparkles class="size-4" /></span>
      <div>
        <h2>{t("localSkills.title")}</h2>
        <p>{t("localSkills.subtitle")}</p>
      </div>
    </div>
    <div class="local-skills__head-actions">
      <div class="local-skills__tabs" role="tablist" aria-label={t("localSkills.title")}>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "installed"}
          class:is-active={mode === "installed"}
          onclick={() => (mode = "installed")}
        >
          {t("localSkills.installedTab")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "marketplace"}
          class:is-active={mode === "marketplace"}
          onclick={showMarketplace}
        >
          <Store class="size-3" />
          {t("localSkills.marketplaceTab")}
        </button>
      </div>
      {#if mode === "installed"}
        <Button variant="outline" size="sm" onclick={() => load()} disabled={loading}>
          <RefreshCw class="size-3.5 {loading ? 'animate-spin' : ''}" />
          {t("localSkills.refresh")}
        </Button>
      {/if}
    </div>
  </header>

  {#if notice}
    <p class="local-skills__notice" role="status"><Check class="size-3.5" /> {notice}</p>
  {/if}

  {#if mode === "marketplace"}
    <div class="local-skills__market">
      <form class="local-skills__market-search" onsubmit={searchMarketplace}>
        <Search class="size-3.5" />
        <input
          bind:value={marketplaceQuery}
          placeholder={t("localSkills.searchMarketplace")}
          aria-label={t("localSkills.searchMarketplace")}
          spellcheck="false"
        />
        <Button type="submit" size="sm" disabled={marketplaceLoading}>
          {#if marketplaceLoading}<Loader2 class="size-3.5 animate-spin" />{:else}<Search class="size-3.5" />{/if}
          {t("localSkills.search")}
        </Button>
      </form>
      <p class="local-skills__market-hint">{t("localSkills.marketplaceHint")}</p>

      {#if marketplaceError}
        <p class="local-skills__error" role="alert">{marketplaceError}</p>
      {/if}

      {#if marketplaceLoading}
        <div class="local-skills__empty" role="status">
          <Loader2 class="size-4 animate-spin" />
          {t("localSkills.loading")}
        </div>
      {:else if marketplaceResults.length === 0}
        <div class="local-skills__empty">
          <Store class="size-5" />
          <p>{t("localSkills.noMarketplaceResults")}</p>
        </div>
      {:else}
        <div class="local-skills__market-list">
          {#each marketplaceResults as item (item.id)}
            <article class="local-skills__market-item">
              <div class="local-skills__market-meta">
                <span class="local-skills__market-category">{item.category || t("localSkills.marketplaceTab")}</span>
                <h3>{item.name}</h3>
                <code>{item.id}</code>
                {#if item.description}<p>{item.description}</p>{/if}
              </div>
              <Button
                size="sm"
                variant="outline"
                disabled={installingMarketplace !== null}
                onclick={() => installMarketplace(item)}
              >
                {#if installingMarketplace === item.id}
                  <Loader2 class="size-3.5 animate-spin" />
                {:else}
                  <Download class="size-3.5" />
                {/if}
                {marketplaceConflictId === item.id
                  ? t("localSkills.updateFromMarketplace")
                  : t("localSkills.installFromMarketplace")}
              </Button>
            </article>
          {/each}
        </div>
      {/if}
    </div>
  {:else}
  {#if error}
    <p class="local-skills__error" role="alert">{error}</p>
  {/if}

  {#if loading && skills.length === 0}
    <div class="local-skills__empty" role="status">
      <Loader2 class="size-4 animate-spin" />
      {t("localSkills.loading")}
    </div>
  {:else if skills.length === 0}
    <div class="local-skills__empty">
      <FileText class="size-5" />
      <p>{t("localSkills.empty")}</p>
    </div>
  {:else}
    <div class="local-skills__grid">
      <nav class="local-skills__list" aria-label={t("localSkills.title")}>
        {#each skills as skill (skill.id)}
          <button
            type="button"
            class="local-skills__item"
            class:is-active={skill.id === selectedId}
            onclick={() => read(skill.id)}
          >
            <span class="local-skills__item-name">{skill.name || skill.id}</span>
            <span class="local-skills__item-id">{skill.id}</span>
            <span class="local-skills__item-meta">
              {#if skill.installed_version}
                <span class="local-skills__chip">
                  <Check class="size-3" />
                  {skill.installed_version}
                </span>
              {/if}
              {#if skill.latest_version && skill.latest_version !== skill.installed_version}
                <span class="local-skills__chip local-skills__chip--update">
                  {t("localSkills.latest")} {skill.latest_version}
                </span>
              {/if}
              {#if skill.installed_modified}
                <span class="local-skills__chip local-skills__chip--warning">
                  {t("localSkills.modified")}
                </span>
              {/if}
            </span>
          </button>
        {/each}
      </nav>

      <section class="local-skills__preview" aria-label={t("localSkills.preview")}>
        {#if viewLoading}
          <div class="local-skills__empty" role="status">
            <Loader2 class="size-4 animate-spin" />
            {t("localSkills.loading")}
          </div>
        {:else if !view}
          <div class="local-skills__empty">
            <FileText class="size-5" />
            <p>{t("localSkills.noneSelected")}</p>
          </div>
        {:else}
          <div class="local-skills__preview-head">
            <div>
              <h3>{view.name || view.id}</h3>
              <p>{view.description || t("localSkills.noDescription")}</p>
            </div>
            <span class="local-skills__source">
              {view.source === "installed"
                ? t("localSkills.installedVersion", { version: view.version })
                : t("localSkills.libraryVersion", { version: view.version })}
            </span>
          </div>

          {#if view.installed_modified}
            <p class="local-skills__warning" role="status">
              <AlertTriangle class="size-3.5" />
              {t("localSkills.forceHint")}
            </p>
          {/if}

          {#if view.example_prompt}
            <div class="local-skills__try">
              <p><strong>{t("localSkills.example")}</strong> {view.example_prompt}</p>
              <Button size="sm" variant="outline" onclick={trySkill}>
                <Sparkles class="size-3.5" />
                {t("localSkills.try")}
              </Button>
            </div>
          {:else}
            <div class="local-skills__try">
              <p>{t("localSkills.tryHint")}</p>
              <Button size="sm" variant="outline" onclick={trySkill}>
                <Sparkles class="size-3.5" />
                {t("localSkills.try")}
              </Button>
            </div>
          {/if}

          {#if view.files.length > 0}
            <div class="local-skills__files">
              <span>{t("localSkills.files")}</span>
              <div>
                {#each view.files as file (file)}
                  <code>{file}</code>
                {/each}
              </div>
            </div>
          {/if}

          <div class="local-skills__versions">
            <span>{t("localSkills.versions")}</span>
            {#each selected?.versions ?? [] as version (version.version)}
              <div class="local-skills__version">
                <div>
                  <strong>{version.version}</strong>
                  {#if version.note}<span class="local-skills__note">{version.note}</span>{/if}
                  <span class="local-skills__date">{formatDate(version.created_at)}</span>
                </div>
                {#if view.installed_version === version.version && !view.installed_modified}
                  <span class="local-skills__chip"><Check class="size-3" /> {t("localSkills.installed")}</span>
                {:else}
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={installing !== null}
                    onclick={() => install(version.version)}
                  >
                    {#if installing === version.version}
                      <Loader2 class="size-3.5 animate-spin" />
                    {:else}
                      <Download class="size-3.5" />
                    {/if}
                    {t("localSkills.install")}
                  </Button>
                {/if}
              </div>
            {/each}
          </div>

          {#if installError}
            <div class="local-skills__install-error" role="alert">
              <p>{installError}</p>
              {#if modifiedRefusal()}
                <Button
                  size="sm"
                  variant="destructive"
                  disabled={installing !== null}
                  onclick={() => install(view!.version, true)}
                >
                  {t("localSkills.replace")}
                </Button>
              {/if}
            </div>
          {/if}

          <div class="local-skills__document">
            <MarkdownRenderer content={documentBody(view.content)} />
          </div>
        {/if}
      </section>
    </div>
  {/if}
  {/if}
</div>
