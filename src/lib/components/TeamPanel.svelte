<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import SimpleSelect from "$lib/components/SimpleSelect.svelte";
  import {
    AlertCircle,
    CheckCircle2,
    Loader2,
    Trash2,
    UserPlus,
    Users,
  } from "@lucide/svelte";

  interface Props {
    currentAccount?: any | null;
  }

  let { currentAccount = null }: Props = $props();

  interface Member {
    username: string;
    role: string;
    owner: boolean;
    disabled: boolean;
    created_at: string;
  }

  let members = $state<Member[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busy = $state<string | null>(null);
  let removeTarget = $state<string | null>(null);
  let loadedFor = $state<string | null>(null);
  let notice = $state<string | null>(null);

  let addUsername = $state("");
  let addPassword = $state("");
  let addRole = $state("viewer");
  let adding = $state(false);
  let addError = $state<string | null>(null);

  const roleOptions = $derived([
    { value: "owner", label: t("workspaceTeam.roleOwner") },
    { value: "operator", label: t("workspaceTeam.roleOperator") },
    { value: "viewer", label: t("workspaceTeam.roleViewer") },
  ]);

  async function load() {
    if (currentAccount?.role !== "owner") {
      loading = false;
      return;
    }
    loading = true;
    error = null;
    try {
      members = await invoke<Member[]>("list_workspace_members");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    const owner =
      currentAccount?.role === "owner" ? String(currentAccount.username) : null;
    if (!owner) {
      loading = false;
      return;
    }
    if (owner !== loadedFor) {
      loadedFor = owner;
      void load();
    }
  });

  async function addMember(event: Event) {
    event.preventDefault();
    if (adding) return;
    const username = addUsername.trim();
    if (!username) {
      addError = t("workspaceTeam.usernameRequired");
      return;
    }
    adding = true;
    addError = null;
    notice = null;
    try {
      const member = await invoke<Member>("add_workspace_member", {
        username,
        password: addPassword.trim() || null,
        role: addRole,
      });
      notice = t("workspaceTeam.added", { name: member.username });
      addUsername = "";
      addPassword = "";
      addRole = "viewer";
      await load();
    } catch (e) {
      addError = String(e);
    } finally {
      adding = false;
    }
  }

  async function changeRole(member: Member, role: string) {
    if (role === member.role || busy) return;
    busy = member.username;
    error = null;
    notice = null;
    try {
      await invoke("set_workspace_member_role", {
        username: member.username,
        role,
      });
      notice = t("workspaceTeam.roleChanged", { name: member.username });
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }

  async function removeMember(member: Member) {
    if (busy) return;
    busy = member.username;
    error = null;
    notice = null;
    try {
      const removed = await invoke<boolean>("revoke_workspace_access", {
        username: member.username,
      });
      if (removed) {
        notice = t("workspaceTeam.removed", { name: member.username });
      }
      removeTarget = null;
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }
</script>

<div class="flex-1 overflow-y-auto no-scrollbar p-6">
  <div class="max-w-2xl mx-auto space-y-5">
    <header class="space-y-1">
      <h3 class="text-lg font-bold text-[var(--text-primary)] flex items-center gap-2">
        <Users class="size-5 text-[var(--brand-text)]" />
        {t("workspaceTeam.title")}
      </h3>
      <p class="text-xs leading-relaxed text-[var(--text-tertiary)]">
        {t("workspaceTeam.desc")}
      </p>
    </header>

    {#if currentAccount?.role !== "owner"}
      <div
        class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-2)] p-4 text-xs text-[var(--text-secondary)]"
        role="status"
      >
        {t("workspaceTeam.ownerOnly")}
      </div>
    {:else if loading}
      <div class="flex items-center gap-2 text-xs text-[var(--text-muted)]" role="status">
        <Loader2 class="size-4 animate-spin" />
        {t("workspaceTeam.loading")}
      </div>
    {:else if error}
      <div
        class="rounded-xl border border-[var(--danger-border)] bg-[var(--danger-soft)] p-3 text-xs text-[var(--danger-text)]"
        role="alert"
      >
        <div class="flex items-start gap-2">
          <AlertCircle class="mt-0.5 size-4 shrink-0" />
          <div class="space-y-2">
            <p class="break-words">{error}</p>
            <Button variant="outline" size="sm" onclick={load}>
              {t("ui.retry")}
            </Button>
          </div>
        </div>
      </div>
    {:else}
      {#if notice}
        <p
          class="flex items-center gap-1.5 rounded-lg border border-[var(--success-text)]/30 bg-[var(--success-soft)] px-3 py-2 text-[11px] text-[var(--success-text)]"
          role="status"
        >
          <CheckCircle2 class="size-3.5 shrink-0" />
          {notice}
        </p>
      {/if}

      <section class="space-y-2" aria-labelledby="team-members-heading">
        <h4
          id="team-members-heading"
          class="text-[11px] font-bold uppercase tracking-wider text-[var(--text-tertiary)]"
        >
          {t("workspaceTeam.members", { count: members.length })}
        </h4>
        {#each members as member (member.username)}
          <div
            class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-3 space-y-2"
          >
            <div class="flex flex-wrap items-center gap-2 min-w-0">
              <span
                class="size-7 shrink-0 rounded-full bg-[var(--brand-soft)] border border-[var(--brand)]/25 text-[var(--brand-text)] text-[11px] font-bold flex items-center justify-center uppercase"
                aria-hidden="true"
              >
                {member.username.slice(0, 1)}
              </span>
              <span class="text-xs font-bold text-[var(--text-primary)] truncate">
                {member.username}
              </span>
              {#if member.owner}
                <span
                  class="rounded-full border border-[var(--brand)]/30 bg-[var(--brand-soft)] px-1.5 py-0.5 text-[11px] font-semibold text-[var(--brand-text)]"
                >
                  {t("workspaceTeam.owner")}
                </span>
              {/if}
              {#if currentAccount?.username === member.username}
                <span class="text-[11px] text-[var(--text-muted)]">{t("workspaceTeam.you")}</span>
              {/if}
              {#if member.disabled}
                <span
                  class="rounded-full border border-[var(--warning-text)]/30 bg-[var(--warning-soft)] px-1.5 py-0.5 text-[11px] font-semibold text-[var(--warning-text)]"
                >
                  {t("workspaceTeam.disabled")}
                </span>
              {/if}

              {#if !member.owner}
                <div class="ml-auto flex items-center gap-1.5">
                  <div class="w-28">
                    <SimpleSelect
                      value={member.role}
                      options={roleOptions}
                      onValueChange={(role) => changeRole(member, role)}
                      disabled={busy === member.username}
                      class="h-7 text-[11px]"
                      ariaLabel={t("workspaceTeam.role")}
                    />
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    class="h-7 px-2 text-[11px] text-[var(--danger-text)] hover:bg-[var(--danger-soft)]"
                    onclick={() => (removeTarget = member.username)}
                    disabled={busy === member.username}
                    aria-label={t("workspaceTeam.removeLabel", { name: member.username })}
                  >
                    <Trash2 class="size-3.5" />
                  </Button>
                </div>
              {/if}
            </div>

            {#if removeTarget === member.username}
              <div
                class="rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2.5 space-y-2"
              >
                <p class="text-[11px] text-[var(--danger-text)]">
                  {t("workspaceTeam.removeConfirm", { name: member.username })}
                </p>
                <div class="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-7 px-2 text-[11px] border-[var(--danger-border)] text-[var(--danger-text)]"
                    onclick={() => removeMember(member)}
                    disabled={busy === member.username}
                  >
                    {busy === member.username ? t("workspaceTeam.removing") : t("workspaceTeam.remove")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    class="h-7 px-2 text-[11px]"
                    onclick={() => (removeTarget = null)}
                  >
                    {t("ui.cancel")}
                  </Button>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </section>

      <form
        class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3"
        onsubmit={addMember}
        aria-labelledby="team-add-heading"
      >
        <div class="space-y-1">
          <h4
            id="team-add-heading"
            class="text-xs font-bold text-[var(--text-primary)] flex items-center gap-1.5"
          >
            <UserPlus class="size-3.5 text-[var(--brand-text)]" />
            {t("workspaceTeam.addTitle")}
          </h4>
          <p class="text-[11px] leading-relaxed text-[var(--text-tertiary)]">
            {t("workspaceTeam.addDesc")}
          </p>
        </div>

        <div class="grid gap-3 sm:grid-cols-2">
          <div class="space-y-1">
            <Label for="team-username">{t("workspaceTeam.username")}</Label>
            <Input
              id="team-username"
              bind:value={addUsername}
              maxlength={64}
              autocomplete="off"
              class="h-8 text-xs"
              placeholder={t("workspaceTeam.usernamePlaceholder")}
            />
          </div>
          <div class="space-y-1">
            <Label for="team-password">{t("workspaceTeam.password")}</Label>
            <Input
              id="team-password"
              type="password"
              bind:value={addPassword}
              maxlength={256}
              autocomplete="new-password"
              class="h-8 text-xs"
              placeholder={t("workspaceTeam.passwordPlaceholder")}
              aria-describedby="team-password-hint"
            />
            <p id="team-password-hint" class="text-[11px] leading-relaxed text-[var(--text-muted)]">
              {t("workspaceTeam.passwordHint")}
            </p>
          </div>
        </div>

        <div class="flex flex-wrap items-end gap-3">
          <div class="w-40 space-y-1">
            <Label for="team-role">{t("workspaceTeam.role")}</Label>
            <SimpleSelect
              id="team-role"
              value={addRole}
              options={roleOptions}
              onValueChange={(role) => (addRole = role)}
              class="h-8 text-xs"
              ariaLabel={t("workspaceTeam.role")}
            />
          </div>
          <Button type="submit" size="sm" class="h-8" disabled={adding || !addUsername.trim()}>
            {#if adding}
              <Loader2 class="size-3.5 animate-spin" />
              {t("workspaceTeam.adding")}
            {:else}
              {t("workspaceTeam.add")}
            {/if}
          </Button>
        </div>

        {#if addError}
          <p
            class="rounded-lg border border-[var(--danger-border)] bg-[var(--danger-soft)] px-2.5 py-2 text-[11px] text-[var(--danger-text)] break-words"
            role="alert"
          >
            {addError}
          </p>
        {/if}

        <p class="text-[11px] leading-relaxed text-[var(--text-muted)]">
          {t("workspaceTeam.launcherNote")}
        </p>
      </form>
    {/if}
  </div>
</div>
