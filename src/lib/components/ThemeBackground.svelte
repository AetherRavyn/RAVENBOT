<script lang="ts">
  import type { ThemeDefinition } from "$lib/theme";

  interface Props {
    theme: ThemeDefinition;
  }

  let { theme }: Props = $props();
</script>

<div class="absolute inset-0 pointer-events-none overflow-hidden select-none">
  {#if theme.id === "grok-sovereign"}
    <!-- Grok Sovereign: Pure AMOLED Minimalist Ambient Atmosphere -->
    <div class="absolute inset-0 bg-[radial-gradient(ellipse_at_top,_var(--tw-gradient-stops))] from-zinc-800/15 via-black to-black"></div>
    <div class="absolute top-1/6 left-1/2 -translate-x-1/2 w-[600px] h-[350px] bg-sky-500/5 blur-[120px] rounded-full pointer-events-none"></div>

    <!-- Fine Geometric Minimalist Horizon Grid -->
    <svg class="absolute inset-0 w-full h-full opacity-10" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <pattern id="grok-grid" width="48" height="48" patternUnits="userSpaceOnUse">
          <path d="M 48 0 L 0 0 0 48" fill="none" stroke="rgba(255, 255, 255, 0.4)" stroke-width="0.5" />
          <circle cx="48" cy="0" r="1" fill="#38bdf8" />
        </pattern>
      </defs>
      <rect width="100%" height="100%" fill="url(#grok-grid)" />
    </svg>
  {:else if theme.id === "the-rot-archive"}
    <!-- The Rot Archive: Occult Transmutation Circle, Runic Marginalia & Alchemical Smoke -->
    <div class="absolute inset-0 bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-[#8B1E1E]/20 via-[#18130f]/60 to-transparent"></div>
    <div class="absolute top-1/4 right-1/4 size-96 rounded-full bg-[#8B1E1E]/15 blur-3xl"></div>
    <div class="absolute bottom-10 left-1/3 size-80 rounded-full bg-[#2D3F31]/20 blur-3xl"></div>

    <!-- Alchemical Geometric Transmutation Seal -->
    <svg class="absolute left-1/2 top-1/3 -translate-x-1/2 -translate-y-1/2 w-[740px] h-[740px] opacity-40" viewBox="0 0 740 740" fill="none">
      <!-- Concentric Runic Rings -->
      <circle cx="370" cy="370" r="330" stroke="#8B1E1E" stroke-width="2" stroke-dasharray="14 6" opacity="0.7" />
      <circle cx="370" cy="370" r="305" stroke="#C8B89B" stroke-width="1.5" opacity="0.5" />
      <circle cx="370" cy="370" r="250" stroke="#5C3B2E" stroke-width="2" stroke-dasharray="6 6" opacity="0.6" />
      <circle cx="370" cy="370" r="170" stroke="#8B1E1E" stroke-width="1.5" opacity="0.6" />
      <circle cx="370" cy="370" r="85" stroke="#2D3F31" stroke-width="2" stroke-dasharray="8 8" opacity="0.7" />

      <!-- Sacred Alchemical Triangles & Pentacle Lines -->
      <polygon points="370,65 635,525 105,525" stroke="#C8B89B" stroke-width="1.5" opacity="0.3" />
      <polygon points="370,675 635,215 105,215" stroke="#8B1E1E" stroke-width="1.5" opacity="0.3" />
      <circle cx="370" cy="370" r="130" stroke="#C8B89B" stroke-width="1" stroke-dasharray="2 4" opacity="0.4" />

      <!-- Occult Alignment Rays -->
      <line x1="370" y1="40" x2="370" y2="700" stroke="#C8B89B" stroke-width="1" stroke-dasharray="3 6" opacity="0.35" />
      <line x1="40" y1="370" x2="700" y2="370" stroke="#C8B89B" stroke-width="1" stroke-dasharray="3 6" opacity="0.35" />

      <!-- Ancient Alchemical Runic Glyphs -->
      <text x="370" y="52" fill="#C8B89B" font-size="16" font-family="serif" text-anchor="middle" opacity="0.9">🜍 ☿ 🜂</text>
      <text x="685" y="375" fill="#8B1E1E" font-size="16" font-family="serif" text-anchor="middle" opacity="0.9">🜄</text>
      <text x="370" y="715" fill="#C8B89B" font-size="16" font-family="serif" text-anchor="middle" opacity="0.9">🜁 🜃 🝢</text>
      <text x="55" y="375" fill="#2D3F31" font-size="16" font-family="serif" text-anchor="middle" opacity="0.9">🜔</text>
    </svg>

    <!-- The Rot Archive Right Atmospheric Grimoire Artwork -->
    <div class="absolute right-0 top-0 bottom-0 w-[55%] flex items-center justify-end pointer-events-none select-none overflow-hidden opacity-30 mix-blend-screen">
      <img src="/hero.png" alt="Grimoire Hero" class="h-full w-auto max-w-full object-cover object-left sepia hue-rotate-[320deg]" />
    </div>

  {:else if theme.id === "cyber-cyan"}
    <!-- Cyber Cyan: Holographic Cyberspace Grid & Quantum Telemetry Streams -->
    <div class="absolute inset-0 bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-cyan-900/25 via-sky-950/20 to-transparent"></div>
    <div class="absolute top-1/4 right-1/4 size-96 rounded-full bg-cyan-500/15 blur-3xl"></div>

    <!-- Holographic Hex Grid & Concentric Quantum Radar -->
    <svg class="absolute left-1/2 top-1/3 -translate-x-1/2 -translate-y-1/2 w-[720px] h-[720px] opacity-35" viewBox="0 0 720 720" fill="none">
      <polygon points="360,50 630,205 630,515 360,670 90,515 90,205" stroke="#06b6d4" stroke-width="1.5" stroke-dasharray="10 5" opacity="0.6" />
      <polygon points="360,110 575,235 575,485 360,610 145,485 145,235" stroke="#38bdf8" stroke-width="1" opacity="0.4" />
      <circle cx="360" cy="360" r="180" stroke="#06b6d4" stroke-width="2" stroke-dasharray="8 8" opacity="0.5" />
      <circle cx="360" cy="360" r="90" stroke="#38bdf8" stroke-width="1.5" opacity="0.6" />
      <!-- Digital Axis Crosshair Lines -->
      <line x1="360" y1="20" x2="360" y2="700" stroke="#06b6d4" stroke-width="1" stroke-dasharray="4 4" opacity="0.3" />
      <line x1="20" y1="360" x2="700" y2="360" stroke="#06b6d4" stroke-width="1" stroke-dasharray="4 4" opacity="0.3" />
    </svg>

    <div class="absolute right-0 top-0 bottom-0 w-[55%] flex items-center justify-end pointer-events-none select-none overflow-hidden opacity-35 mix-blend-screen">
      <img src="/hero.png" alt="Cyber Hero" class="h-full w-auto max-w-full object-cover object-left hue-rotate-180" />
    </div>

  {:else if theme.id === "emerald-matrix"}
    <!-- Emerald Matrix: Phosphor Terminal Mainframe & Scanlines -->
    <div class="absolute inset-0 bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-emerald-950/30 via-[#031508]/40 to-transparent"></div>
    <div class="absolute top-1/4 right-1/4 size-96 rounded-full bg-emerald-600/15 blur-3xl"></div>

    <!-- Terminal Mainframe Matrix Grid -->
    <svg class="absolute left-1/2 top-1/3 -translate-x-1/2 -translate-y-1/2 w-[700px] h-[700px] opacity-35" viewBox="0 0 700 700" fill="none">
      <rect x="50" y="50" width="600" height="600" stroke="#10b981" stroke-width="1.5" stroke-dasharray="8 8" opacity="0.5" />
      <rect x="120" y="120" width="460" height="460" stroke="#34d399" stroke-width="1" opacity="0.3" />
      <circle cx="350" cy="350" r="160" stroke="#10b981" stroke-width="2" stroke-dasharray="4 4" opacity="0.6" />
      <circle cx="350" cy="350" r="70" stroke="#34d399" stroke-width="1.5" opacity="0.5" />
      <!-- Phosphor Grid Crossbars -->
      <line x1="350" y1="20" x2="350" y2="680" stroke="#10b981" stroke-width="1" opacity="0.4" />
      <line x1="20" y1="350" x2="680" y2="350" stroke="#10b981" stroke-width="1" opacity="0.4" />
    </svg>

    <div class="absolute right-0 top-0 bottom-0 w-[55%] flex items-center justify-end pointer-events-none select-none overflow-hidden opacity-30 mix-blend-screen">
      <img src="/hero.png" alt="Matrix Hero" class="h-full w-auto max-w-full object-cover object-left hue-rotate-90" />
    </div>

  {:else if theme.id === "crimson-sovereign"}
    <!-- Crimson Sovereign: Combat Vanguard War-Room Grid -->
    <div class="absolute inset-0 bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-rose-950/30 via-red-950/20 to-transparent"></div>
    <div class="absolute top-1/4 right-1/4 size-96 rounded-full bg-rose-600/15 blur-3xl"></div>

    <svg class="absolute left-1/2 top-1/3 -translate-x-1/2 -translate-y-1/2 w-[700px] h-[700px] opacity-35" viewBox="0 0 700 700" fill="none">
      <path d="M350,50 L650,150 L650,450 Q650,620 350,670 Q50,620 50,450 L50,150 Z" stroke="#f43f5e" stroke-width="1.5" stroke-dasharray="10 5" opacity="0.6" />
      <circle cx="350" cy="350" r="160" stroke="#fb7185" stroke-width="1.5" stroke-dasharray="4 4" opacity="0.5" />
      <circle cx="350" cy="350" r="80" stroke="#f43f5e" stroke-width="2" opacity="0.7" />
      <line x1="350" y1="20" x2="350" y2="680" stroke="#f43f5e" stroke-width="1" stroke-dasharray="4 4" opacity="0.3" />
      <line x1="20" y1="350" x2="680" y2="350" stroke="#f43f5e" stroke-width="1" stroke-dasharray="4 4" opacity="0.3" />
    </svg>

    <div class="absolute right-0 top-0 bottom-0 w-[55%] flex items-center justify-end pointer-events-none select-none overflow-hidden opacity-35 mix-blend-screen">
      <img src="/hero.png" alt="Vanguard Hero" class="h-full w-auto max-w-full object-cover object-left hue-rotate-[-30deg]" />
    </div>

  {:else if theme.id === "amber-sunset"}
    <!-- Amber Sunset: Solar Fusion Core & Gold Weave -->
    <div class="absolute inset-0 bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-amber-950/30 via-orange-950/20 to-transparent"></div>
    <div class="absolute top-1/4 right-1/4 size-96 rounded-full bg-amber-500/15 blur-3xl"></div>

    <svg class="absolute left-1/2 top-1/3 -translate-x-1/2 -translate-y-1/2 w-[700px] h-[700px] opacity-35" viewBox="0 0 700 700" fill="none">
      <circle cx="350" cy="350" r="300" stroke="#f59e0b" stroke-width="1.5" stroke-dasharray="12 6" opacity="0.6" />
      <circle cx="350" cy="350" r="220" stroke="#fbbf24" stroke-width="1" opacity="0.4" />
      <circle cx="350" cy="350" r="140" stroke="#f59e0b" stroke-width="2" stroke-dasharray="6 3" opacity="0.6" />
      <circle cx="350" cy="350" r="60" stroke="#fbbf24" stroke-width="2" opacity="0.7" />
      <line x1="350" y1="20" x2="350" y2="680" stroke="#f59e0b" stroke-width="1" stroke-dasharray="4 4" opacity="0.3" />
      <line x1="20" y1="350" x2="680" y2="350" stroke="#f59e0b" stroke-width="1" stroke-dasharray="4 4" opacity="0.3" />
    </svg>

    <div class="absolute right-0 top-0 bottom-0 w-[55%] flex items-center justify-end pointer-events-none select-none overflow-hidden opacity-35 mix-blend-screen">
      <img src="/hero.png" alt="Solar Hero" class="h-full w-auto max-w-full object-cover object-left hue-rotate-[45deg]" />
    </div>

  {:else if theme.id === "onyx-monochrome"}
    <!-- Onyx AMOLED: Pure Black Precision Titanium Crosshairs -->
    <svg class="absolute left-1/2 top-1/3 -translate-x-1/2 -translate-y-1/2 w-[700px] h-[700px] opacity-25" viewBox="0 0 700 700" fill="none">
      <rect x="50" y="50" width="600" height="600" stroke="#e4e4e7" stroke-width="1" stroke-dasharray="8 8" opacity="0.4" />
      <circle cx="350" cy="350" r="200" stroke="#ffffff" stroke-width="1" opacity="0.3" />
      <circle cx="350" cy="350" r="80" stroke="#71717a" stroke-width="1" stroke-dasharray="2 4" opacity="0.5" />
      <line x1="350" y1="0" x2="350" y2="700" stroke="#e4e4e7" stroke-width="1" opacity="0.3" />
      <line x1="0" y1="350" x2="700" y2="350" stroke="#e4e4e7" stroke-width="1" opacity="0.3" />
    </svg>

  {:else}
    <!-- Obsidian Violet (Default) -->
    <div class="absolute -top-24 right-1/4 size-96 rounded-full blur-3xl opacity-20" style="background-color: {theme.primaryColor}"></div>
    <div class="absolute top-1/3 right-0 size-[500px] rounded-full blur-3xl opacity-15" style="background-color: {theme.accentColor}"></div>

    <svg class="absolute left-1/2 top-1/3 -translate-x-1/2 -translate-y-1/2 w-[700px] h-[700px] opacity-25" viewBox="0 0 700 700" fill="none">
      <circle cx="350" cy="350" r="160" stroke={theme.primaryColor} stroke-width="1" stroke-dasharray="4 6" opacity="0.4" />
      <circle cx="350" cy="350" r="230" stroke={theme.accentColor} stroke-width="1" opacity="0.3" />
      <circle cx="350" cy="350" r="300" stroke={theme.primaryColor} stroke-width="1" stroke-dasharray="8 8" opacity="0.2" />
    </svg>

    <div class="absolute right-0 top-0 bottom-0 w-[55%] flex items-center justify-end pointer-events-none select-none overflow-hidden">
      <img src="/hero.png" alt="Raven Hero" class="h-full w-auto max-w-full object-cover object-left opacity-35 mix-blend-screen" />
    </div>
  {/if}
</div>
