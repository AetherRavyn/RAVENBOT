<script lang="ts">
  import type { ThemeDefinition } from "$lib/theme";

  interface Props {
    theme: ThemeDefinition;
    size?: "sm" | "md" | "lg" | "xl";
    class?: string;
  }

  let { theme, size = "md", class: customClass = "" }: Props = $props();

  let sizeClass = $derived(
    size === "sm" ? "size-8" : size === "md" ? "size-10" : size === "lg" ? "size-16" : "size-32"
  );
</script>

<div class="relative flex items-center justify-center shrink-0 {sizeClass} {customClass}">
  {#if theme.brand.logoType === "svg-grok"}
    <!-- Grok Sovereign: Minimalist Geometric Titanium & Cyan Blade Emblem -->
    <svg viewBox="0 0 120 120" class="size-full drop-shadow-[0_0_25px_rgba(56,189,248,0.45)]" fill="none">
      <polygon points="60,6 110,32 110,88 60,114 10,88 10,32" stroke="#27272a" stroke-width="1.5" />
      <polygon points="60,16 100,38 100,82 60,104 20,82 20,38" stroke="rgba(255,255,255,0.15)" stroke-width="1" stroke-dasharray="3 3" />
      <!-- Grok Slash / Raven Geometric Wings -->
      <path d="M60,18 L100,42 L60,102 L20,42 Z" stroke="#ffffff" stroke-width="2.5" stroke-linejoin="round" />
      <path d="M60,30 L88,48 L60,90 L32,48 Z" fill="#000000" stroke="#38bdf8" stroke-width="1.5" stroke-linejoin="round" />
      <polygon points="60,42 76,54 60,78 44,54" fill="#38bdf8" fill-opacity="0.3" stroke="#ffffff" stroke-width="1.5" />
      <circle cx="60" cy="54" r="3.5" fill="#38bdf8" />
    </svg>
  {:else if theme.brand.logoType === "image"}
    <div class="size-full rounded-2xl overflow-hidden shadow-lg border" style="border-color: {theme.primaryColor}80; background-color: {theme.cardHex};">
      <img src={theme.brand.logoImage || "/ravenicon.png"} alt={theme.name} class="size-full object-cover" />
    </div>
  {:else if theme.brand.logoType === "svg-rot"}
    <!-- The Rot Archive: Occult Grimoire Skull & Alchemical Transmutation Sigil -->
    <svg viewBox="0 0 120 120" class="size-full drop-shadow-[0_0_20px_rgba(139,30,30,0.6)]" fill="none">
      <!-- Dark Aged Parchment Seal Base -->
      <polygon points="60,8 108,35 108,85 60,112 12,85 12,35" fill="#16120e" stroke="#5C3B2E" stroke-width="2.5" />
      <polygon points="60,16 100,39 100,81 60,104 20,81 20,39" stroke="#8B1E1E" stroke-width="1.5" stroke-dasharray="3 3" />
      <!-- Inscribed Inverted Occult Triangle -->
      <polygon points="60,96 28,42 92,42" stroke="#C8B89B" stroke-width="1.5" opacity="0.85" />
      <!-- Center Bone Skull / Occult Eye Icon -->
      <circle cx="60" cy="55" r="16" fill="#0c0a08" stroke="#E5E0D8" stroke-width="2" />
      <circle cx="53" cy="53" r="3.5" fill="#8B1E1E" />
      <circle cx="67" cy="53" r="3.5" fill="#8B1E1E" />
      <!-- Nasal cavity & teeth runes -->
      <path d="M60,58 L60,63 M55,67 L65,67 M57,67 L57,71 M63,67 L63,71" stroke="#E5E0D8" stroke-width="1.5" stroke-linecap="round" />
      <!-- Blood Runes Around Rim -->
      <text x="60" y="27" fill="#C8B89B" font-size="9" font-family="serif" text-anchor="middle" font-weight="bold">🜍</text>
      <text x="96" y="64" fill="#8B1E1E" font-size="9" font-family="serif" text-anchor="middle" font-weight="bold">☿</text>
      <text x="60" y="105" fill="#C8B89B" font-size="9" font-family="serif" text-anchor="middle" font-weight="bold">🜄</text>
      <text x="24" y="64" fill="#8B1E1E" font-size="9" font-family="serif" text-anchor="middle" font-weight="bold">🜂</text>
    </svg>
  {:else if theme.brand.logoType === "svg-cyber"}
    <!-- Cyber Cyan: Holographic Quantum Hex-Core -->
    <svg viewBox="0 0 120 120" class="size-full drop-shadow-[0_0_25px_rgba(6,182,212,0.7)]" fill="none">
      <polygon points="60,6 110,33 110,87 60,114 10,87 10,33" fill="#040c1a" stroke="#06b6d4" stroke-width="2" />
      <polygon points="60,18 98,40 98,80 60,102 22,80 22,40" stroke="#38bdf8" stroke-width="1" stroke-dasharray="4 2" />
      <!-- Concentric Quantum Rings -->
      <circle cx="60" cy="60" r="22" stroke="#06b6d4" stroke-width="2" />
      <circle cx="60" cy="60" r="14" fill="#06b6d4" fill-opacity="0.2" stroke="#38bdf8" stroke-width="1.5" />
      <!-- Cyber Circuit Nodes -->
      <line x1="60" y1="6" x2="60" y2="38" stroke="#38bdf8" stroke-width="2" />
      <line x1="60" y1="82" x2="60" y2="114" stroke="#38bdf8" stroke-width="2" />
      <line x1="10" y1="60" x2="38" y2="60" stroke="#38bdf8" stroke-width="2" />
      <line x1="82" y1="60" x2="110" y2="60" stroke="#38bdf8" stroke-width="2" />
      <circle cx="60" cy="60" r="5" fill="#38bdf8" />
    </svg>
  {:else if theme.brand.logoType === "svg-matrix"}
    <!-- Emerald Matrix: Phosphor Terminal CRT Daemon Sigil -->
    <svg viewBox="0 0 120 120" class="size-full drop-shadow-[0_0_20px_rgba(16,185,129,0.7)]" fill="none">
      <rect x="10" y="10" width="100" height="100" rx="16" fill="#031006" stroke="#10b981" stroke-width="2.5" />
      <rect x="18" y="18" width="84" height="84" rx="10" stroke="#34d399" stroke-width="1" stroke-dasharray="2 4" />
      <!-- Phosphor Prompt Glyph -->
      <text x="32" y="68" fill="#34d399" font-size="34" font-family="monospace" font-weight="900">&gt;_</text>
      <!-- Binary Matrix Streams on top right -->
      <text x="75" y="42" fill="#10b981" font-size="10" font-family="monospace" opacity="0.7">101</text>
      <text x="75" y="56" fill="#10b981" font-size="10" font-family="monospace" opacity="0.7">010</text>
      <text x="75" y="70" fill="#10b981" font-size="10" font-family="monospace" opacity="0.7">110</text>
      <!-- Bottom Activity Blip -->
      <rect x="26" y="84" width="68" height="4" rx="2" fill="#10b981" />
    </svg>
  {:else if theme.brand.logoType === "svg-crimson"}
    <!-- Crimson Sovereign: Iron Vanguard Combat Shield -->
    <svg viewBox="0 0 120 120" class="size-full drop-shadow-[0_0_25px_rgba(244,63,94,0.7)]" fill="none">
      <path d="M60,6 L108,24 L108,72 Q108,102 60,116 Q12,102 12,72 L12,24 Z" fill="#18040a" stroke="#f43f5e" stroke-width="2.5" />
      <path d="M60,16 L98,30 L98,68 Q98,92 60,104 Q22,92 22,68 L22,30 Z" stroke="#fb7185" stroke-width="1.5" stroke-dasharray="4 2" />
      <!-- Vanguard Eagle / Chevron Blade -->
      <path d="M60,32 L78,60 L68,60 L76,82 L60,70 L44,82 L52,60 L42,60 Z" fill="#f43f5e" stroke="#fb7185" stroke-width="1.5" />
    </svg>
  {:else if theme.brand.logoType === "svg-amber"}
    <!-- Amber Sunset: Solar Fusion Reactor Core -->
    <svg viewBox="0 0 120 120" class="size-full drop-shadow-[0_0_25px_rgba(245,158,11,0.7)]" fill="none">
      <circle cx="60" cy="60" r="50" fill="#140b03" stroke="#f59e0b" stroke-width="2.5" />
      <circle cx="60" cy="60" r="38" stroke="#fbbf24" stroke-width="1.5" stroke-dasharray="6 3" />
      <circle cx="60" cy="60" r="24" fill="#f59e0b" fill-opacity="0.2" stroke="#f59e0b" stroke-width="2" />
      <!-- Radiating Solar Fusion Arcs -->
      <path d="M60,10 L60,22 M60,98 L60,110 M10,60 L22,60 M98,60 L110,60 M24,24 L34,34 M86,86 L96,96 M24,96 L34,86 M86,34 L96,24" stroke="#fbbf24" stroke-width="2.5" stroke-linecap="round" />
      <circle cx="60" cy="60" r="8" fill="#fbbf24" />
    </svg>
  {:else if theme.brand.logoType === "svg-onyx"}
    <!-- Onyx AMOLED: Pure Geometric Minimalist Titanium Wireframe -->
    <svg viewBox="0 0 120 120" class="size-full drop-shadow-[0_0_20px_rgba(255,255,255,0.4)]" fill="none">
      <rect x="12" y="12" width="96" height="96" fill="#000000" stroke="#e4e4e7" stroke-width="2" />
      <rect x="24" y="24" width="72" height="72" stroke="#71717a" stroke-width="1" stroke-dasharray="4 4" />
      <!-- Precision Crosshair Wireframe -->
      <line x1="60" y1="12" x2="60" y2="108" stroke="#e4e4e7" stroke-width="1.5" />
      <line x1="12" y1="60" x2="108" y2="60" stroke="#e4e4e7" stroke-width="1.5" />
      <polygon points="60,36 84,60 60,84 36,60" fill="#000000" stroke="#ffffff" stroke-width="2" />
      <circle cx="60" cy="60" r="4" fill="#ffffff" />
    </svg>
  {/if}
</div>
