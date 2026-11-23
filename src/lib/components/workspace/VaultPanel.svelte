<script lang="ts">
  /**
   * The local credential vault.
   *
   * List responses carry metadata only. A secret is fetched only when the user
   * explicitly reveals an item, and the revealed value is cleared automatically.
   */
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "$lib/i18n";
  import type {
    VaultImportPreview,
    VaultImportResult,
    VaultItem,
  } from "$lib/vault";
  import {
    Copy,
    CreditCard,
    Eye,
    EyeOff,
    KeyRound,
    Loader2,
    Plus,
    StickyNote,
    Trash2,
    Upload,
    X,
  } from "@lucide/svelte";

  let items = $state<VaultItem[]>([]);
  let loading = $state(true);
  let error = $state("");
  let addOpen = $state(false);
  let saving = $state(false);
  let kind = $state<"login" | "card" | "contact">("login");
  let name = $state("");
  let username = $state("");
  let website = $state("");
  let secret = $state("");
  let notice = $state("");
  let revealedId = $state<string | null>(null);
  let revealed = $state("");
  let revealTimer: ReturnType<typeof setTimeout> | null = null;
  let importPath = $state("");
  let importPreview = $state<VaultImportPreview | null>(null);
  let importBusy = $state(false);
  let importing = $state(false);

  async function load() {
    loading = true;
    error = "";
    try {
      items = await invoke<VaultItem[]>("list_vault_items");
    } catch (cause) {
      error = String(cause) || t("vault.loadFailed");
    } finally {
      loading = false;
    }
  }

  function resetForm() {
    name = "";
    username = "";
    website = "";
    secret = "";
    kind = "login";
    addOpen = false;
  }

  async function save() {
    if (!name.trim() || !secret.trim() || saving) {
      error = t("vault.required");
      return;
    }
    saving = true;
    error = "";
    try {
      const digits = secret.replace(/\D/gu, "");
      await invoke("save_vault_item", {
        item: {
          kind,
          name: name.trim(),
          username: username.trim() || null,
          website: website.trim() || null,
          last4: kind === "card" && digits.length >= 4 ? digits.slice(-4) : null,
          secret:
            kind === "login"
              ? { password: secret }
              : kind === "card"
                ? { number: secret }
                : { note: secret },
        },
      });
      notice = t("vault.saved");
      resetForm();
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      saving = false;
    }
  }

  async function chooseImport() {
    if (importBusy) return;
    importBusy = true;
    error = "";
    notice = "";
    importPreview = null;
    importPath = "";
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Vault export", extensions: ["json", "csv", "1pux"] }],
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      importPreview = await invoke<VaultImportPreview>("preview_vault_import", {
        path,
      });
      importPath = path;
    } catch (cause) {
      error = String(cause);
    } finally {
      importBusy = false;
    }
  }

  async function commitImport() {
    if (!importPath || !importPreview || importing) return;
    importing = true;
    error = "";
    notice = "";
    try {
      const result = await invoke<VaultImportResult>("import_vault_items", {
        path: importPath,
      });
      importPreview = null;
      importPath = "";
      notice =
        result.failed > 0
          ? t("vault.importedWithFailures", {
              count: result.imported,
              failed: result.failed,
            })
          : t("vault.imported", { count: result.imported });
      await load();
    } catch (cause) {
      error = String(cause);
    } finally {
      importing = false;
    }
  }

  function cancelImport() {
    importPreview = null;
    importPath = "";
  }

  async function reveal(item: VaultItem) {
    if (revealedId === item.id) {
      hideRevealed();
      return;
    }
    try {
      const value = await invoke<Record<string, unknown>>("reveal_vault_item", { id: item.id });
      revealed = Object.entries(value)
        .map(([key, entry]) => `${key}: ${typeof entry === "string" ? entry : JSON.stringify(entry)}`)
        .join("\n");
      revealedId = item.id;
      if (revealTimer) clearTimeout(revealTimer);
      revealTimer = setTimeout(hideRevealed, 20_000);
    } catch (cause) {
      error = `${t("vault.revealFailed")} ${String(cause)}`;
    }
  }

  function hideRevealed() {
    revealed = "";
    revealedId = null;
    if (revealTimer) clearTimeout(revealTimer);
    revealTimer = null;
  }

  async function copyRevealed() {
    if (!revealed) return;
    try {
      await navigator.clipboard.writeText(revealed);
      notice = t("vault.copied");
    } catch {
      /* clipboard denied */
    }
  }

  async function remove(item: VaultItem) {
    if (!confirm(t("vault.deleteConfirm"))) return;
    try {
      await invoke("delete_vault_item", { id: item.id });
      if (revealedId === item.id) hideRevealed();
      notice = t("vault.deleted");
      await load();
    } catch (cause) {
      error = String(cause);
    }
  }

  function kindLabel(value: string): string {
    if (value === "card") return t("vault.card");
    if (value === "contact") return t("vault.contact");
    return t("vault.login");
  }

  function kindIcon(value: string) {
    if (value === "card") return CreditCard;
    if (value === "contact") return StickyNote;
    return KeyRound;
  }

  onMount(() => void load());
  onDestroy(() => {
    if (revealTimer) clearTimeout(revealTimer);
  });
