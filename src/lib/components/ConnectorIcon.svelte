<script lang="ts">
  import { cn } from "$lib/utils.js";
  import {
    Terminal,
    Database,
    Globe,
    Server,
    Key,
    Shield,
    Bot,
    Folder,
    Code,
    Cpu,
    Zap,
    Layers,
    FileText,
    Share2,
    HardDrive,
    Volume2,
    Clipboard,
    Radio,
    Home,
    Search,
    TrendingUp,
    MessageSquare,
    Sparkles,
    Lock,
    Compass,
    Eye,
    CheckCircle,
  } from "@lucide/svelte";

  interface Props {
    id: string;
    name?: string;
    class?: string;
    size?: "sm" | "md" | "lg" | "xl";
  }

  let { id = "", name = "", class: customClass = "", size = "md" }: Props = $props();

  let cleanId = $derived(id.toLowerCase().replace(/[^a-z0-9_]/g, ""));
  let cleanName = $derived(name.toLowerCase());

  let sizeClasses = $derived(
    size === "sm"
      ? "size-6.5 p-1 text-[10px] rounded-lg"
      : size === "lg"
        ? "size-12 p-2 text-sm rounded-2xl"
        : size === "xl"
          ? "size-14 p-2.5 text-base rounded-2xl"
          : "size-10 p-1.5 text-xs rounded-xl"
  );
</script>

