<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import {
    Users, X, Loader2, FileText, Link2, AlertTriangle, CheckCircle2, Bot as BotIcon,
  } from "@lucide/svelte";

  interface Props {
    onClose: () => void;
    onImported?: (summary: any) => void;
  }

  let { onClose, onImported }: Props = $props();

  let mode = $state<"paste" | "url">("paste");
  let markdown = $state("");
  let url = $state("");
  let preview = $state<any | null>(null);
  let loading = $state(false);
  let importing = $state(false);
  let error = $state<string | null>(null);
  let result = $state<any | null>(null);

  async function doPreview() {
    error = null;
    preview = null;
    loading = true;
    try {
      preview =
        mode === "url"
          ? await invoke("fetch_and_preview_team", { url: url.trim() })
          : await invoke("preview_team", { markdown });
    } catch (e: any) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function doImport() {
    error = null;
    importing = true;
    try {
      result = await invoke("import_team", { markdown });
      onImported?.(result);
    } catch (e: any) {
      error = String(e);
    } finally {
      importing = false;
    }
  }

  function reset() {
    preview = null;
    result = null;
    error = null;
  }
</script>

<div class="fixed inset-0 z-[60] flex items-center justify-center bg-black/60  p-4">
  <div class="modal-panel w-full max-w-2xl flex flex-col max-h-[90vh]">
    <div class="modal-header flex items-center justify-between shrink-0">
      <div class="flex items-center gap-2">
        <Users class="size-4 text-[var(--brand-text)]" />
        <span class="font-bold text-sm text-[var(--text-primary)]">{t("team.title")}</span>
      </div>
      <button
        type="button"
        onclick={onClose}
        aria-label="Close"
        class="size-7 rounded-full bg-[var(--surface-2)] border border-[var(--hairline)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center justify-center cursor-pointer"
      >
        <X class="size-3.5" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto px-5 py-4 space-y-4">
      {#if result}
        <div class="rounded-xl border border-success/30 bg-success/20 px-4 py-3 space-y-2" role="status">
          <div class="flex items-center gap-2 text-success font-bold text-sm">
            <CheckCircle2 class="size-4" /> {t("team.imported", { name: result.name })}
          </div>
          <div class="text-[12px] text-[var(--text-secondary)] space-y-0.5">
            <div>{t("team.botsCreated", { n: result.bots.length })}</div>
            {#if result.office_id}<div>{t("team.officeCreated")}</div>{/if}
            {#if result.routines.length}<div>{t("team.routinesCreated", { n: result.routines.length })}</div>{/if}
          </div>
          <p class="text-[11px] text-[var(--text-muted)]">{t("team.routinesNote")}</p>
        </div>
        <Button onclick={onClose} class="w-full">{t("ui.done")}</Button>
      {:else}
        <!-- Source -->
        <div class="flex gap-2">
          <button
            type="button"
            onclick={() => { mode = "paste"; reset(); }}
            aria-pressed={mode === "paste"}
            class="flex-1 h-9 rounded-xl border text-xs font-bold flex items-center justify-center gap-1.5 cursor-pointer {mode === 'paste' ? 'border-[var(--brand)] bg-[var(--brand-soft)] text-[var(--text-primary)]' : 'border-[var(--hairline)] text-[var(--text-tertiary)]'}"
          >
            <FileText class="size-3.5" /> {t("team.pasteMd")}
          </button>
          <button
            type="button"
            onclick={() => { mode = "url"; reset(); }}
            aria-pressed={mode === "url"}
            class="flex-1 h-9 rounded-xl border text-xs font-bold flex items-center justify-center gap-1.5 cursor-pointer {mode === 'url' ? 'border-[var(--brand)] bg-[var(--brand-soft)] text-[var(--text-primary)]' : 'border-[var(--hairline)] text-[var(--text-tertiary)]'}"
          >
            <Link2 class="size-3.5" /> {t("team.fromUrl")}
          </button>
        </div>

        {#if mode === "paste"}
          <div class="space-y-1.5">
            <Label for="team-md" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">{t("team.mdLabel")}</Label>
            <Textarea
              id="team-md"
              bind:value={markdown}
              oninput={reset}
              rows={10}
              placeholder={"---\nname: My Team\nbots:\n  - name: Chief of Staff\n    title: Chief of Staff\n    prompt: |\n      You coordinate the team.\n---\n# Playbook\n..."}
              class="font-mono text-xs bg-[var(--surface-2)] border-[var(--hairline)]"
            />
          </div>
        {:else}
          <div class="space-y-1.5">
            <Label for="team-url" class="text-xs font-bold uppercase tracking-wider text-[var(--text-tertiary)]">{t("team.urlLabel")}</Label>
            <Input id="team-url" bind:value={url} oninput={reset} placeholder="https://raw.githubusercontent.com/.../team.md" class="h-9 text-xs bg-[var(--surface-2)] border-[var(--hairline)]" />
          </div>
        {/if}

        {#if error}
          <div class="flex items-start gap-2 rounded-xl border border-danger/30 bg-danger/20 px-3 py-2 text-[11px] text-danger" role="alert">
            <AlertTriangle class="size-3.5 shrink-0 mt-0.5" /><span>{error}</span>
          </div>
        {/if}

        {#if !preview}
          <Button
            onclick={doPreview}
            disabled={loading || (mode === 'paste' ? !markdown.trim() : !url.trim())}
            class="w-full gap-1.5"
          >
            {#if loading}<Loader2 class="size-3.5 animate-spin" />{/if}
            {t("team.review")}
          </Button>
        {:else}
          <!-- Review screen: nothing is created until Import -->
          <div class="rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)] p-4 space-y-3">
            <div>
              <div class="text-sm font-bold text-[var(--text-primary)]">{preview.name}</div>
              {#if preview.description}<div class="text-[11px] text-[var(--text-tertiary)] mt-0.5">{preview.description}</div>{/if}
              <div class="text-[10px] font-mono text-[var(--brand-text)] mt-1">{preview.summary}</div>
            </div>
            <div class="space-y-1.5">
              <div class="text-[10px] uppercase tracking-wider text-[var(--text-muted)] font-bold">{t("team.bots")}</div>
              {#each preview.bots as b}
                <div class="flex items-center gap-2 text-[12px] text-[var(--text-secondary)]">
                  <BotIcon class="size-3.5 text-[var(--brand-text)] shrink-0" />
                  <span class="font-semibold">{b.name}</span>
                  {#if b.rank}<span class="text-[10px] font-mono text-[var(--text-muted)]">{b.rank}</span>{/if}
                  {#if b.model}<span class="text-[10px] font-mono text-[var(--text-muted)]">{b.model}</span>{/if}
                </div>
              {/each}
            </div>
            {#if preview.office}
              <div>
                <div class="text-[10px] uppercase tracking-wider text-[var(--text-muted)] font-bold">{t("team.office")}</div>
                <div class="text-[12px] text-[var(--text-secondary)]">{preview.office.name} {#if preview.office.template}<span class="text-[10px] font-mono text-[var(--text-muted)]">{preview.office.template}</span>{/if}</div>
              </div>
            {/if}
            {#if preview.routines?.length}
              <div>
                <div class="text-[10px] uppercase tracking-wider text-[var(--text-muted)] font-bold">{t("team.routinesPaused")}</div>
                {#each preview.routines as r}
                  <div class="text-[12px] text-[var(--text-secondary)]">{r.name} · <span class="font-mono text-[10px]">{r.bot}</span></div>
                {/each}
              </div>
            {/if}
          </div>

          <div class="flex gap-2">
            <Button variant="outline" onclick={reset} class="flex-1">{t("team.back")}</Button>
            <Button onclick={doImport} disabled={importing} class="flex-1 gap-1.5">
              {#if importing}<Loader2 class="size-3.5 animate-spin" />{/if}
              {t("team.importTeam")}
            </Button>
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>
