<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import { t } from "$lib/i18n";
  import {
    Check,
    Loader2,
    Mail,
    Save,
    Send,
    Trash2,
    X,
  } from "@lucide/svelte";

  interface AgentEmailStatus {
    configured: boolean;
    host: string;
    port: number;
    security: string;
    username: string;
    from: string;
    allowlist: string;
    allowlist_valid: boolean;
    has_password: boolean;
  }

  interface EmailDraftSummary {
    id: string;
    bot_id: string;
    thread_id: string | null;
    run_id: string | null;
    to_addresses: string[];
    cc_addresses: string[];
    subject: string;
    status: string;
    error: string | null;
    created_at: string;
    updated_at: string;
    sent_at: string | null;
  }

  interface EmailDraft extends EmailDraftSummary {
    body: string;
  }

  let account = $state<AgentEmailStatus | null>(null);
  let host = $state("");
  let port = $state(587);
  let security = $state("starttls");
  let username = $state("");
  let password = $state("");
  let from = $state("");
  let allowlist = $state("");
  let testTo = $state("");

  let drafts = $state<EmailDraftSummary[]>([]);
  let selectedId = $state<string | null>(null);
  let draft = $state<EmailDraft | null>(null);
  let to = $state("");
  let cc = $state("");
  let subject = $state("");
  let body = $state("");

  let loading = $state(true);
  let accountBusy = $state(false);
  let editorBusy = $state(false);
  let error = $state("");
  let notice = $state("");

  let canEdit = $derived(draft?.status === "draft");
  let dirty = $derived(
    draft !== null &&
      (to !== draft.to_addresses.join(", ") ||
        cc !== draft.cc_addresses.join(", ") ||
        subject !== draft.subject ||
        body !== draft.body),
  );
  let validPort = $derived(Number(port) >= 1 && Number(port) <= 65535);
  let securityOptions = $derived([
    { value: "starttls", label: t("mail.starttls") },
    { value: "none", label: t("mail.none") },
  ]);

  function errorText(cause: unknown): string {
    return String(cause).replace(/^Error:\s*/, "");
  }

  function applyAccount(next: AgentEmailStatus | null | undefined) {
    if (!next) {
      account = null;
      host = "";
      port = 587;
      security = "starttls";
      username = "";
      password = "";
      from = "";
      allowlist = "";
      return;
    }
    const previousFrom = account?.from ?? "";
    account = next;
    host = next.host ?? "";
    port = next.port || 587;
    security = next.security === "none" ? "none" : "starttls";
    username = next.username ?? "";
    from = next.from ?? "";
    allowlist = next.allowlist ?? "";
    password = "";
    if (!testTo || testTo === previousFrom) {
      testTo = next.from ?? "";
    }
  }

  function applyDraft(next: EmailDraft) {
    selectedId = next.id;
    draft = next;
    to = next.to_addresses.join(", ");
    cc = next.cc_addresses.join(", ");
    subject = next.subject;
    body = next.body;
  }

  function clearDraftState() {
    selectedId = null;
    draft = null;
    to = "";
    cc = "";
    subject = "";
    body = "";
  }

  function closeEditor() {
    clearDraftState();
    error = "";
    notice = "";
  }

  function statusLabel(status: string): string {
    switch (status) {
      case "draft":
        return t("mail.statusDraft");
      case "sending":
        return t("mail.statusSending");
      case "sent":
        return t("mail.statusSent");
      case "discarded":
        return t("mail.statusDiscarded");
      default:
        return status;
    }
  }

  function statusClass(status: string): string {
    switch (status) {
      case "sending":
        return "text-warning bg-warning/10";
      case "sent":
        return "text-success bg-success/10";
      case "discarded":
        return "text-[var(--text-muted)] bg-[var(--surface-2)]";
      default:
        return "text-[var(--brand-text)] bg-[var(--brand-soft)]";
    }
  }

  function formatTime(value: string | null): string {
    if (!value) return "";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  async function load() {
    loading = true;
    error = "";
    try {
      const [nextAccount, nextDrafts] = await Promise.all([
        invoke<AgentEmailStatus>("get_agent_email"),
        invoke<EmailDraftSummary[]>("list_email_drafts"),
      ]);
      applyAccount(nextAccount);
      drafts = Array.isArray(nextDrafts) ? nextDrafts : [];
    } catch (cause) {
      error = `${t("mail.loadError")} ${errorText(cause)}`;
    } finally {
      loading = false;
    }
  }

  async function refreshDrafts() {
    try {
      drafts = await invoke<EmailDraftSummary[]>("list_email_drafts");
    } catch (cause) {
      error = errorText(cause);
    }
  }

  async function saveAccount() {
    if (accountBusy || loading || !host.trim() || !from.trim() || !validPort) return;
    accountBusy = true;
    error = "";
    notice = "";
    const input: Record<string, string | number> = {
      host: host.trim(),
      port: Number(port),
      security,
      username: username.trim(),
      from: from.trim(),
      allowlist: allowlist.trim(),
    };
    // Blank means "keep the saved password". Sending an empty string would
    // clear it, which is reserved for removing the account.
    if (password !== "") input.password = password;
    try {
      applyAccount(await invoke<AgentEmailStatus>("set_agent_email", { input }));
      notice = t("mail.saved");
    } catch (cause) {
      error = errorText(cause);
    } finally {
      accountBusy = false;
    }
  }

  async function testAccount() {
    if (accountBusy || loading || !account?.configured || !testTo.trim()) return;
    if (!confirm(t("mail.testConfirm", { to: testTo.trim() }))) return;
    accountBusy = true;
    error = "";
    notice = "";
    try {
      await invoke("test_agent_email", { to: testTo.trim() });
      notice = t("mail.testSent");
    } catch (cause) {
      error = errorText(cause);
    } finally {
      accountBusy = false;
    }
  }

  async function removeAccount() {
    if (accountBusy || loading) return;
    if (!confirm(t("mail.removeConfirm"))) return;
    accountBusy = true;
    error = "";
    notice = "";
    try {
      await invoke("clear_agent_email");
      account = null;
      host = "";
      port = 587;
      security = "starttls";
      username = "";
      password = "";
      from = "";
      allowlist = "";
      testTo = "";
    } catch (cause) {
      error = errorText(cause);
    } finally {
      accountBusy = false;
    }
  }

  async function openDraft(id: string) {
    if (editorBusy) return;
    selectedId = id;
    editorBusy = true;
    error = "";
    notice = "";
    try {
      const next = await invoke<EmailDraft | null>("get_email_draft", { id });
      if (next) {
        applyDraft(next);
      } else {
        clearDraftState();
        error = t("mail.loadError");
        await refreshDrafts();
      }
    } catch (cause) {
      clearDraftState();
      error = errorText(cause);
    } finally {
      editorBusy = false;
    }
  }

  async function refreshAfterChange(id: string) {
    await refreshDrafts();
    try {
      const next = await invoke<EmailDraft | null>("get_email_draft", { id });
      if (next) {
        applyDraft(next);
      } else {
        clearDraftState();
      }
    } catch (cause) {
      error = errorText(cause);
    }
  }

  async function saveDraft() {
    if (editorBusy || !draft || !canEdit) return;
    const id = draft.id;
    editorBusy = true;
    error = "";
    notice = "";
    try {
      const next = await invoke<EmailDraft>("update_email_draft", {
        id,
        to: to.trim(),
        cc: cc.trim(),
        subject,
        body,
      });
      applyDraft(next);
      notice = t("mail.saved");
      await refreshDrafts();
    } catch (cause) {
      error = errorText(cause);
    } finally {
      editorBusy = false;
    }
  }

  async function sendDraft() {
    if (editorBusy || !draft || !canEdit || dirty) return;
    const id = draft.id;
    const recipients = [...draft.to_addresses, ...draft.cc_addresses].join(", ");
    if (
      !confirm(
        t("mail.sendConfirm", {
          recipients,
          subject: draft.subject || t("mail.untitled"),
        }),
      )
    ) {
      return;
    }
    editorBusy = true;
    error = "";
    notice = "";
    try {
      await invoke("send_email_draft", { id });
      await refreshAfterChange(id);
    } catch (cause) {
      error = errorText(cause);
      await refreshAfterChange(id);
    } finally {
      editorBusy = false;
    }
  }

  async function discardDraft() {
    if (editorBusy || !draft || !canEdit) return;
    const id = draft.id;
    if (!confirm(t("mail.discardConfirm"))) return;
    editorBusy = true;
    error = "";
    notice = "";
    try {
      await invoke("discard_email_draft", { id });
      await refreshAfterChange(id);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      editorBusy = false;
    }
  }

  onMount(() => void load());
</script>

<section class="space-y-5" aria-label={t("mail.title")}>
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <h3 class="flex items-center gap-2 text-lg font-bold text-[var(--text-primary)]">
        <Mail class="size-5 text-[var(--brand-text)]" />
        {t("mail.title")}
      </h3>
      <p class="mt-1 max-w-2xl text-sm text-[var(--text-tertiary)]">
        {t("mail.description")}
      </p>
    </div>
  </div>

  {#if notice}
    <p class="text-[11px] text-[var(--success-text)]" role="status">{notice}</p>
  {/if}
  {#if error}
    <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
      {error}
    </p>
  {/if}

  {#if loading}
    <p class="flex items-center gap-2 text-[11px] text-[var(--text-tertiary)]" role="status">
      <Loader2 class="size-3.5 animate-spin" />
      {t("ui.loading")}
    </p>
  {:else}
    <div class="space-y-3 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4">
      <div class="flex flex-wrap items-start justify-between gap-2">
        <div class="min-w-0">
          <h4 class="text-sm font-bold text-[var(--text-primary)]">{t("mail.account")}</h4>
          <p class="mt-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
            {t("mail.accountHint")}
          </p>
        </div>
        <span
          class="rounded-full px-2 py-0.5 text-[11px] font-mono {account?.configured
            ? 'bg-success/10 text-success'
            : 'bg-[var(--surface-2)] text-[var(--text-muted)]'}"
        >
          {account?.configured ? t("mail.configured") : t("mail.notConfigured")}
        </span>
      </div>

      <p class="text-[11px] leading-4 text-[var(--text-muted)]">
        {t("mail.separateAccount")}
      </p>

      <div class="grid gap-3 sm:grid-cols-2">
        <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
          <span class="block">{t("mail.host")}</span>
          <input
            class="chat-question__input font-mono"
            bind:value={host}
            placeholder="smtp.example.com"
            aria-label={t("mail.host")}
            autocomplete="off"
            spellcheck="false"
            disabled={accountBusy}
          />
        </label>
        <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
          <span class="block">{t("mail.port")}</span>
          <input
            class="chat-question__input font-mono"
            type="number"
            min="1"
            max="65535"
            bind:value={port}
            aria-label={t("mail.port")}
            disabled={accountBusy}
          />
        </label>
        <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
          <span class="block">{t("mail.security")}</span>
          <SimpleSelect
            value={security}
            options={securityOptions}
            onValueChange={(value) => (security = value)}
            ariaLabel={t("mail.security")}
            class="h-8.5 rounded-xl"
            disabled={accountBusy}
          />
        </label>
        <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
          <span class="block">{t("mail.username")}</span>
          <input
            class="chat-question__input font-mono"
            bind:value={username}
            aria-label={t("mail.username")}
            autocomplete="off"
            spellcheck="false"
            disabled={accountBusy}
          />
        </label>
        <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
          <span class="block">{t("mail.password")}</span>
          <input
            class="chat-question__input font-mono"
            type="password"
            bind:value={password}
            placeholder={account?.has_password ? t("mail.passwordKeep") : t("mail.password")}
            aria-label={t("mail.password")}
            autocomplete="off"
            spellcheck="false"
            disabled={accountBusy}
          />
          {#if account?.has_password}
            <span class="flex items-center gap-1 text-[11px] text-[var(--success-text)]">
              <Check class="size-3" />
              {t("mail.passwordKeep")}
            </span>
          {/if}
        </label>
        <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
          <span class="block">{t("mail.from")}</span>
          <input
            class="chat-question__input font-mono"
            type="email"
            bind:value={from}
            placeholder="agent@example.com"
            aria-label={t("mail.from")}
            autocomplete="off"
            spellcheck="false"
            disabled={accountBusy}
          />
        </label>
        <label class="space-y-1 text-[11px] text-[var(--text-secondary)] sm:col-span-2">
          <span class="block">{t("mail.allowlist")}</span>
          <input
            class="chat-question__input font-mono"
            bind:value={allowlist}
            placeholder="example.com, person@example.com"
            aria-label={t("mail.allowlist")}
            autocomplete="off"
            spellcheck="false"
            disabled={accountBusy}
          />
          <span class="mt-1 block text-[11px] leading-4 text-[var(--text-muted)]">
            {t("mail.allowlistHint")}
          </span>
          {#if account && !account.allowlist_valid}
            <span class="mt-1 block text-[11px] leading-4 text-[var(--danger-text)]" role="alert">
              {t("mail.allowlistInvalid")}
            </span>
          {/if}
        </label>
      </div>

      <p class="text-[11px] leading-4 text-[var(--text-muted)]">
        {t("mail.loopbackOnly")}
      </p>

      <div class="flex flex-wrap items-end gap-2">
        <label class="min-w-52 flex-1 text-[11px] text-[var(--text-secondary)]">
          <span class="mb-1 block">{t("mail.testTo")}</span>
          <input
            class="chat-question__input font-mono"
            type="email"
            bind:value={testTo}
            placeholder={from || "person@example.com"}
            aria-label={t("mail.testTo")}
            autocomplete="off"
            spellcheck="false"
            disabled={accountBusy}
          />
        </label>
        <button
          type="button"
          class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
          onclick={() => void testAccount()}
          disabled={accountBusy || !account?.configured || !testTo.trim()}
        >
          {#if accountBusy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <Send class="size-3.5" />
          {/if}
          {t("mail.test")}
        </button>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
          onclick={() => void saveAccount()}
          disabled={accountBusy || !host.trim() || !from.trim() || !validPort}
        >
          {#if accountBusy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <Save class="size-3.5" />
          {/if}
          {t("mail.save")}
        </button>
        <button
          type="button"
          class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 text-[11px] font-semibold text-[var(--danger-text)] cursor-pointer disabled:opacity-50"
          onclick={() => void removeAccount()}
          disabled={accountBusy || !account?.configured}
        >
          <Trash2 class="size-3.5" />
          {t("mail.remove")}
        </button>
      </div>

      <p class="text-[11px] leading-4 text-[var(--text-muted)]">{t("mail.noInbound")}</p>
      <p class="text-[11px] leading-4 text-[var(--text-muted)]">{t("mail.advancedHint")}</p>
    </div>

    {#if draft}
      <div class="space-y-3 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4">
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div class="min-w-0">
            <h4 class="truncate text-sm font-bold text-[var(--text-primary)]">
              {draft.subject || t("mail.untitled")}
            </h4>
            <div class="mt-1 flex flex-wrap items-center gap-2">
              <span class="rounded-full px-2 py-0.5 text-[11px] font-mono {statusClass(draft.status)}">
                {statusLabel(draft.status)}
              </span>
              {#if draft.status === "sending"}
                <span class="text-[11px] text-[var(--warning-text)]" role="status">
                  {t("mail.sending")}
                </span>
              {/if}
              {#if draft.status === "sent" && draft.sent_at}
                <span class="text-[11px] text-[var(--text-muted)]">
                  {t("mail.sentAt", { time: formatTime(draft.sent_at) })}
                </span>
              {/if}
            </div>
          </div>
          <button
            type="button"
            class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-3 text-[11px] font-semibold text-[var(--text-secondary)] cursor-pointer disabled:opacity-50"
            onclick={closeEditor}
            disabled={editorBusy}
          >
            <X class="size-3.5" />
            {t("mail.close")}
          </button>
        </div>

        {#if draft.error}
          <p class="break-words text-[11px] text-[var(--danger-text)]" role="alert">
            {draft.error}
          </p>
        {/if}

        <div class="grid gap-3 sm:grid-cols-2">
          <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
            <span class="block">{t("mail.to")}</span>
            <input
              class="chat-question__input font-mono"
              bind:value={to}
              aria-label={t("mail.to")}
              autocomplete="off"
              spellcheck="false"
              disabled={editorBusy || !canEdit}
            />
          </label>
          <label class="space-y-1 text-[11px] text-[var(--text-secondary)]">
            <span class="block">{t("mail.cc")}</span>
            <input
              class="chat-question__input font-mono"
              bind:value={cc}
              aria-label={t("mail.cc")}
              autocomplete="off"
              spellcheck="false"
              disabled={editorBusy || !canEdit}
            />
          </label>
          <label class="space-y-1 text-[11px] text-[var(--text-secondary)] sm:col-span-2">
            <span class="block">{t("mail.subject")}</span>
            <input
              class="chat-question__input"
              bind:value={subject}
              aria-label={t("mail.subject")}
              autocomplete="off"
              disabled={editorBusy || !canEdit}
            />
          </label>
          <label class="space-y-1 text-[11px] text-[var(--text-secondary)] sm:col-span-2">
            <span class="block">{t("mail.body")}</span>
            <textarea
              class="min-h-32 w-full rounded-lg border border-[var(--hairline)] bg-[var(--surface-2)] px-2.5 py-2 text-xs text-[var(--text-primary)] placeholder:text-[var(--text-muted)]"
              rows="7"
              bind:value={body}
              aria-label={t("mail.body")}
              spellcheck="false"
              disabled={editorBusy || !canEdit}
            ></textarea>
          </label>
        </div>

        {#if dirty && canEdit}
          <p class="text-[11px] leading-4 text-[var(--warning-text)]" role="status">
            {t("mail.saveBeforeSend")}
          </p>
        {/if}

        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
            onclick={() => void saveDraft()}
            disabled={editorBusy || !canEdit}
          >
            {#if editorBusy}
              <Loader2 class="size-3.5 animate-spin" />
            {:else}
              <Save class="size-3.5" />
            {/if}
            {t("mail.saveDraft")}
          </button>
          <button
            type="button"
            class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--brand)]/40 bg-[var(--brand-soft)] px-3 text-[11px] font-semibold text-[var(--brand-text)] cursor-pointer disabled:opacity-50"
            onclick={() => void sendDraft()}
            disabled={editorBusy || !canEdit || dirty}
          >
            <Send class="size-3.5" />
            {t("mail.send")}
          </button>
          <button
            type="button"
            class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 text-[11px] font-semibold text-[var(--danger-text)] cursor-pointer disabled:opacity-50"
            onclick={() => void discardDraft()}
            disabled={editorBusy || !canEdit}
          >
            <Trash2 class="size-3.5" />
            {t("mail.discard")}
          </button>
        </div>
      </div>
    {/if}

    <div class="space-y-3 rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4">
      <div>
        <h4 class="text-sm font-bold text-[var(--text-primary)]">{t("mail.drafts")}</h4>
        <p class="mt-1 text-[11px] leading-4 text-[var(--text-tertiary)]">
          {t("mail.draftsHint")}
        </p>
      </div>

      {#if drafts.length === 0}
        <p class="text-[11px] text-[var(--text-muted)]">{t("mail.noDrafts")}</p>
      {:else}
        <ul class="divide-y divide-[var(--hairline)]">
          {#each drafts as item (item.id)}
            <li>
              <button
                type="button"
                class="w-full rounded-xl px-2 py-3 text-left cursor-pointer transition-colors hover:bg-[var(--surface-2)] disabled:opacity-50"
                onclick={() => void openDraft(item.id)}
                disabled={editorBusy}
                aria-pressed={selectedId === item.id}
              >
                <div class="flex flex-wrap items-center gap-2">
                  <span class="rounded-full px-2 py-0.5 text-[11px] font-mono {statusClass(item.status)}">
                    {statusLabel(item.status)}
                  </span>
                  <span class="min-w-0 flex-1 truncate text-xs font-semibold text-[var(--text-primary)]">
                    {item.subject || t("mail.untitled")}
                  </span>
                  <span class="text-[11px] font-mono text-[var(--text-muted)]">
                    {formatTime(item.created_at)}
                  </span>
                </div>
                <p class="mt-1 truncate text-[11px] text-[var(--text-tertiary)]">
                  {t("mail.to")}: {item.to_addresses.join(", ")}
                </p>
                {#if item.error}
                  <p class="mt-1 break-words text-[11px] text-[var(--danger-text)]">
                    {item.error}
                  </p>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>
