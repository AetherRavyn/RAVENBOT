<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Loader2 } from "@lucide/svelte";
  import Workspace from "$lib/components/workspace/Workspace.svelte";
  import AttentionIsland from "$lib/components/AttentionIsland.svelte";
  import Login from "$lib/components/Login.svelte";

  // The attention island is a second Tauri window loading the same SPA with
  // `?island=1`. It renders only the compact pill, never the workspace.
  const island =
    typeof window !== "undefined" &&
    (window.location.hash === "#island" ||
      new URLSearchParams(window.location.search).get("island") === "1");

  let mode = $state<"loading" | "launcher" | "app">(island ? "app" : "loading");

  onMount(async () => {
    if (island) return;
    try {
      const value = await invoke<string>("ravenbot_account_mode");
      mode = value === "launcher" ? "launcher" : "app";
    } catch {
      // The launcher build does not register the full-app command.
      try {
        await invoke<string>("launcher_mode");
        mode = "launcher";
      } catch {
        // Browser previews and tests have no Tauri bridge; show the app.
        mode = "app";
      }
    }
  });
</script>

{#if island}
  <AttentionIsland />
{:else if mode === "loading"}
  <div class="h-screen w-screen flex items-center justify-center bg-background">
    <Loader2 class="size-6 animate-spin text-[var(--text-muted)]" />
  </div>
{:else if mode === "launcher"}
  <Login />
{:else}
  <Workspace />
{/if}
