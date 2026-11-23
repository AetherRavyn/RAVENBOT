<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Bot, Loader2, LogIn, ShieldCheck, UserPlus } from "@lucide/svelte";
  import { initI18n, t } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";

  let accounts = $state<any[]>([]);
  let canCreateOwner = $state(false);
  let username = $state("");
  let password = $state("");
  let confirm = $state("");
  let busy = $state(false);
  let starting = $state(false);
  let error = $state<string | null>(null);
  let loginResult = $state<any>(null);
  let creatingExtra = $state(false);

  onMount(async () => {
    initI18n();
    try {
      const status: any = await invoke("account_launcher_status");
      accounts = status?.accounts || [];
      canCreateOwner = Boolean(status?.can_create_owner);
      if (accounts.length > 0) username = accounts[0].username;
    } catch (e) {
      error = String(e);
    }
  });

  const creating = $derived(canCreateOwner || creatingExtra);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = null;
    if (creating && password !== confirm) {
      error = t("login.passwordMismatch");
      return;
    }
    if (creating && password.length < 10) {
      error = t("login.passwordHint");
      return;
    }
    busy = true;
    starting = true;
    try {
      if (creating) {
        await invoke(
          canCreateOwner ? "create_owner_account" : "create_launcher_account",
          { username, password },
        );
      } else {
        const result: any = await invoke("login_account", { username, password });
        if (result?.workspaces?.length) {
          loginResult = result;
          starting = false;
          busy = false;
          return;
        }
      }
      // The launcher normally exits here and the account window takes over.
      // If the command returns, keep the status visible instead of guessing.
    } catch (e) {
      error = String(e);
      starting = false;
      busy = false;
    }
  }

  async function openWorkspace(workspaceId: string) {
    if (busy || !loginResult) return;
    busy = true;
    starting = true;
    error = null;
    try {
      await invoke("open_workspace", {
        loginId: loginResult.login_id,
        workspaceId,
      });
    } catch (e) {
      error = String(e);
      loginResult = null;
      starting = false;
      busy = false;
    }
  }
</script>

