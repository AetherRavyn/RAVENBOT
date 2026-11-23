<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import {
    Cookie,
    Globe,
    Loader2,
    Pencil,
    Plus,
    RefreshCw,
    Trash2,
  } from "@lucide/svelte";

  interface BrowserProfile {
    id: string;
    name: string;
    created_at: string;
    updated_at: string;
    cookie_policy: string;
  }

  let profiles = $state<BrowserProfile[]>([]);
  let name = $state("");
  let loading = $state(true);
  let busy = $state(false);
  let error = $state("");
  let notice = $state("");
  let editingId = $state<string | null>(null);
  let editName = $state("");
  let autoRejectCookies = $state(false);
  let cookieBusy = $state(false);
  let cookieError = $state("");
  let policyBusy = $state<string | null>(null);
  const cookiePolicyOptions = $derived([
    { value: "inherit", label: t("browserProfiles.cookieInherit") },
    { value: "always", label: t("browserProfiles.cookieAlways") },
    { value: "never", label: t("browserProfiles.cookieNever") },
  ]);

  function announceChanged() {
    window.dispatchEvent(new CustomEvent("ravenbot:browser-profiles-changed"));
  }

  async function load() {
    loading = true;
    error = "";
    try {
      const [nextProfiles, policy] = await Promise.all([
        invoke<BrowserProfile[]>("list_browser_profiles"),
        invoke<boolean>("get_browser_cookie_policy"),
      ]);
      profiles = nextProfiles;
      autoRejectCookies = policy;
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function toggleCookiePolicy() {
    if (cookieBusy) return;
    cookieBusy = true;
    cookieError = "";
    try {
      autoRejectCookies = await invoke<boolean>("set_browser_cookie_policy", {
        enabled: !autoRejectCookies,
      });
      window.dispatchEvent(
        new CustomEvent("ravenbot:browser-cookie-policy-changed", {
          detail: { enabled: autoRejectCookies },
        }),
      );
    } catch (cause) {
      cookieError = String(cause);
    } finally {
      cookieBusy = false;
    }
  }

  async function setProfilePolicy(
    profile: BrowserProfile,
    policy: string,
  ) {
    if (policyBusy) return;
    policyBusy = profile.id;
    cookieError = "";
    try {
      const saved = await invoke<string>(
        "set_browser_profile_cookie_policy",
        { id: profile.id, policy },
      );
      profiles = profiles.map((entry) =>
        entry.id === profile.id
          ? { ...entry, cookie_policy: saved }
          : entry,
      );
      window.dispatchEvent(
        new CustomEvent("ravenbot:browser-cookie-policy-changed"),
      );
    } catch (cause) {
      cookieError = String(cause);
    } finally {
      policyBusy = null;
    }
  }

  async function create() {
    if (busy || !name.trim()) return;
    busy = true;
    error = "";
    notice = "";
    try {
      await invoke<BrowserProfile>("create_browser_profile", {
        name: name.trim(),
      });
      name = "";
      notice = t("browserProfiles.created");
      await load();
      announceChanged();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  function startRename(profile: BrowserProfile) {
    editingId = profile.id;
    editName = profile.name;
    notice = "";
    error = "";
  }

  async function rename() {
    if (!editingId || busy || !editName.trim()) return;
    busy = true;
    error = "";
    try {
      await invoke<BrowserProfile>("rename_browser_profile", {
        id: editingId,
        name: editName.trim(),
      });
      editingId = null;
      editName = "";
      notice = t("browserProfiles.renamed");
      await load();
      announceChanged();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function remove(profile: BrowserProfile) {
    if (busy) return;
    if (!confirm(t("browserProfiles.deleteConfirm", { name: profile.name }))) {
      return;
    }
    busy = true;
    error = "";
    notice = "";
    try {
      await invoke("delete_browser_profile", { id: profile.id });
      notice = t("browserProfiles.deleted");
      await load();
      announceChanged();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  onMount(() => void load());
</script>

<section class="space-y-5" aria-label={t("browserProfiles.title")}>
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <h3 class="flex items-center gap-2 text-lg font-bold text-[var(--text-primary)]">
        <Globe class="size-5 text-[var(--brand-text)]" />
        {t("browserProfiles.title")}
      </h3>
      <p class="mt-1 max-w-2xl text-sm text-[var(--text-tertiary)]">
        {t("browserProfiles.desc")}
      </p>
    </div>
    <button
      type="button"
      class="icon-btn size-8 border border-[var(--hairline)] bg-[var(--surface-2)]"
      onclick={() => void load()}
      disabled={loading || busy}
      title={t("browserProfiles.refresh")}
      aria-label={t("browserProfiles.refresh")}
    >
      {#if loading}
        <Loader2 class="size-3.5 animate-spin" />
      {:else}
        <RefreshCw class="size-3.5" />
      {/if}
    </button>
  </div>

  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="flex items-center gap-1.5 text-xs font-bold text-[var(--text-primary)]">
          <Cookie class="size-4 text-[var(--brand-text)]" />
          {t("browserProfiles.autoRejectTitle")}
        </p>
        <p class="mt-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
          {t("browserProfiles.autoRejectDesc")}
        </p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={autoRejectCookies}
        aria-label={t("browserProfiles.autoRejectTitle")}
        onclick={() => void toggleCookiePolicy()}
        disabled={cookieBusy}
        class="relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full transition-colors duration-200 disabled:opacity-50 {autoRejectCookies
          ? 'bg-[var(--brand)]'
          : 'bg-[var(--surface-4)]'}"
      >
        <span
          class="pointer-events-none inline-block size-5 transform rounded-full bg-[var(--surface-light)] shadow ring-0 transition duration-200 {autoRejectCookies
            ? 'translate-x-5'
            : 'translate-x-0.5'}"
        ></span>
      </button>
    </div>
    <p class="text-[11px] leading-4 text-[var(--text-muted)]">
      {t("browserProfiles.autoRejectHint")}
    </p>
    {#if cookieError}
      <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
        {cookieError}
      </p>
    {/if}
  </div>

  <div class="rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3">
    <p class="text-[11px] leading-4 text-[var(--text-muted)]">
      {t("browserProfiles.autoNote")}
    </p>
    <p class="text-[11px] leading-4 text-[var(--text-muted)]">
      {t("browserProfiles.cookiePolicyHint")}
    </p>
    <form
      class="flex flex-wrap items-end gap-2"
      onsubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      <label class="min-w-48 flex-1 text-[11px] text-[var(--text-secondary)]">
        <span class="mb-1 block">{t("browserProfiles.name")}</span>
        <input
          class="chat-question__input"
          bind:value={name}
          maxlength={60}
          placeholder={t("browserProfiles.namePlaceholder")}
          disabled={busy}
        />
      </label>
      <button
        type="submit"
        class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
        disabled={busy || !name.trim()}
      >
        {#if busy}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <Plus class="size-3.5" />
        {/if}
        {t("browserProfiles.create")}
      </button>
    </form>

    {#if loading && profiles.length === 0}
      <p class="flex items-center gap-2 text-[11px] text-[var(--text-muted)]" role="status">
        <Loader2 class="size-3.5 animate-spin" /> {t("ui.loading")}
      </p>
    {:else if profiles.length === 0}
      <p class="text-[11px] text-[var(--text-muted)]">{t("browserProfiles.empty")}</p>
    {:else}
      <div class="space-y-1.5">
        {#each profiles as profile (profile.id)}
          <div class="flex flex-wrap items-center gap-2 rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3 py-2">
            {#if editingId === profile.id}
              <input
                class="chat-question__input min-w-40 flex-1"
                bind:value={editName}
                maxlength={60}
                aria-label={t("browserProfiles.name")}
                disabled={busy}
              />
              <button
                type="button"
                class="inline-flex h-7 items-center rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-2.5 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer"
                onclick={() => void rename()}
                disabled={busy || !editName.trim()}
              >{t("browserProfiles.save")}</button>
              <button
                type="button"
                class="inline-flex h-7 items-center rounded-lg border border-[var(--hairline)] px-2.5 text-[11px] font-semibold text-[var(--text-secondary)] cursor-pointer"
                onclick={() => (editingId = null)}
                disabled={busy}
              >{t("ui.cancel")}</button>
            {:else}
              <span class="min-w-0 flex-1 truncate text-xs font-semibold text-[var(--text-primary)]">
                {profile.name}
              </span>
              <span class="font-mono text-[11px] text-[var(--text-muted)]">
                {profile.id.slice(0, 14)}
              </span>
              <div class="w-36 shrink-0">
                <SimpleSelect
                  value={profile.cookie_policy || "inherit"}
                  options={cookiePolicyOptions}
                  onValueChange={(value) =>
                    void setProfilePolicy(profile, value)}
                  class="h-7 text-[11px] rounded-lg"
                  disabled={policyBusy === profile.id}
                />
              </div>
              <button
                type="button"
                class="icon-btn size-7"
                onclick={() => startRename(profile)}
                title={t("browserProfiles.rename")}
                aria-label={t("browserProfiles.rename")}
                disabled={busy}
              >
                <Pencil class="size-3.5" />
              </button>
              <button
                type="button"
                class="icon-btn size-7 text-[var(--text-muted)] hover:text-danger"
                onclick={() => void remove(profile)}
                title={t("browserProfiles.delete")}
                aria-label={t("browserProfiles.delete")}
                disabled={busy}
              >
                <Trash2 class="size-3.5" />
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if notice}
      <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
    {/if}
    {#if error}
      <p class="text-[11px] text-[var(--danger-text)] break-words" role="alert">{error}</p>
    {/if}
    <p class="text-[11px] leading-4 text-[var(--text-muted)]">
      {t("browserProfiles.deleteHint")}
    </p>
  </div>
</section>