</script>

<div class="flex min-h-0 flex-1 flex-col bg-[var(--surface-0)]">
  <header class="flex shrink-0 items-start justify-between gap-3 border-b border-[var(--hairline)] px-4 py-3">
    <div class="min-w-0">
      <h2 class="text-sm font-bold text-[var(--text-primary)]">{t("vault.title")}</h2>
      <p class="mt-0.5 text-[11px] leading-4 text-[var(--text-tertiary)]">{t("vault.subtitle")}</p>
    </div>
    <div class="flex shrink-0 items-center gap-1">
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2 text-[11px] font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] cursor-pointer disabled:opacity-50"
        onclick={() => void chooseImport()}
        disabled={importBusy || importing}
        title={t("vault.import")}
      >
        {#if importBusy}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <Upload class="size-3.5" />
        {/if}
        {t("vault.import")}
      </button>
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2 text-[11px] font-semibold text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--hairline-strong)] cursor-pointer"
        onclick={() => (addOpen = !addOpen)}
      >
        {#if addOpen}<X class="size-3.5" />{:else}<Plus class="size-3.5" />{/if}
        {t("vault.add")}
      </button>
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto p-3">
    {#if addOpen}
      <form
        class="mb-3 space-y-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3"
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <div class="flex gap-1 rounded-lg border border-[var(--hairline)] bg-[var(--surface-0)] p-1">
          {#each [["login", t("vault.login")], ["card", t("vault.card")], ["contact", t("vault.contact")]] as entry (entry[0])}
            <button
              type="button"
              class="h-6 flex-1 rounded-md text-[11px] font-semibold cursor-pointer {kind === entry[0]
                ? 'bg-[var(--surface-2)] text-[var(--text-primary)]'
                : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'}"
              onclick={() => (kind = entry[0] as typeof kind)}
            >{entry[1]}</button>
          {/each}
        </div>
        <input
          class="chat-question__input"
          placeholder={t("vault.name")}
          aria-label={t("vault.name")}
          bind:value={name}
          maxlength={120}
          disabled={saving}
        />
        <input
          class="chat-question__input"
          placeholder={kind === "login" ? t("vault.username") : kind === "card" ? t("vault.username") : t("vault.username")}
          aria-label={t("vault.username")}
          bind:value={username}
          disabled={saving}
        />
        <input
          class="chat-question__input"
          placeholder={t("vault.website")}
          aria-label={t("vault.website")}
          bind:value={website}
          disabled={saving}
        />
        <input
          class="chat-question__input"
          type="password"
          placeholder={t("vault.secretHint")}
          aria-label={t("vault.secret")}
          bind:value={secret}
          disabled={saving}
        />
        <div class="flex justify-end gap-2">
          <button type="button" class="browser-secret__cancel" onclick={resetForm}>{t("vault.cancel")}</button>
          <button type="submit" class="browser-secret__submit" disabled={saving}>
            {saving ? t("ui.saving") : t("vault.save")}
          </button>
        </div>
      </form>
    {/if}

    {#if importPreview}
      <section
        class="mb-3 space-y-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3"
        aria-label={t("vault.importTitle")}
      >
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0">
            <p class="text-[11px] font-semibold text-[var(--text-primary)]">
              {t("vault.importReview")} · {importPreview.format}
            </p>
            <p class="mt-0.5 text-[11px] leading-4 text-[var(--text-tertiary)]">
              {t("vault.importHint")}
            </p>
          </div>
          <button
            type="button"
            class="icon-btn size-6 shrink-0"
            onclick={cancelImport}
            disabled={importing}
            title={t("vault.cancel")}
            aria-label={t("vault.cancel")}
          >
            <X class="size-3.5" />
          </button>
        </div>

        <div class="flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-[var(--text-secondary)]">
          <span>{t("vault.importable", { count: importPreview.importable })}</span>
          <span>{t("vault.duplicates", { count: importPreview.duplicates })}</span>
          <span>{t("vault.skipped", { count: importPreview.skipped })}</span>
        </div>

        {#if importPreview.items.length > 0}
          <div class="max-h-40 space-y-1 overflow-y-auto no-scrollbar rounded-lg border border-[var(--hairline)] bg-[var(--surface-0)] p-1.5">
            {#each importPreview.items as item, index (index)}
              <div class="flex min-w-0 items-center gap-2 rounded-md px-1.5 py-1 text-[11px]">
                <span class="shrink-0 rounded border border-[var(--hairline)] bg-[var(--surface-2)] px-1 font-mono text-[11px] text-[var(--text-tertiary)]">
                  {kindLabel(item.kind)}
                </span>
                <span class="min-w-0 flex-1 truncate font-semibold text-[var(--text-primary)]">{item.name}</span>
                {#if item.username}
                  <span class="hidden min-w-0 truncate text-[var(--text-muted)] sm:block">{item.username}</span>
                {:else if item.website}
                  <span class="hidden min-w-0 truncate text-[var(--text-muted)] sm:block">{item.website}</span>
                {/if}
                {#if item.duplicate}
                  <span class="shrink-0 rounded border border-[var(--warning-border)] bg-[var(--warning-soft)] px-1 text-[11px] text-[var(--warning-text)]">
                    {t("vault.duplicate")}
                  </span>
                {/if}
              </div>
            {/each}
          </div>
          {#if importPreview.more}
            <p class="text-[11px] text-[var(--text-muted)]">
              {t("vault.importMore", { count: importPreview.items.length })}
            </p>
          {/if}
        {:else}
          <p class="text-[11px] text-[var(--text-muted)]">{t("vault.importEmpty")}</p>
        {/if}

        <div class="flex justify-end gap-2">
          <button type="button" class="browser-secret__cancel" onclick={cancelImport} disabled={importing}>
            {t("vault.cancel")}
          </button>
          <button
            type="button"
            class="browser-secret__submit"
            onclick={() => void commitImport()}
            disabled={importing || importPreview.importable === 0}
          >
            {#if importing}
              <Loader2 class="inline size-3.5 animate-spin" />
              {t("vault.importing")}
            {:else}
              {t("vault.importConfirm", { count: importPreview.importable })}
            {/if}
          </button>
        </div>
      </section>
    {/if}

    {#if error}
      <p class="mb-2 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger-text)]" role="alert">
        {error}
      </p>
    {/if}
    {#if notice}
      <p class="mb-2 rounded-lg border border-[var(--success-text)]/30 bg-[var(--success-soft)] px-3 py-2 text-[11px] text-[var(--success-text)]" role="status">
        {notice}
      </p>
    {/if}

    {#if loading}
      <div class="flex items-center justify-center gap-2 py-12 text-[11px] text-[var(--text-muted)]" role="status">
        <Loader2 class="size-4 animate-spin" />{t("ui.loading")}
      </div>
    {:else if items.length === 0}
      <div class="flex flex-col items-center justify-center gap-2 px-5 py-12 text-center">
        <KeyRound class="size-6 text-[var(--text-muted)]" strokeWidth={1.6} />
        <p class="text-xs font-semibold text-[var(--text-primary)]">{t("vault.empty")}</p>
        <p class="max-w-xs text-[11px] leading-5 text-[var(--text-tertiary)]">{t("vault.emptyHint")}</p>
      </div>
    {:else}
      <div class="space-y-2">
        {#each items as item (item.id)}
          {@const Icon = kindIcon(item.kind)}
          <article class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)]">
            <div class="flex items-start gap-2.5 p-3">
              <span class="flex size-8 shrink-0 items-center justify-center rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] text-[var(--brand-text)]">
                <Icon class="size-4" strokeWidth={1.8} />
              </span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="truncate text-sm font-semibold text-[var(--text-primary)]">{item.name}</span>
                  <span class="shrink-0 rounded-full border border-[var(--hairline)] px-1.5 py-0.5 text-[11px] font-mono text-[var(--text-muted)]">
                    {kindLabel(item.kind)}
                  </span>
                </div>
                <p class="mt-0.5 truncate text-[11px] text-[var(--text-tertiary)]">
                  {item.username || item.website || t("vault.noWebsite")}
                </p>
                {#if item.last4}
                  <p class="mt-0.5 font-mono text-[11px] text-[var(--text-muted)]">{t("vault.cardMasked", { last4: item.last4 })}</p>
                {/if}
              </div>
              <div class="flex shrink-0 gap-0.5">
                <button
                  type="button"
                  class="browser-dock__button"
                  aria-label={revealedId === item.id ? t("vault.hide") : t("vault.reveal")}
                  title={revealedId === item.id ? t("vault.hide") : t("vault.reveal")}
                  onclick={() => void reveal(item)}
                >
                  {#if revealedId === item.id}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
                </button>
                <button
                  type="button"
                  class="browser-dock__button"
                  aria-label={t("vault.delete")}
                  title={t("vault.delete")}
                  onclick={() => void remove(item)}
                >
                  <Trash2 class="size-3.5" />
                </button>
              </div>
            </div>
            {#if revealedId === item.id}
              <div class="border-t border-[var(--hairline)] px-3 py-2">
                <div class="flex items-start gap-2">
                  <pre class="min-w-0 flex-1 whitespace-pre-wrap break-all font-mono text-[11px] leading-5 text-[var(--text-primary)]">{revealed}</pre>
                  <button type="button" class="browser-dock__button shrink-0" aria-label={t("vault.copy")} title={t("vault.copy")} onclick={() => void copyRevealed()}>
                    <Copy class="size-3.5" />
                  </button>
                </div>
              </div>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  </div>
</div>