<main data-tauri-drag-region class="min-h-screen w-full flex items-center justify-center bg-background p-6">
  <div class="w-full max-w-md rounded-3xl border bg-card/80 p-7 shadow-xl" style="border-color: hsl(var(--border));">
    <div class="flex items-center gap-3 mb-6">
      <div class="size-11 rounded-2xl bg-[var(--brand-soft)] flex items-center justify-center">
        <Bot class="size-6 text-[var(--brand)]" />
      </div>
      <div>
        <h1 class="text-xl font-bold text-[var(--text-primary)]">{t("login.title")}</h1>
        <p class="text-xs text-[var(--text-tertiary)]">{t("login.subtitle")}</p>
      </div>
    </div>

    {#if starting}
      <div class="flex items-center gap-3 rounded-xl border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-4 py-4" aria-live="polite">
        <Loader2 class="size-5 animate-spin text-[var(--brand)]" />
        <p class="text-sm text-[var(--text-secondary)]">{t("login.starting")}</p>
      </div>
    {:else if loginResult}
      <div class="space-y-3">
        <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3.5 py-3">
          <p class="text-xs font-bold text-[var(--text-primary)]">{t("login.workspacePickerTitle")}</p>
          <p class="text-[11px] leading-relaxed text-[var(--text-tertiary)] mt-1">{t("login.workspacePickerDesc")}</p>
        </div>
        {#each loginResult.workspaces as workspace (workspace.id)}
          <div class="flex items-center justify-between gap-3 rounded-xl border px-3.5 py-3" style="border-color: hsl(var(--border));">
            <div class="min-w-0">
              <p class="text-sm font-semibold text-[var(--text-primary)] truncate">{workspace.username}</p>
              <p class="text-[11px] text-[var(--text-muted)]">
                {workspace.role === "owner" ? t("settings.accountRoleOwner") : workspace.role === "operator" ? t("settings.accountRoleOperator") : t("settings.accountRoleViewer")}
              </p>
            </div>
            <Button size="sm" class="gap-1.5" onclick={() => openWorkspace(workspace.id)} disabled={busy}>
              {t("login.workspaceOpen")}
            </Button>
          </div>
        {/each}
        {#if error}
          <p class="text-[11px] leading-relaxed text-danger bg-danger/10 border border-danger/20 rounded-lg px-3 py-2" aria-live="polite">{error}</p>
        {/if}
      </div>
    {:else}
      <form class="space-y-4" onsubmit={submit}>
        {#if creating}
          <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] px-3.5 py-3">
            <div class="flex items-center gap-2 mb-1">
              <UserPlus class="size-4 text-[var(--brand)]" />
              <p class="text-xs font-bold text-[var(--text-primary)]">{canCreateOwner ? t("login.createTitle") : t("login.createAnotherTitle")}</p>
            </div>
            <p class="text-[11px] leading-relaxed text-[var(--text-tertiary)]">{canCreateOwner ? t("login.createDesc") : t("login.createAnotherDesc")}</p>
          </div>
          <label class="block text-[11px] text-[var(--text-muted)]">
            <span class="block mb-1">{t("login.username")}</span>
            <input
              bind:value={username}
              class="h-10 w-full rounded-lg border bg-[var(--surface-1)] px-3 text-sm text-[var(--text-primary)]"
              style="border-color: hsl(var(--border));"
              autocomplete="username"
              required
              minlength="3"
              maxlength="64"
            />
          </label>
        {:else}
          <label class="block text-[11px] text-[var(--text-muted)]">
            <span class="block mb-1">{t("login.accountLabel")}</span>
            <input
              bind:value={username}
              list={accounts.length > 1 ? "ravenbot-accounts" : undefined}
              class="h-10 w-full rounded-lg border bg-[var(--surface-1)] px-3 text-sm text-[var(--text-primary)]"
              style="border-color: hsl(var(--border));"
              autocomplete="username"
              required
              readonly={accounts.length <= 1}
            />
            {#if accounts.length > 1}
              <datalist id="ravenbot-accounts">
                {#each accounts as account (account.username)}
                  <option value={account.username}>
                    {account.username}{account.owner ? ` · ${t("login.owner")}` : ""}
                  </option>
                {/each}
              </datalist>
            {/if}
          </label>
        {/if}

        <label class="block text-[11px] text-[var(--text-muted)]">
          <span class="block mb-1">{t("login.password")}</span>
          <input
            type="password"
            bind:value={password}
            class="h-10 w-full rounded-lg border bg-[var(--surface-1)] px-3 text-sm text-[var(--text-primary)]"
            style="border-color: hsl(var(--border));"
            autocomplete={creating ? "new-password" : "current-password"}
            required
            minlength={creating ? 10 : undefined}
          />
        </label>

        {#if creating}
          <label class="block text-[11px] text-[var(--text-muted)]">
            <span class="block mb-1">{t("login.confirm")}</span>
            <input
              type="password"
              bind:value={confirm}
              class="h-10 w-full rounded-lg border bg-[var(--surface-1)] px-3 text-sm text-[var(--text-primary)]"
              style="border-color: hsl(var(--border));"
              autocomplete="new-password"
              required
              minlength="10"
            />
          </label>
          <p class="text-[11px] text-[var(--text-muted)]">{t("login.passwordHint")}</p>
        {/if}

        {#if error}
          <p class="text-[11px] leading-relaxed text-danger bg-danger/10 border border-danger/20 rounded-lg px-3 py-2" aria-live="polite">
            {error}
          </p>
        {/if}

        <Button type="submit" class="w-full h-10 gap-2" disabled={busy}>
          {#if creating}
            <UserPlus class="size-4" /> {canCreateOwner ? t("login.create") : t("login.createAnother")}
          {:else}
            <LogIn class="size-4" /> {t("login.signIn")}
          {/if}
        </Button>
        {#if accounts.length > 0}
          <button
            type="button"
            class="w-full text-[11px] text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
            onclick={() => {
              creatingExtra = !creatingExtra;
              username = creatingExtra ? "" : accounts[0]?.username || "";
              password = "";
              confirm = "";
              error = null;
            }}
          >
            {creatingExtra ? t("login.cancel") : t("login.createAnother")}
          </button>
        {/if}
      </form>
    {/if}

    <div class="mt-6 flex items-start gap-2 border-t border-[var(--hairline)] pt-4">
      <ShieldCheck class="size-4 shrink-0 text-[var(--text-muted)] mt-0.5" />
      <p class="text-[11px] leading-relaxed text-[var(--text-muted)]">{t("login.securityNote")}</p>
    </div>
  </div>
</main>
