<script lang="ts">
  interface Props {
    id: string;
    name?: string;
    logoUrl?: string;
    class?: string;
    size?: "sm" | "md" | "lg";
  }

  let { id = "", name = "", logoUrl = "", class: customClass = "", size = "md" }: Props = $props();

  let cleanId = $derived(id.toLowerCase().replace(/[^a-z0-9]/g, ""));
  let cleanName = $derived(name.toLowerCase());

  let sizeClasses = $derived(
    size === "sm" ? "size-6 text-[10px]" : size === "lg" ? "size-12 text-sm" : "size-10 text-xs"
  );
</script>

<div class="relative flex items-center justify-center rounded-xl overflow-hidden shrink-0 shadow-sm transition-all {sizeClasses} {customClass}">
  {#if cleanId.includes("gmail") || cleanName.includes("gmail")}
    <!-- Official Gmail Vector Logo -->
    <div class="size-full bg-white p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#4285F4" d="M22 6c0-1.1-.9-2-2-2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6z" opacity="0.1" />
        <path fill="#EA4335" d="M20 18h2V6c0-1.1-.9-2-2-2L12 11 4 4C2.9 4 2 4.9 2 6v12h2V8l8 6 8-6v10z" />
        <path fill="#4285F4" d="M4 18V8l8 6L4 8z" />
        <path fill="#34A853" d="M20 18V8l-8 6 8-6z" />
        <path fill="#FBBC05" d="M4 6L2 8v10h2V8z" />
        <path fill="#EA4335" d="M20 6l2 2v10h-2V8z" />
      </svg>
    </div>
  {:else if cleanId.includes("googlecalendar") || cleanName.includes("calendar")}
    <!-- Official Google Calendar Vector Logo -->
    <div class="size-full bg-white p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full">
        <rect width="20" height="20" x="2" y="2" fill="#4285F4" rx="4" />
        <rect width="14" height="14" x="5" y="6" fill="#FFFFFF" rx="2" />
        <text x="12" y="17" fill="#4285F4" font-size="9" font-family="sans-serif" font-weight="bold" text-anchor="middle">31</text>
        <rect width="2" height="3" x="7" y="1" fill="#EA4335" rx="1" />
        <rect width="2" height="3" x="15" y="1" fill="#EA4335" rx="1" />
      </svg>
    </div>
  {:else if cleanId.includes("googledrive") || cleanName.includes("drive")}
    <!-- Official Google Drive Vector Logo -->
    <div class="size-full bg-white p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#0066DA" d="M8.01 19.5h12.98l-4.49-7.79H3.52z" />
        <path fill="#00AC47" d="M3.52 11.71l6.49-11.21h6.49L9.99 11.71z" />
        <path fill="#FFBA00" d="M16.5 0.5l6.49 11.21-6.49 7.79-6.49-7.79z" />
      </svg>
    </div>
  {:else if cleanId.includes("github") || cleanName.includes("github")}
    <!-- Official GitHub Vector Logo -->
    <div class="size-full bg-[#181717] p-1.5 flex items-center justify-center rounded-xl border border-white/20 shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
      </svg>
    </div>
  {:else if cleanId.includes("slack") || cleanName.includes("slack")}
    <!-- Official Slack Vector Logo -->
    <div class="size-full bg-[#4A154B] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#E01E5A" d="M5.04 14.28a2.52 2.52 0 1 1-2.52-2.52h2.52v2.52zm1.26 0a2.52 2.52 0 0 1 5.04 0v6.3a2.52 2.52 0 1 1-5.04 0v-6.3z" />
        <path fill="#36C5F0" d="M9.72 5.04a2.52 2.52 0 1 1 2.52-2.52v2.52H9.72zm0 1.26a2.52 2.52 0 0 1 0 5.04H3.42a2.52 2.52 0 1 1 0-5.04h6.3z" />
        <path fill="#2EB67D" d="M18.96 9.72a2.52 2.52 0 1 1 2.52 2.52h-2.52V9.72zm-1.26 0a2.52 2.52 0 0 1-5.04 0V3.42a2.52 2.52 0 1 1 5.04 0v6.3z" />
        <path fill="#ECB22E" d="M14.28 18.96a2.52 2.52 0 1 1-2.52 2.52v-2.52h2.52zm0-1.26a2.52 2.52 0 0 1 0-5.04h6.3a2.52 2.52 0 1 1 0 5.04h-6.3z" />
      </svg>
    </div>
  {:else if cleanId.includes("notion") || cleanName.includes("notion")}
    <!-- Official Notion Vector Logo -->
    <div class="size-full bg-white p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-black">
        <path d="M4.459 4.208c.746.606 1.026.56 2.428.466l11.455-.746c.42 0 .093-.327-.14-.467L15.932 1.83a2.49 2.49 0 0 0-1.54-.467L2.454 2.296c-.466.046-.56.28-.373.466l2.378 1.446zm-.42 2.705v13.62c0 .746.373 1.026 1.213.98l13.606-.84c.84-.047 1.027-.56 1.027-1.12V5.795c0-.606-.234-.886-.84-.84l-14.166.886c-.606.047-.84.373-.84.98v.092zm12.35 1.12l.14 10.312c0 .42-.14.606-.606.653l-2.006.14c-.467 0-.607-.233-.607-.606V9.897L8.796 17.873c-.234.373-.514.42-.84.42l-1.913.093c-.467 0-.607-.28-.607-.653V8.125c0-.42.187-.653.607-.653h1.866c.42 0 .606.233.606.653v7.417l4.385-7.37c.28-.42.56-.56.98-.56h2.006c.42 0 .56.187.56.56v.093z" />
      </svg>
    </div>
  {:else if cleanId.includes("linear") || cleanName.includes("linear")}
    <!-- Official Linear Vector Logo -->
    <div class="size-full bg-[#5E6AD2] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M2.5 12a9.5 9.5 0 1 1 19 0 9.5 9.5 0 0 1-19 0zm10.7-5.5a6.5 6.5 0 0 0-6.2 6.5c0 .7.1 1.4.3 2l8.9-8.9c-.9-.4-1.9-.6-3-.6zm4.4 3.5l-8.9 8.9c.8.4 1.7.6 2.7.6a6.5 6.5 0 0 0 6.2-6.5c0-1.1-.4-2.2-1-3z" />
      </svg>
    </div>
  {:else if cleanId.includes("discord") || cleanName.includes("discord")}
    <!-- Official Discord Vector Logo -->
    <div class="size-full bg-[#5865F2] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994.021-.041.001-.09-.041-.106a13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.929 1.793 8.18 1.793 12.061 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.894.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.028zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
      </svg>
    </div>
  {:else if cleanId.includes("stripe") || cleanName.includes("stripe")}
    <!-- Official Stripe Vector Logo -->
    <div class="size-full bg-[#635BFF] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M13.976 9.15c-2.172-.806-3.356-1.426-3.356-2.409 0-.831.683-1.305 1.901-1.305 2.227 0 4.515.858 6.09 1.631l.89-5.494C18.252.975 15.697 0 12.165 0 9.667 0 7.589.654 6.104 1.872 4.56 3.147 3.757 4.992 3.757 7.218c0 4.039 2.467 5.76 6.476 7.219 2.585.92 3.445 1.574 3.445 2.583 0 .98-.84 1.545-2.354 1.545-1.875 0-4.965-.921-6.99-2.109l-.9 5.555C5.175 23.018 8.01 24 11.517 24c2.614 0 4.753-.632 6.273-1.789 1.605-1.226 2.453-3.118 2.453-5.463 0-4.143-2.52-5.76-6.267-7.598z" />
      </svg>
    </div>
  {:else if cleanId.includes("figma") || cleanName.includes("figma")}
    <!-- Official Figma Vector Logo -->
    <div class="size-full bg-[#1E1E1E] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#F24E1E" d="M8 2h4v5H8a2.5 2.5 0 1 1 0-5z" />
        <path fill="#FF7262" d="M12 2h4a2.5 2.5 0 0 1 0 5h-4V2z" />
        <path fill="#1ABCFE" d="M12 7h4a2.5 2.5 0 0 1 0 5h-4V7z" />
        <path fill="#0ACF83" d="M8 12h4v5H8a2.5 2.5 0 0 1 0-5z" />
        <path fill="#A259FF" d="M8 7h4v5H8a2.5 2.5 0 0 1 0-5z" />
      </svg>
    </div>
  {:else if cleanId.includes("jira") || cleanName.includes("jira")}
    <!-- Official Jira Vector Logo -->
    <div class="size-full bg-[#0052CC] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M11.53 2c0 2.4-1.97 4.35-4.4 4.35H4.88V8.6c0 2.4-1.97 4.35-4.4 4.35H0V2h11.53zM18.76 7.67c0 2.4-1.97 4.35-4.4 4.35h-2.25v2.25c0 2.4-1.97 4.35-4.4 4.35H7.23V7.67h11.53zM24 13.33c0 2.4-1.97 4.35-4.4 4.35h-2.25v2.25c0 2.4-1.97 4.35-4.4 4.35h-.48V13.33H24z" />
      </svg>
    </div>
  {:else if cleanId.includes("hubspot") || cleanName.includes("hubspot")}
    <!-- Official HubSpot Vector Logo -->
    <div class="size-full bg-[#FF7A59] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M18.5 7.5a2.5 2.5 0 0 0-2.16 1.25l-4.59-2.3V4.5a2.5 2.5 0 1 0-2.5 2.5v1.45l-4.59 2.3A2.5 2.5 0 1 0 4.5 13.5a2.5 2.5 0 0 0 1.25-.34l2.3 4.59a2.5 2.5 0 1 0 2.45-.75l-2.3-4.59a2.5 2.5 0 0 0-.2-1.41l4.59-2.3a2.5 2.5 0 0 0 1.41.34 2.5 2.5 0 0 0 2.5-2.5 2.5 2.5 0 0 0 2-2.46z" />
      </svg>
    </div>
  {:else if cleanId.includes("salesforce") || cleanName.includes("salesforce")}
    <!-- Official Salesforce Vector Logo -->
    <div class="size-full bg-[#00A1E0] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M19.34 8.65a4.7 4.7 0 0 0-4.14-2.53 4.8 4.8 0 0 0-2.82.91A6.2 6.2 0 0 0 6.64 6a6.3 6.3 0 0 0-6.14 5.3 4.6 4.6 0 0 0 3.25 4.37 5.6 5.6 0 0 0 4.65.13 4.8 4.8 0 0 0 4.2 2.2 4.9 4.9 0 0 0 4.33-2.6 4.3 4.3 0 0 0 2.41-3.75 4.3 4.3 0 0 0-.01-.01z" />
      </svg>
    </div>
  {:else if cleanId.includes("trello") || cleanName.includes("trello")}
    <!-- Official Trello Vector Logo -->
    <div class="size-full bg-[#0079BF] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <rect x="2" y="2" width="20" height="20" rx="3" fill="#0079BF" />
        <rect x="5" y="5" width="6" height="11" rx="1.5" fill="#FFFFFF" />
        <rect x="13" y="5" width="6" height="7" rx="1.5" fill="#FFFFFF" />
      </svg>
    </div>
  {:else if cleanId.includes("asana") || cleanName.includes("asana")}
    <!-- Official Asana Vector Logo -->
    <div class="size-full bg-[#F06A6A] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <circle cx="12" cy="7" r="4" />
        <circle cx="6" cy="17" r="4" />
        <circle cx="18" cy="17" r="4" />
      </svg>
    </div>
  {:else if cleanId.includes("supabase") || cleanName.includes("supabase")}
    <!-- Official Supabase Vector Logo -->
    <div class="size-full bg-[#1C1C1C] p-1.5 flex items-center justify-center rounded-xl border border-[#3ECF8E]/30 shadow">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#3ECF8E" d="M11.6 1.2c.4-.6 1.4-.4 1.5.3l1.8 9.2h6.7c.8 0 1.2 1 .7 1.6l-9.8 11c-.5.6-1.5.3-1.5-.4l-1.8-9.2H2.5c-.8 0-1.2-1-.7-1.6l9.8-10.9z" />
      </svg>
    </div>
  {:else if cleanId.includes("telegram") || cleanName.includes("telegram")}
    <!-- Official Telegram Vector Logo -->
    <div class="size-full bg-[#24A1DE] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M12 0C5.373 0 0 5.373 0 12s5.373 12 12 12 12-5.373 12-12S18.627 0 12 0zm5.894 8.221l-1.97 9.28c-.145.658-.537.818-1.084.508l-3-2.21-1.446 1.394c-.16.16-.295.295-.605.295l.213-3.053 5.56-5.023c.242-.213-.054-.333-.373-.121l-6.871 4.326-2.962-.924c-.643-.204-.657-.643.136-.953l11.57-4.458c.536-.196 1.006.128.832.943z" />
      </svg>
    </div>
  {:else if cleanId.includes("twitter") || cleanName.includes("twitter") || cleanId.includes("x")}
    <!-- Official X / Twitter Vector Logo -->
    <div class="size-full bg-black p-1.5 flex items-center justify-center rounded-xl border border-white/20 shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
      </svg>
    </div>
  {:else if cleanId.includes("youtube") || cleanName.includes("youtube")}
    <!-- Official YouTube Vector Logo -->
    <div class="size-full bg-[#FF0000] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z" />
      </svg>
    </div>
  {:else if cleanId.includes("aws") || cleanName.includes("aws")}
    <!-- Official AWS Vector Logo -->
    <div class="size-full bg-[#232F3E] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full">
        <path fill="#FF9900" d="M18.7 17.5c-3.7 2.4-8.8 2.6-12.7.5-.2-.1-.2-.4 0-.5.8-.8 1.8-1.5 2.8-2.1.2-.1.5 0 .6.2 2.7 1.7 6.3 1.6 9-.2.2-.2.5-.1.6.1l.4.5c.2.2.1.5-.1.6z" />
        <path fill="#FF9900" d="M19.4 15.6c-.3-.4-1.8-.8-2.8-.7-.3 0-.4-.2-.2-.4 1-1.3 3.1-1.2 3.4-.9.3.3.3 2.5-.7 3.6-.2.2-.4.1-.3-.1.2-.5.8-1.2.6-1.5z" />
        <text x="12" y="12" fill="#FFFFFF" font-size="7" font-family="sans-serif" font-weight="900" text-anchor="middle">AWS</text>
      </svg>
    </div>
  {:else if cleanId.includes("zoom") || cleanName.includes("zoom")}
    <!-- Official Zoom Vector Logo -->
    <div class="size-full bg-[#2D8CFF] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <path d="M4.5 7.5A2.5 2.5 0 0 1 7 5h7a2.5 2.5 0 0 1 2.5 2.5v9A2.5 2.5 0 0 1 14 19H7a2.5 2.5 0 0 1-2.5-2.5v-9zm12.5 2.2l4.2-3.1a1 1 0 0 1 1.6.8v9.2a1 1 0 0 1-1.6.8L17 14.3V9.7z" />
      </svg>
    </div>
  {:else if cleanId.includes("outlook") || cleanName.includes("outlook")}
    <!-- Official Outlook Vector Logo -->
    <div class="size-full bg-[#0078D4] p-1.5 flex items-center justify-center rounded-xl shadow">
      <svg viewBox="0 0 24 24" class="size-full fill-white">
        <rect x="2" y="4" width="14" height="16" rx="2" fill="#0078D4" />
        <path d="M16 7l6 3v7l-6 3V7z" fill="#005A9E" />
        <text x="9" y="15" fill="#FFFFFF" font-size="9" font-family="sans-serif" font-weight="bold" text-anchor="middle">O</text>
      </svg>
    </div>
  {:else}
    <!-- Generic Fallback with Beautiful Neon Initial Badge -->
    <div class="size-full bg-gradient-to-br from-purple-900/60 to-indigo-900/60 border border-purple-500/40 p-1 flex items-center justify-center rounded-xl font-mono font-bold text-white shadow">
      <span>{(name || id || "PL").slice(0, 2).toUpperCase()}</span>
    </div>
  {/if}
</div>