<div class={cn("relative flex items-center justify-center overflow-hidden shrink-0 shadow-sm transition-transform group-hover:scale-105 border select-none", sizeClasses, customClass)}>
  <!-- DEV & CODE -->
  {#if cleanId.includes("github") || cleanName.includes("github")}
    <!-- GitHub SVG -->
    <div class="size-full bg-[#181717] flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
      </svg>
    </div>
  {:else if cleanId.includes("gitlab")}
    <!-- GitLab SVG -->
    <div class="size-full bg-[#E24329]/15 border border-[#E24329]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FC6D26]">
        <path d="M23.955 13.587l-1.342-4.135-2.664-8.189c-.135-.423-.73-.423-.867 0L16.418 9.45H7.582L4.918 1.263c-.137-.423-.732-.423-.867 0L1.387 9.452.045 13.587c-.121.375.014.789.331 1.023L12 23.054l11.624-8.444c.318-.234.453-.648.331-1.023z" />
      </svg>
    </div>
  {:else if cleanId.includes("bitbucket")}
    <!-- Bitbucket SVG -->
    <div class="size-full bg-[#0052CC]/20 border border-[#0052CC]/50 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#2684FF]">
        <path d="M2.65 3C2.3 3 2 3.3 2 3.65l2.25 16.7c.05.35.35.65.7.65h14.1c.35 0 .65-.3.7-.65L22 3.65c0-.35-.3-.65-.65-.65H2.65zM14.2 14.1H9.8L8.6 8.3h6.8l-1.2 5.8z" />
      </svg>
    </div>
  {:else if cleanId.includes("docker")}
    <!-- Docker SVG -->
    <div class="size-full bg-[#2496ED]/15 border border-[#2496ED]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#2496ED]">
        <path d="M13.983 11.078h2.119a.186.186 0 00.186-.185V9.006a.186.186 0 00-.186-.186h-2.119a.185.185 0 00-.185.186v1.887c0 .102.083.185.185.185zm-2.954-5.43h2.118a.186.186 0 00.186-.186V3.574a.186.186 0 00-.186-.185h-2.118a.185.185 0 00-.185.185v1.888c0 .102.082.185.185.185zm0 2.716h2.118a.187.187 0 00.186-.186V6.29a.186.186 0 00-.186-.185h-2.118a.185.185 0 00-.185.185v1.888c0 .102.082.186.185.186zm-2.93 0h2.12a.186.186 0 00.184-.186V6.29a.185.185 0 00-.185-.185H8.1a.185.185 0 00-.185.185v1.888c0 .102.083.186.185.186zm-2.964 0h2.119a.186.186 0 00.185-.186V6.29a.185.185 0 00-.185-.185H5.136a.186.186 0 00-.186.185v1.888c0 .102.084.186.186.186zm5.893 2.714h2.118a.186.186 0 00.186-.185V9.006a.186.186 0 00-.186-.186h-2.118a.185.185 0 00-.185.186v1.887c0 .102.082.185.185.185zm-2.93 0h2.12a.185.185 0 00.184-.185V9.006a.185.185 0 00-.184-.186H8.1a.185.185 0 00-.185.186v1.887c0 .102.083.185.185.185zm-2.964 0h2.119a.185.185 0 00.185-.185V9.006a.185.185 0 00-.185-.186H5.136a.186.186 0 00-.186.186v1.887c0 .102.084.185.186.185zm-2.928 0h2.119a.185.185 0 00.185-.185V9.006a.185.185 0 00-.185-.186H2.208a.186.186 0 00-.186.186v1.887c0 .102.084.185.186.185zM23.76 10.99c-.32-.23-.97-.32-1.57-.15-.17-.55-.65-.96-1.27-.96-.4 0-.75.17-.99.44-.76-.48-1.83-.56-2.73-.24-.13-.53-.59-.92-1.16-.92h-.03a1.9 1.9 0 00-1.87 1.48H.9a.9.9 0 00-.9.9v2.22c0 3.86 3.14 7 7 7h10c3.86 0 7-3.14 7-7 0-.58-.07-1.15-.24-1.68z" />
      </svg>
    </div>
  {:else if cleanId.includes("kubernetes")}
    <!-- Kubernetes SVG -->
    <div class="size-full bg-[#326CE5]/15 border border-[#326CE5]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#326CE5]">
        <path d="M12.001 0l-3.328 1.92-3.327 1.921v3.844l-3.327 1.922v7.688l3.327 1.921v3.844l3.327 1.92 3.328 1.921 3.327-1.92 3.328-1.921v-3.844l3.327-1.921V9.607l-3.327-1.922V3.841L15.328 1.92 12 0zm0 4.195l1.664.96v1.921l-1.664.96-1.664-.96V5.155l1.664-.96zm-4.99 2.882l1.664.96v1.921l-1.664.96-1.664-.96V8.037l1.664-.96zm9.98 0l1.664.96v1.921l-1.664.96-1.664-.96V8.037l1.664-.96zm-4.99 2.882l1.664.961v1.92l-1.664.961-1.664-.96V10.92l1.664-.961zm-4.99 2.882l1.664.96v1.921l-1.664.96-1.664-.96v-1.921l1.664-.96zm9.98 0l1.664.96v1.921l-1.664.96-1.664-.96v-1.921l1.664-.96zm-4.99 2.881l1.664.96v1.921l-1.664.96-1.664-.96v-1.921l1.664-.96z" />
      </svg>
    </div>
  {:else if cleanId.includes("postman")}
    <!-- Postman SVG -->
    <div class="size-full bg-[#FF6C37]/15 border border-[#FF6C37]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FF6C37]">
        <path d="M13.5 1.5c-4.97 0-9 4.03-9 9 0 1.95.62 3.75 1.68 5.23L1.5 22.5l6.77-4.68A8.96 8.96 0 0 0 13.5 19.5c4.97 0 9-4.03 9-9s-4.03-9-9-9zm4.23 12.77L13.5 11.5l-4.23 2.77 1.62-4.92L6.66 6.5h5.08L13.5 1.75 15.26 6.5h5.08l-4.23 2.85 1.62 4.92z" />
      </svg>
    </div>
  {:else if cleanId.includes("npm")}
    <!-- NPM SVG -->
    <div class="size-full bg-[#CB3837]/15 border border-[#CB3837]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#CB3837]">
        <path d="M0 0v24h24V0H0zm20.8 20.8h-4.8V8h-4.8v12.8H3.2V3.2h17.6v17.6z" />
      </svg>
    </div>
  {:else if cleanId.includes("pypi")}
    <!-- PyPI / Python SVG -->
    <div class="size-full bg-[#3776AB]/15 border border-[#3776AB]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#3776AB" d="M11.9 0C5.4 0 5.8 2.8 5.8 2.8l.01 2.9h6.2v.9H3.4S0 6.1 0 12.6c0 6.4 3 6.2 3 6.2h1.8v-2.5s-.1-3 2.9-3h5.1s2.8.05 2.8-2.7V2.8S16.1 0 11.9 0zm-1.7 1.8c.6 0 1 .4 1 1s-.4 1-1 1-1-.4-1-1 .4-1 1-1z" />
        <path fill="#FFD43B" d="M12.1 24c6.5 0 6.1-2.8 6.1-2.8l-.01-2.9h-6.2v-.9h8.6s3.4.5 3.4-6c0-6.4-3-6.2-3-6.2h-1.8v2.5s.1 3-2.9 3h-5.1s-2.8-.05-2.8 2.7v7.8s-.5 2.8 3.7 2.8zm1.7-1.8c-.6 0-1-.4-1-1s.4-1 1-1 1 .4 1 1-.4 1-1 1z" />
      </svg>
    </div>
  {:else if cleanId.includes("linear")}
    <!-- Linear SVG -->
    <div class="size-full bg-[#5E6AD2]/20 border border-[#5E6AD2]/50 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#5E6AD2]">
        <path d="M2.5 2.5h19v19h-19z" fill="none"/>
        <path d="M3.2 16.8L16.8 3.2c.4-.4 1.1-.4 1.5 0l2.5 2.5c.4.4.4 1.1 0 1.5L7.2 20.8c-.4.4-1.1.4-1.5 0l-2.5-2.5c-.4-.4-.4-1.1 0-1.5z"/>
      </svg>
    </div>
  {:else if cleanId.includes("jira")}
    <!-- Jira SVG -->
    <div class="size-full bg-[#0052CC]/15 border border-[#0052CC]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#2684FF]">
        <path d="M11.53 2c0 2.4-1.97 4.35-4.4 4.35H2.8v4.35h4.33c4.8 0 8.7-3.9 8.7-8.7V2h-4.3zM11.53 10.7c0 2.4-1.97 4.35-4.4 4.35H2.8v4.35h4.33c4.8 0 8.7-3.9 8.7-8.7v-.01h-4.3z"/>
      </svg>
    </div>
  {:else if cleanId.includes("sentry")}
    <!-- Sentry SVG -->
    <div class="size-full bg-[#362D59]/40 border border-[#FF4081]/35 flex items-center justify-center rounded-md text-[#FF4081]">
      <Shield class="size-full" />
    </div>
  {:else if cleanId.includes("git")}
    <!-- Git SVG -->
    <div class="size-full bg-[#F05032]/15 border border-[#F05032]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#F05032]">
        <path d="M23.546 10.93L13.067.452a1.5 1.5 0 0 0-2.124 0L8.835 2.56l2.94 2.94a1.8 1.8 0 0 1 2.29 2.29l2.83 2.83a1.8 1.8 0 1 1-.95.95l-2.65-2.65a1.8 1.8 0 0 1-1.99-.39l-2.8 2.8a1.8 1.8 0 1 1-.95-.03V6.26L.452 13.067a1.5 1.5 0 0 0 0 2.124l10.479 10.479a1.5 1.5 0 0 0 2.124 0l10.49-10.48a1.5 1.5 0 0 0 .001-2.26z" />
      </svg>
    </div>

  <!-- DATABASES -->
  {:else if cleanId.includes("postgres")}
    <!-- PostgreSQL SVG -->
    <div class="size-full bg-[#336791]/15 border border-[#336791]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#336791]">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z" />
      </svg>
    </div>
  {:else if cleanId.includes("mysql")}
    <!-- MySQL SVG -->
    <div class="size-full bg-[#00758F]/15 border border-[#00758F]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#F29111]">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 14H9v-2h2v2zm0-4H9V7h2v5zm4 4h-2V7h2v9z" />
      </svg>
    </div>
  {:else if cleanId.includes("duckdb")}
    <!-- DuckDB Official Yellow Duck SVG -->
    <div class="size-full bg-[#FFF000]/15 border border-[#FFF000]/40 flex items-center justify-center rounded-md text-amber-300">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FFF000]">
        <path d="M12 2a5 5 0 0 0-5 5c0 1.4.6 2.7 1.5 3.6C6.1 11.5 4 13.9 4 17a5 5 0 0 0 5 5h6a5 5 0 0 0 5-5c0-3.1-2.1-5.5-4.5-6.4.9-.9 1.5-2.2 1.5-3.6a5 5 0 0 0-5-5zm-1.5 4a1 1 0 1 1 0 2 1 1 0 0 1 0-2z" />
      </svg>
    </div>
  {:else if cleanId.includes("redis")}
    <!-- Redis SVG -->
    <div class="size-full bg-[#DC382D]/15 border border-[#DC382D]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#DC382D]">
        <path d="M21.578 14.285l-8.73-4.757a1.69 1.69 0 00-1.696 0L2.422 14.285a.846.846 0 000 1.48l8.73 4.757c.525.286 1.171.286 1.696 0l8.73-4.757a.846.846 0 000-1.48zM12 3.478L3.27 8.235a.846.846 0 000 1.48l8.73 4.757a1.69 1.69 0 001.696 0l8.73-4.757a.846.846 0 000-1.48L13.696 3.478a1.69 1.69 0 00-1.696 0z" />
      </svg>
    </div>
  {:else if cleanId.includes("mongodb")}
    <!-- MongoDB SVG -->
    <div class="size-full bg-[#47A248]/15 border border-[#47A248]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#47A248]">
        <path d="M12 0a11.96 11.96 0 00-1.12.06c-.88.1-1.74.32-2.56.66-.4.16-.78.36-1.15.58-.36.22-.7.47-1.01.75l-.42.4c-.26.27-.5.56-.72.86-.2.27-.38.56-.54.86-.16.29-.29.6-.4.91-.1.3-.18.61-.23.93-.05.3-.08.62-.08.93 0 .32.03.63.08.94.05.31.13.62.23.92.11.31.24.62.4.91.16.3.34.59.54.86.22.3.46.59.72.86l.42.4c.31.28.65.53 1.01.75.37.22.75.42 1.15.58.82.34 1.68.56 2.56.66.37.04.74.06 1.12.06s.75-.02 1.12-.06c.88-.1 1.74-.32 2.56-.66.4-.16.78-.36 1.15-.58.36-.22.7-.47 1.01-.75l.42-.4c.26-.27.5-.56.72-.86.2-.27.38-.56.54-.86.16-.29.29-.6.4-.91.1-.3.18-.61.23-.92.05-.31.08-.62.08-.94 0-.31-.03-.63-.08-.93-.05-.32-.13-.63-.23-.93-.11-.31-.24-.62-.4-.91-.16-.3-.34-.59-.54-.86-.22-.3-.46-.59-.72-.86l-.42-.4c-.31-.28-.65-.53-1.01-.75-.37-.22-.75-.42-1.15-.58-.82-.34-1.68-.56-2.56-.66A11.96 11.96 0 0012 0z" />
      </svg>
    </div>
  {:else if cleanId.includes("supabase")}
    <!-- Supabase SVG -->
    <div class="size-full bg-[#3ECF8E]/15 border border-[#3ECF8E]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#3ECF8E]">
        <path d="M21.362 9.354H12V.396a.396.396 0 0 0-.716-.233L.857 13.916a.396.396 0 0 0 .313.638H12v8.958a.396.396 0 0 0 .716.233l10.427-13.754a.396.396 0 0 0-.313-.637z" />
      </svg>
    </div>
  {:else if cleanId.includes("firebase")}
    <!-- Firebase SVG -->
    <div class="size-full bg-[#FFCA28]/15 border border-[#FFA000]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FFA000]">
        <path d="M3.89 15.672L6.255.461A.542.542 0 0 1 7.27.28l3.543 6.672zm16.788 3.737L18.423 4.285a.54.54 0 0 0-.946-.222L3.352 19.41l8.03 4.516a1.24 1.24 0 0 0 1.218 0l8.078-4.517z" />
      </svg>
    </div>
  {:else if cleanId.includes("neon")}
    <!-- Neon Postgres SVG -->
    <div class="size-full bg-[#00E599]/15 border border-[#00E599]/40 flex items-center justify-center rounded-md text-[#00E599]">
      <Database class="size-full" />
    </div>
  {:else if cleanId.includes("snowflake")}
    <!-- Snowflake SVG -->
    <div class="size-full bg-[#29B5E8]/15 border border-[#29B5E8]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#29B5E8]">
        <path d="M12 0l1.6 3.2L12 6.4 10.4 3.2zm0 17.6l1.6 3.2L12 24l-1.6-3.2zm8.4-8.4l3.2 1.6-3.2 1.6-1.6-3.2zm-16.8 0l-3.2 1.6 3.2 1.6 1.6-3.2zm12.3-4.3l3.4-.6-1.2 3.2-2.2-2.6zm-7.8 7.8l-3.4.6 1.2-3.2 2.2 2.6zm7.8 0l2.2 2.6-3.4-.6 1.2-3.2zm-7.8-7.8l-2.2-2.6 3.4.6-1.2 3.2z" />
      </svg>
    </div>
  {:else if cleanId.includes("elasticsearch") || cleanId.includes("elastic")}
    <!-- Elastic SVG -->
    <div class="size-full bg-[#005571]/20 border border-[#00BFB3]/40 flex items-center justify-center rounded-md text-[#00BFB3]">
      <Search class="size-full" />
    </div>

  <!-- CLOUD & INFRA -->
  {:else if cleanId.includes("aws")}
    <!-- AWS SVG -->
    <div class="size-full bg-[#FF9900]/15 border border-[#FF9900]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FF9900]">
        <path d="M18.793 15.698c-2.618 1.928-6.425 2.946-9.686 2.946-4.57 0-8.687-1.688-11.802-4.502-.244-.22-.027-.518.27-.353 3.37 1.868 7.49 2.986 11.728 2.986 2.906 0 6.13-.67 9.074-2.054.444-.21.815.318.416.977z" />
        <path d="M19.988 14.47c-.332-.424-2.19-.2-3.04-.1-.258.03-.298-.194-.067-.355 1.5-1.047 3.954-.746 4.24-.393.284.354-.74 2.766-2.146 3.93-.217.18-.423.084-.33-.153.308-.79.805-2.08.343-2.93z" />
      </svg>
    </div>
  {:else if cleanId.includes("cloudflare")}
    <!-- Cloudflare SVG -->
    <div class="size-full bg-[#F38020]/15 border border-[#F38020]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#F38020]">
        <path d="M18.23 9.05c-.44-2.73-2.8-4.8-5.65-4.8-2.34 0-4.35 1.4-5.27 3.42-.51-.12-1.04-.19-1.58-.19-3.47 0-6.28 2.81-6.28 6.28 0 3.47 2.81 6.28 6.28 6.28h12.56c2.92 0 5.28-2.36 5.28-5.28 0-2.77-2.13-5.04-4.85-5.26-.16-.16-.33-.31-.49-.45z" />
      </svg>
    </div>
  {:else if cleanId.includes("vercel")}
    <!-- Vercel Triangle SVG -->
    <div class="size-full bg-white/15 border border-white/30 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M24 22.525H0l12-21.05 12 21.05z" />
      </svg>
    </div>
  {:else if cleanId.includes("terraform")}
    <!-- Terraform SVG -->
    <div class="size-full bg-[#844FBA]/20 border border-[#844FBA]/50 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#844FBA]">
        <path d="M1.44 0v7.575l6.55 3.79V3.788L1.44 0zm7.55 4.363v7.575l6.55 3.79V8.153L8.99 4.363zm7.55 4.363v7.576l6.55 3.79V12.516L16.54 8.726zM1.44 9.088v7.575l6.55 3.79v-7.576L1.44 9.088z" />
      </svg>
    </div>

  <!-- AI & ML -->
  {:else if cleanId.includes("openai")}
    <!-- OpenAI SVG -->
    <div class="size-full bg-[#10A37F]/15 border border-[#10A37F]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#10A37F]">
        <path d="M22.282 9.821a5.985 5.985 0 00-.516-4.91 6.046 6.046 0 00-6.51-2.9A6.065 6.065 0 0010.86.3a6.05 6.05 0 00-5.786 4.148 6.02 6.02 0 00-4.086 2.97 6.05 6.05 0 00.743 7.108 5.98 5.98 0 00.516 4.91 6.046 6.046 0 006.51 2.9A6.055 6.055 0 0013.14 23.7a6.05 6.05 0 005.786-4.148 6.02 6.02 0 004.086-2.97 6.046 6.046 0 00-.73-7.761zM13.14 22.23a4.55 4.55 0 01-2.985-1.11 4.545 4.545 0 01-.137-.123l3.528-2.037a.75.75 0 00.375-.65v-4.99l1.5.866v4.614a4.57 4.57 0 01-2.281 3.43zm-7.65-4.417a4.545 4.545 0 01-.54-3.142l3.527 2.037a.75.75 0 00.75 0l4.321-2.495v1.732l-3.996 2.307a4.57 4.57 0 01-4.062-.439zm-1.89-7.854a4.54 4.54 0 012.445-2.031v4.074a.75.75 0 00.375.65l4.322 2.495-1.5.866-3.996-2.307a4.57 4.57 0 01-1.646-3.747zm13.14-3.484l-4.322-2.495 1.5-.866 3.996 2.307a4.57 4.57 0 011.646 3.747 4.54 4.54 0 01-2.445 2.031V7.126a.75.75 0 00-.375-.65zm3.78 4.417a4.545 4.545 0 01.54 3.142l-3.527-2.037a.75.75 0 00-.75 0l-4.321 2.495V12.73l3.996-2.307a4.57 4.57 0 014.062.439zM10.86 1.77a4.55 4.55 0 012.985 1.11c.046.038.092.079.137.123L10.454 5.04a.75.75 0 00-.375.65v4.99l-1.5-.866V5.2a4.57 4.57 0 012.281-3.43zM9.36 10.466l2.64-1.524 2.64 1.524v3.048l-2.64 1.524-2.64-1.524v-3.048z" />
      </svg>
    </div>
  {:else if cleanId.includes("anthropic") || cleanId.includes("claude")}
    <!-- Anthropic Claude SVG -->
    <div class="size-full bg-[#D97706]/15 border border-[#D97706]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#D97706]">
        <path d="M17.3 3H6.7l-4.7 9 4.7 9h10.6l4.7-9-4.7-9zm-5.3 14.5l-3.8-3.8 1.4-1.4 2.4 2.4 5.4-5.4 1.4 1.4-6.8 6.8z" />
      </svg>
    </div>
  {:else if cleanId.includes("huggingface")}
    <!-- Hugging Face Emoji SVG -->
    <div class="size-full bg-[#FFD21E]/15 border border-[#FFD21E]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FFD21E]">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-3 7c.83 0 1.5.67 1.5 1.5S9.83 12 9 12s-1.5-.67-1.5-1.5S8.17 9 9 9zm6 0c.83 0 1.5.67 1.5 1.5s-.67 1.5-1.5 1.5-1.5-.67-1.5-1.5.67-1.5 1.5-1.5zm-3 8.5c-2.33 0-4.31-1.46-5.11-3.5h10.22c-.8 2.04-2.78 3.5-5.11 3.5z" />
      </svg>
    </div>
  {:else if cleanId.includes("pinecone")}
    <!-- Pinecone SVG -->
    <div class="size-full bg-[#000000] border border-[#047857]/40 flex items-center justify-center rounded-md text-[#10B981]">
      <svg viewBox="0 0 24 24" class="size-full fill-[#10B981]">
        <path d="M12 2L4 9l2 3h12l2-3-8-7zm0 6l-5 5 1.5 2h7l1.5-2-5-5zm0 6l-3 4h6l-3-4z"/>
      </svg>
    </div>

  <!-- PRODUCTIVITY -->
  {:else if cleanId.includes("notion")}
    <!-- Notion SVG -->
    <div class="size-full bg-white/10 border border-white/20 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M4.459 4.208c.746.606 1.026.56 2.428.466l13.215-.793c.28 0 .047-.28-.046-.326L17.86 1.968c-.42-.326-.981-.7-2.055-.607L3.245 2.295c-.466.046-.56.28-.373.466l1.587 1.447zm.793 4.293v13.672c0 .7.373.933 1.167.886l14.428-.84c.793-.046.933-.513.933-1.12V7.474c0-.607-.233-.887-.793-.84l-14.94.887c-.56.047-.795.327-.795.98zm13.447 1.026c.093.42 0 .84-.42.887l-.84.14v10.312l-2.055-1.073V9.76l-4.293 6.906-2.1-1.073V8.873l1.167-.14c.42-.047.653-.327.56-.747-.094-.42-.42-.467-.84-.42L5.86 7.8c-.42.047-.653.327-.56.747.093.42.42.467.84.42l.84-.093v11.945c0 .653.373.98 1.073.933l1.913-.14c.653-.047.886-.42 1.213-.933l4.853-7.792v7.745c-.233.093-.653.28-.653.7 0 .42.373.56.98.513l3.687-.233c.56-.047.793-.42.793-.84 0-.42-.373-.513-.746-.56l-.84-.093V9.527z" />
      </svg>
    </div>
  {:else if cleanId.includes("slack")}
    <!-- Slack SVG -->
    <div class="size-full bg-[#4A154B]/20 border border-[#4A154B]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#E01E5A]">
        <path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z" />
      </svg>
    </div>
  {:else if cleanId.includes("discord")}
    <!-- Discord SVG -->
    <div class="size-full bg-[#5865F2]/15 border border-[#5865F2]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#5865F2]">
        <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994.021-.041.001-.09-.041-.106a13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.929 1.793 8.18 1.793 12.061 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.893.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.028zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
      </svg>
    </div>
  {:else if cleanId.includes("gdrive") || cleanId.includes("googledrive")}
    <!-- Google Drive SVG -->
    <div class="size-full bg-white/10 border border-white/20 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#0066DA" d="M8.01 19.5h12.98l-4.49-7.79H3.52z" />
        <path fill="#00AC47" d="M3.52 11.71l6.49-11.21h6.49L9.99 11.71z" />
        <path fill="#FFBA00" d="M16.5 0.5l6.49 11.21-6.49 7.79-6.49-7.79z" />
      </svg>
    </div>
  {:else if cleanId.includes("gcalendar") || cleanId.includes("calendar")}
    <!-- Google Calendar SVG -->
    <div class="size-full bg-[#4285F4]/15 border border-[#4285F4]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full">
        <rect width="20" height="20" x="2" y="2" fill="#4285F4" rx="4" />
        <rect width="14" height="14" x="5" y="6" fill="#FFFFFF" rx="2" />
        <text x="12" y="17" fill="#4285F4" font-size="9" font-family="sans-serif" font-weight="bold" text-anchor="middle">31</text>
      </svg>
    </div>
  {:else if cleanId.includes("gsheets") || cleanId.includes("sheets")}
    <!-- Google Sheets SVG -->
    <div class="size-full bg-[#0F9D58]/15 border border-[#0F9D58]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#0F9D58]">
        <path d="M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6zm-1 9h5v2h-5v-2zm-5 0h3v2H8v-2zm0 4h3v2H8v-2zm5 2v-2h5v2h-5zM13 3.5L18.5 9H13V3.5z" />
      </svg>
    </div>
  {:else if cleanId.includes("gdocs") || cleanId.includes("docs")}
    <!-- Google Docs SVG -->
    <div class="size-full bg-[#4285F4]/15 border border-[#4285F4]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#4285F4]">
        <path d="M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z" />
      </svg>
    </div>
  {:else if cleanId.includes("obsidian")}
    <!-- Obsidian SVG -->
    <div class="size-full bg-[#7C3AED]/20 border border-[#7C3AED]/50 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#A78BFA]">
        <path d="M12 2L3 9v6l9 7 9-7V9l-9-7zm0 3.2L18.4 9 12 12.8 5.6 9 12 5.2z" />
      </svg>
    </div>

  <!-- DESIGN -->
  {:else if cleanId.includes("figma")}
    <!-- Figma SVG -->
    <div class="size-full bg-[#F24E1E]/10 border border-[#F24E1E]/30 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#0ACF83" d="M12 24a4 4 0 004-4v-4h-4a4 4 0 000 8z" />
        <path fill="#A259FF" d="M4 20a4 4 0 014-4h4v4a4 4 0 01-8 0z" />
        <path fill="#F24E1E" d="M4 12a4 4 0 014-4h4v8H8a4 4 0 01-4-4z" />
        <path fill="#FF7262" d="M4 4a4 4 0 014-4h4v8H8a4 4 0 01-4-4z" />
        <path fill="#1ABCFE" d="M12 0h4a4 4 0 010 8h-4V0z" />
      </svg>
    </div>
  {:else if cleanId.includes("canva")}
    <!-- Canva SVG -->
    <div class="size-full bg-[#00C4CC]/15 border border-[#00C4CC]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#00C4CC]">
        <path d="M12 0C5.37 0 0 5.37 0 12s5.37 12 12 12 12-5.37 12-12S18.63 0 12 0zm3.5 16.5c-2.4 0-4.5-1.6-4.5-4.5 0-2.8 2.1-4.5 4.5-4.5 1.5 0 2.8.7 3.5 1.8l-1.6 1.2c-.5-.7-1.1-1.1-1.9-1.1-1.4 0-2.4 1.2-2.4 2.6s1 2.6 2.4 2.6c.8 0 1.4-.4 1.9-1.1l1.6 1.2c-.7 1.1-2 1.8-3.5 1.8z" />
      </svg>
    </div>

  <!-- BUSINESS & FINANCE -->
  {:else if cleanId.includes("stripe")}
    <!-- Stripe SVG -->
    <div class="size-full bg-[#635BFF]/15 border border-[#635BFF]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#635BFF]">
        <path d="M13.976 9.15c-2.172-.806-3.356-1.426-3.356-2.409 0-.831.683-1.305 1.901-1.305 2.227 0 4.515.858 6.09 1.631l.89-5.494C18.252.97 15.654.5 12.52.5 6.057.5 1.7 3.868 1.7 9.544c0 6.643 7.842 6.84 7.842 9.61 0 .977-.858 1.48-2.227 1.48-2.296 0-5.112-1.026-6.945-2.052L0 23.5c2.196 1.07 5.753 1.67 9.387 1.67 6.84 0 11.238-3.238 11.238-9.088 0-6.841-6.649-6.932-6.649-6.932z" />
      </svg>
    </div>
  {:else if cleanId.includes("shopify")}
    <!-- Shopify SVG -->
    <div class="size-full bg-[#95BF47]/15 border border-[#95BF47]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#95BF47]">
        <path d="M21.2 5.6l-1.9-.6c-.2-.1-.4 0-.5.1l-2.4 2.4-1.2-2.9c-.1-.2-.3-.3-.5-.3h-.1L12.5 5 11 3.5c-.3-.3-.8-.3-1.1 0L3.1 10.3c-.2.2-.2.5-.1.7l3.6 11.2c.1.3.4.5.7.5h9.4c.3 0 .6-.2.7-.5l3.6-11.2c.1-.2 0-.5-.1-.7L19 8.2l2.2-2.1c.2-.2.2-.4 0-.5z" />
      </svg>
    </div>
  {:else if cleanId.includes("hubspot")}
    <!-- HubSpot SVG -->
    <div class="size-full bg-[#FF7A59]/15 border border-[#FF7A59]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FF7A59]">
        <path d="M17.6 7.2V5.1c.8-.4 1.4-1.3 1.4-2.3 0-1.4-1.1-2.5-2.5-2.5S14 1.4 14 2.8c0 1 .6 1.9 1.4 2.3v2.1c-.8.3-1.4.9-1.8 1.6l-4.7-3.6c.1-.4.1-.7.1-1.1 0-2.2-1.8-4-4-4S1 1.9 1 4.1s1.8 4 4 4c.6 0 1.2-.1 1.7-.4l4.6 3.6c-.3.7-.4 1.4-.4 2.2 0 .8.2 1.6.5 2.3l-3.2 3.2c-.4-.2-.8-.3-1.2-.3-1.7 0-3 1.3-3 3s1.3 3 3 3 3-1.3 3-3c0-.4-.1-.8-.3-1.2l3.2-3.2c.7.3 1.5.5 2.3.5 2.8 0 5-2.2 5-5 0-1.4-.6-2.7-1.6-3.6z" />
      </svg>
    </div>
  {:else if cleanId.includes("paypal")}
    <!-- PayPal SVG -->
    <div class="size-full bg-[#003087]/15 border border-[#003087]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#0079C1]">
        <path d="M7.076 21.337H2.47a.641.641 0 0 1-.633-.74L4.944.901C5.004.535 5.32.267 5.69.267h6.634c3.484 0 6.002 1.636 5.485 5.32-.486 3.46-2.923 5.46-6.079 5.46H8.922l-1.328 8.44c-.062.383-.396.65-.78.65h-.002l.264-1.666z" />
      </svg>
    </div>
  {:else if cleanId.includes("coingecko")}
    <!-- CoinGecko Gecko SVG -->
    <div class="size-full bg-[#8DC63F]/15 border border-[#8DC63F]/40 flex items-center justify-center rounded-md text-[#8DC63F]">
      <svg viewBox="0 0 24 24" class="size-full fill-[#8DC63F]">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 14h-2v-2h2v2zm0-4h-2V7h2v5z" />
      </svg>
    </div>
  {:else if cleanId.includes("etherscan") || cleanId.includes("alchemy")}
    <!-- Ethereum / Web3 Diamond SVG -->
    <div class="size-full bg-[#627EEA]/15 border border-[#627EEA]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#627EEA]">
        <path d="M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0zm.04 3.737l5.31 8.813-5.31 2.39-5.31-2.39 5.31-8.813zm0 12.302l5.31-3.085-5.31 7.309-5.31-7.309 5.31 3.085z" />
      </svg>
    </div>
  {:else if cleanId.includes("yfinance") || cleanId.includes("yahoo")}
    <!-- Yahoo Finance SVG -->
    <div class="size-full bg-[#6001D2]/20 border border-[#6001D2]/50 flex items-center justify-center rounded-md text-[#A855F7]">
      <TrendingUp class="size-full" />
    </div>

  <!-- SOCIAL -->
  {:else if cleanId.includes("telegram")}
    <!-- Telegram SVG -->
    <div class="size-full bg-[#26A5E4]/15 border border-[#26A5E4]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#26A5E4]">
        <path d="M12 0C5.373 0 0 5.373 0 12s5.373 12 12 12 12-5.373 12-12S18.627 0 12 0zm5.562 8.161c-.18.892-1.748 7.42-2.54 10.74-.334 1.405-.85 1.876-1.35 1.922-.98.09-1.722-.703-2.67-1.325-1.485-.973-2.324-1.579-3.766-2.53-1.666-1.098-.586-1.701.364-2.688.248-.258 4.567-4.185 4.65-4.54.01-.047.02-.224-.085-.318-.105-.094-.26-.062-.372-.037-.158.036-2.678 1.705-7.56 5.002-.716.491-1.364.731-1.947.718-.642-.014-1.876-.363-2.794-.662-1.127-.367-1.217-.97-.042-1.428 4.606-2.007 7.68-3.33 9.222-3.968 4.398-1.82 5.311-2.136 5.908-2.146.131-.002.424.031.614.186.16.13.204.307.225.432.022.124.048.407.027.629z" />
      </svg>
    </div>
  {:else if cleanId.includes("whatsapp")}
    <!-- WhatsApp SVG -->
    <div class="size-full bg-[#25D366]/15 border border-[#25D366]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#25D366]">
        <path d="M.057 24l1.687-6.163c-1.041-1.804-1.588-3.849-1.587-5.946.003-6.556 5.338-11.891 11.893-11.891 3.181.001 6.167 1.24 8.413 3.488 2.245 2.248 3.481 5.236 3.48 8.414-.003 6.557-5.338 11.892-11.893 11.892-1.99-.001-3.951-.5-5.688-1.448L.057 24zm6.597-3.807c1.676.995 3.276 1.591 5.392 1.592 5.448 0 9.886-4.434 9.889-9.885.002-5.462-4.415-9.89-9.881-9.892-5.452 0-9.887 4.434-9.889 9.884-.001 2.225.651 3.891 1.746 5.634l-.999 3.648 3.742-.981z" />
      </svg>
    </div>
  {:else if cleanId.includes("twitter") || cleanId.includes("x")}
    <!-- X / Twitter SVG -->
    <div class="size-full bg-white/10 border border-white/20 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
      </svg>
    </div>
  {:else if cleanId.includes("reddit")}
    <!-- Reddit SVG -->
    <div class="size-full bg-[#FF4500]/15 border border-[#FF4500]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#FF4500]">
        <path d="M12 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0zm5.01 4.744c.688 0 1.25.561 1.25 1.249a1.25 1.25 0 0 1-2.498.056l-2.597-.547-.8 3.747c1.824.07 3.48.632 4.674 1.488.308-.309.73-.491 1.207-.491.968 0 1.754.786 1.754 1.754 0 .716-.435 1.333-1.01 1.614a3.111 3.111 0 0 1 .042.52c0 2.694-3.13 4.87-7.004 4.87-3.874 0-7.004-2.176-7.004-4.87 0-.183.015-.366.043-.534A1.748 1.748 0 0 1 4.028 12c0-.968.786-1.754 1.754-1.754.463 0 .868.196 1.171.49 1.186-.86 2.842-1.424 4.662-1.498l.966-4.526 3.429.722c.005.001.01.002.015.002a1.249 1.249 0 0 1 .985-.492z" />
      </svg>
    </div>
  {:else if cleanId.includes("resend") || cleanId.includes("sendgrid")}
    <!-- Resend / Email SVG -->
    <div class="size-full bg-white/10 border border-white/20 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M22 6c0-1.1-.9-2-2-2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6zm-2 0l-8 5-8-5h16zm0 12H4V8l8 5 8-5v10z" />
      </svg>
    </div>

  <!-- SECURITY & SYSTEM -->
  {:else if cleanId.includes("datadog")}
    <!-- Datadog SVG -->
    <div class="size-full bg-[#632CA6]/20 border border-[#632CA6]/50 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#632CA6]">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z" />
      </svg>
    </div>
  {:else if cleanId.includes("grafana")}
    <!-- Grafana SVG -->
    <div class="size-full bg-[#F46800]/15 border border-[#F46800]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#F46800]">
        <path d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm1 14.93V15h-2v1.93A8 8 0 0 1 4.07 13H6v-2H4.07A8 8 0 0 1 11 4.07V6h2V4.07A8 8 0 0 1 19.93 11H18v2h1.93A8 8 0 0 1 13 16.93z" />
      </svg>
    </div>
  {:else if cleanId.includes("vault") || cleanId.includes("onepassword") || cleanId.includes("bitwarden")}
    <!-- Vault / 1Password / Security Shield SVG -->
    <div class="size-full bg-[#0A85EA]/20 border border-[#0A85EA]/40 flex items-center justify-center rounded-md text-[#0A85EA]">
      <Lock class="size-full" />
    </div>
  {:else if cleanId.includes("tailscale")}
    <!-- Tailscale Mesh SVG -->
    <div class="size-full bg-white/10 border border-white/20 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <circle cx="6" cy="6" r="3" />
        <circle cx="18" cy="6" r="3" />
        <circle cx="12" cy="12" r="3" />
        <circle cx="6" cy="18" r="3" />
        <circle cx="18" cy="18" r="3" />
      </svg>
    </div>
  {:else if cleanId.includes("home_assistant")}
    <!-- Home Assistant Cyan House SVG -->
    <div class="size-full bg-[#03A9F4]/15 border border-[#03A9F4]/40 flex items-center justify-center rounded-md text-[#03A9F4]">
      <Home class="size-full" />
    </div>
  {:else if cleanId.includes("shell") || cleanId.includes("ssh") || cleanId.includes("terminal")}
    <!-- Terminal Shell SVG -->
    <div class="size-full bg-emerald-950/40 border border-emerald-500/40 flex items-center justify-center rounded-md text-emerald-400">
      <Terminal class="size-full" />
    </div>
  {:else if cleanId.includes("wolfram")}
    <!-- Wolfram Alpha Red Spikey SVG -->
    <div class="size-full bg-[#DD1100]/15 border border-[#DD1100]/40 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-[#DD1100]">
        <path d="M12 2l2.4 5.2 5.6.8-4 4 1 5.6-5-2.6-5 2.6 1-5.6-4-4 5.6-.8z" />
      </svg>
    </div>
  {:else if cleanId.includes("wikipedia")}
    <!-- Wikipedia W SVG -->
    <div class="size-full bg-white/15 border border-white/30 flex items-center justify-center rounded-md">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M12.09 13.62l2.67-7.2h2.09l-4.08 10.36h-1.36L8.73 9.42l-2.68 7.36H4.69L.61 6.42h2.09l2.67 7.2 2.68-7.2h1.36l2.68 7.2z" />
      </svg>
    </div>
  {:else}
    <!-- Clean Fallback Vector Icon Container -->
    <div class="size-full bg-purple-950/30 border border-purple-500/30 flex items-center justify-center rounded-md text-purple-300">
      <Layers class="size-full" />
    </div>
  {/if}
</div>
