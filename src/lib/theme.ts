// Comprehensive Theme Engine for RAVENBOT
// Complete dynamic metamorphosis: Logos, Backgrounds, Buttons, Badges, Typography, and Lore

export interface ThemeBrandIdentity {
  logoType: "svg-grok" | "image" | "svg-rot" | "svg-cyber" | "svg-matrix" | "svg-crimson" | "svg-amber" | "svg-onyx";
  logoImage?: string;
  brandTitle: string;
  brandAccent: string;
  badgeLabel: string;
  subtitle: string;
  tagline: string;
  action1Title: string;
  action1Desc: string;
  action1Icon: string;
  action2Title: string;
  action2Desc: string;
  protocolTitle: string;
  protocolDesc: string;
  protocolTags: [string, string, string];
  statusLabel: string;
  statusDesc: string;
  buttonBorderRadius: string;
  buttonClass: string;
  cardClass: string;
}

export interface ThemeDefinition {
  id: string;
  name: string;
  category: string;
  primaryColor: string;
  accentColor: string;
  secondaryAccent?: string;
  bgHex: string;
  cardHex: string;
  borderHex: string;
  boneColor?: string;
  parchmentColor?: string;
  rustColor?: string;
  mossColor?: string;
  description: string;
  brand: ThemeBrandIdentity;
}

export const THEMES: ThemeDefinition[] = [
  {
    id: "grok-sovereign",
    name: "Grok Sovereign",
    category: "Superluminal Minimal",
    primaryColor: "#ffffff",
    accentColor: "#38bdf8",
    secondaryAccent: "#a1a1aa",
    bgHex: "#000000",
    cardHex: "#09090b",
    borderHex: "#27272a",
    description: "Ultra-sleek Grok dark interface with pure OLED black and electric cyan accents",
    brand: {
      logoType: "svg-grok",
      brandTitle: "RAVEN",
      brandAccent: "BOT",
      badgeLabel: "SOVEREIGN OS",
      subtitle: "Autonomous Local-First Agent OS",
      tagline: "Persistent Fleet of Sovereign Agents Living On Your Machine",
      action1Title: "Command Palette",
      action1Desc: "Search fleet, dispatch actions, trigger autonomous tools",
      action1Icon: "sparkles",
      action2Title: "Settings & Model Keys",
      action2Desc: "Configure Claude 3.5, GPT-4o, Ollama Local & MCP Connectors",
      protocolTitle: "Sovereign Enclave Protocol",
      protocolDesc: "Hardware-sandboxed execution with zero external telemetry",
      protocolTags: ["Local-First", "Sandboxed", "Encrypted"],
      statusLabel: "SYSTEM OPTIMAL",
      statusDesc: "All systems verified & operational",
      buttonBorderRadius: "rounded-xl",
      buttonClass: "border-zinc-800 bg-[#121216] hover:border-zinc-500 hover:bg-[#181820] text-zinc-100 shadow-sm",
      cardClass: "bg-[#09090b]/95 border-[#27272a] shadow-2xl",
    },
  },
  {
    id: "the-rot-archive",
    name: "The Rot Archive",
    category: "Forbidden Grimoire",
    primaryColor: "#8B1E1E", // Blood Red
    accentColor: "#C8B89B",  // Aged Parchment
    secondaryAccent: "#2D3F31", // Necrotic Moss
    bgHex: "#080706",        // Decayed Reliquary Charcoal
    cardHex: "#12100d",      // Ancient Dark Leather & Wood
    borderHex: "#3e2c22",    // Rust Iron & Dried Blood
    boneColor: "#E5E0D8",    // Bone White
    parchmentColor: "#C8B89B", // Aged Parchment
    rustColor: "#5C3B2E",    // Rust Brown
    mossColor: "#2D3F31",    // Moss Green
    description: "Forbidden alchemical codex: Blood Red, Aged Parchment, Bone White, and Necrotic Moss",
    brand: {
      logoType: "svg-rot",
      brandTitle: "ROT",
      brandAccent: "ARCHIVE",
      badgeLabel: "SEALED CODEX",
      subtitle: "Forbidden Codex & Living Knowledge",
      tagline: "Alchemical Mutating Multi-Agent Realm",
      action1Title: "Grimoire & Incantations",
      action1Desc: "Summon scholars, cast alchemical transmutation tools",
      action1Icon: "book-open",
      action2Title: "Alchemical Reliquary & Keys",
      action2Desc: "Configure cipher seals, local distillers, and secret runes",
      protocolTitle: "Necrotic Quarantine Protocol",
      protocolDesc: "All forbidden texts quarantined within alchemical boundaries",
      protocolTags: ["Quarantined", "Encrypted", "Purified"],
      statusLabel: "SEAL INTACT",
      statusDesc: "All forbidden codices contained",
      buttonBorderRadius: "rounded-xl",
      buttonClass: "border-stone-700/60 bg-[#161310] hover:bg-[#201c17] text-[#C8B89B] shadow-inner",
      cardClass: "bg-[#12100d]/95 border-[#3e2c22] shadow-[0_4px_25px_rgba(0,0,0,0.8)]",
    },
  },
  {
    id: "obsidian-violet",
    name: "Obsidian Violet",
    category: "Sovereign Default",
    primaryColor: "#8b5cf6", // Electric Violet
    accentColor: "#a855f7",
    secondaryAccent: "#06b6d4",
    bgHex: "#07070a",
    cardHex: "#0c0c14",
    borderHex: "#222234",
    description: "Deep obsidian dark with electric violet neon crest",
    brand: {
      logoType: "image",
      logoImage: "/ravenicon.png",
      brandTitle: "RAVEN",
      brandAccent: "BOT",
      badgeLabel: "SOVEREIGN",
      subtitle: "Sovereign, Local-First, Hardware-Encrypted",
      tagline: "Multi-Agent Fleet Operating System",
      action1Title: "Command Palette",
      action1Desc: "Search fleet, launch actions, configure tools",
      action1Icon: "terminal",
      action2Title: "Settings & Keys",
      action2Desc: "Configure OpenRouter, Claude, GPT, Ollama and themes",
      protocolTitle: "Fleet Safety Protocol",
      protocolDesc: "All agents operate within secure boundaries",
      protocolTags: ["Sandboxed", "Encrypted", "Verified"],
      statusLabel: "SYSTEM SECURE",
      statusDesc: "All systems operational",
      buttonBorderRadius: "rounded-2xl",
      buttonClass: "border-[#1f1f2e] bg-[#0c0c14]/90 hover:border-purple-500/50 hover:bg-[#10101b] text-white",
      cardClass: "bg-[#0c0c14]/90 border-[#1f1f2e]",
    },
  },
  {
    id: "cyber-cyan",
    name: "Cyber Cyan",
    category: "Futuristic",
    primaryColor: "#06b6d4", // Neon Cyan
    accentColor: "#38bdf8",  // Sky Blue
    secondaryAccent: "#3b82f6",
    bgHex: "#030712",
    cardHex: "#061024",
    borderHex: "#112648",
    description: "Sub-zero neon cyan with deep ocean trench obsidian",
    brand: {
      logoType: "svg-cyber",
      brandTitle: "CYBER",
      brandAccent: "CORE",
      badgeLabel: "QUANTUM 9",
      subtitle: "Holographic Neural Grid & Agent Mainframe",
      tagline: "High-Bandwidth Distributed Intelligence",
      action1Title: "Quantum Dispatcher",
      action1Desc: "Route neural vectors, execute parallel sub-routines",
      action1Icon: "cpu",
      action2Title: "Neural Telemetry & Keys",
      action2Desc: "Manage quantum endpoints, API channels and latency",
      protocolTitle: "Quantum Shielding Matrix",
      protocolDesc: "Hardware-isolated neural sandboxes with cryptographic proof",
      protocolTags: ["Isolated", "Encrypted", "Supercharged"],
      statusLabel: "GRID ONLINE",
      statusDesc: "Sub-zero neural channels 100% synchronized",
      buttonBorderRadius: "rounded-lg",
      buttonClass: "border-cyan-800/60 bg-[#071328] hover:border-cyan-400 hover:bg-[#0c1f40] text-cyan-200 shadow-[0_0_15px_rgba(6,182,212,0.15)]",
      cardClass: "bg-[#061024]/95 border-[#112648]",
    },
  },
  {
    id: "emerald-matrix",
    name: "Emerald Matrix",
    category: "Terminal",
    primaryColor: "#10b981", // Matrix Green
    accentColor: "#34d399",
    secondaryAccent: "#059669",
    bgHex: "#020904",
    cardHex: "#051509",
    borderHex: "#0f3317",
    description: "Cyberpunk mainframe green with stealth CRT scanlines",
    brand: {
      logoType: "svg-matrix",
      brandTitle: "MATRIX",
      brandAccent: "MAINFRAME",
      badgeLabel: "ROOT PRIVILEGED",
      subtitle: "Phosphor CRT Daemon & Shell Environment",
      tagline: "Zero-Latency Local Daemon Swarm",
      action1Title: ">_ Terminal Shell Exec",
      action1Desc: "Spawn daemon threads, pipe standard streams, dispatch IPC",
      action1Icon: "terminal",
      action2Title: "SysConfig & Root Keys",
      action2Desc: "Configure kernel parameters, local LLMs and shell hooks",
      protocolTitle: "Kernel Sandbox Boundary",
      protocolDesc: "Enforced cgroups and eBPF syscall filtering active",
      protocolTags: ["Air-Gapped", "eBPF Locked", "Zero-Leak"],
      statusLabel: "MAINFRAME LOCKED",
      statusDesc: "Root kernel integrity verified",
      buttonBorderRadius: "rounded-md",
      buttonClass: "border-emerald-800/80 bg-[#06180b] hover:border-emerald-400 hover:bg-[#0c2914] text-emerald-300 font-mono shadow-[0_0_12px_rgba(16,185,129,0.2)]",
      cardClass: "bg-[#051509]/95 border-[#0f3317]",
    },
  },
  {
    id: "crimson-sovereign",
    name: "Crimson Sovereign",
    category: "Combat",
    primaryColor: "#f43f5e", // Crimson
    accentColor: "#fb7185",
    secondaryAccent: "#e11d48",
    bgHex: "#0c0306",
    cardHex: "#18060d",
    borderHex: "#3d1020",
    description: "Aggressive blood-ruby crimson with dark obsidian vanguard",
    brand: {
      logoType: "svg-crimson",
      brandTitle: "VANGUARD",
      brandAccent: "RED",
      badgeLabel: "TACTICAL DEFCON 1",
      subtitle: "Mission-Critical Sovereign Fleet Vanguard",
      tagline: "High-Priority Autonomous Strike Teams",
      action1Title: "Tactical Strike Palette",
      action1Desc: "Engage task graphs, coordinate agent squads, execute targets",
      action1Icon: "zap",
      action2Title: "Armory & Encryption Keys",
      action2Desc: "Manage hardware security modules and credential armory",
      protocolTitle: "Vanguard Combat Protocol",
      protocolDesc: "Total isolation with immediate automatic failover",
      protocolTags: ["Hardened", "Armored", "Impervious"],
      statusLabel: "VANGUARD ARMED",
      statusDesc: "All tactical units in ready state",
      buttonBorderRadius: "rounded-xl",
      buttonClass: "border-rose-900/80 bg-[#1e0710] hover:border-rose-500 hover:bg-[#2b0a17] text-rose-200 shadow-[0_0_15px_rgba(244,63,94,0.2)]",
      cardClass: "bg-[#18060d]/95 border-[#3d1020]",
    },
  },
  {
    id: "amber-sunset",
    name: "Amber Sunset",
    category: "Warm",
    primaryColor: "#f59e0b", // Solar Gold
    accentColor: "#fbbf24",
    secondaryAccent: "#d97706",
    bgHex: "#0a0703",
    cardHex: "#170f06",
    borderHex: "#38230e",
    description: "High-voltage amber gold with warm solar carbon backdrop",
    brand: {
      logoType: "svg-amber",
      brandTitle: "SOLAR",
      brandAccent: "FORGE",
      badgeLabel: "FUSION REACTOR",
      subtitle: "Solar Powered High-Yield Agent Forge",
      tagline: "Unrestricted Local Compute & Creative Synthesizer",
      action1Title: "Solar Command Conduit",
      action1Desc: "Ignite creative engines, synthesize ideas, forge workflows",
      action1Icon: "flame",
      action2Title: "Forge Foundry & Keys",
      action2Desc: "Tune token temperatures, configure models, manage keys",
      protocolTitle: "Solar Containment Shield",
      protocolDesc: "High-temperature safety buffers and rate-limiting safeguards",
      protocolTags: ["Thermal Safe", "Fusion Locked", "Optimal"],
      statusLabel: "REACTOR STABLE",
      statusDesc: "Optimal core temperature and throughput",
      buttonBorderRadius: "rounded-2xl",
      buttonClass: "border-amber-800/70 bg-[#1a1106] hover:border-amber-400 hover:bg-[#261909] text-amber-200 shadow-[0_0_15px_rgba(245,158,11,0.18)]",
      cardClass: "bg-[#170f06]/95 border-[#38230e]",
    },
  },
  {
    id: "onyx-monochrome",
    name: "Onyx AMOLED",
    category: "Stealth",
    primaryColor: "#e4e4e7", // Titanium Silver
    accentColor: "#ffffff",  // Pure White
    secondaryAccent: "#71717a",
    bgHex: "#000000",        // Pure 100% AMOLED Black
    cardHex: "#0a0a0a",
    borderHex: "#222222",
    description: "Pure true-black AMOLED with high-contrast titanium silver",
    brand: {
      logoType: "svg-onyx",
      brandTitle: "ONYX",
      brandAccent: "ZERO",
      badgeLabel: "STEALTH AIR-GAP",
      subtitle: "Zero-Light Pure AMOLED Minimalist Core",
      tagline: "Pure Local Sovereign Computation",
      action1Title: "Stealth Dispatcher",
      action1Desc: "Execute silent background tasks with minimal footprint",
      action1Icon: "sparkles",
      action2Title: "Onyx Key Vault",
      action2Desc: "Manage hardware enclave and biometric authentications",
      protocolTitle: "Air-Gapped Stealth Protocol",
      protocolDesc: "Total zero-telemetry hardware enclave isolation",
      protocolTags: ["Air-Gapped", "Encrypted", "Zero-Trace"],
      statusLabel: "STEALTH ACTIVE",
      statusDesc: "0 dB acoustic / 0 byte network telemetry",
      buttonBorderRadius: "rounded-none",
      buttonClass: "border-zinc-800 bg-[#0e0e0e] hover:border-zinc-400 hover:bg-[#181818] text-zinc-100 font-mono",
      cardClass: "bg-[#080808] border-[#222222]",
    },
  },
];

let activeThemeId = "grok-sovereign";
const listeners = new Set<(theme: ThemeDefinition) => void>();

export function getStoredTheme(): ThemeDefinition {
  if (typeof window === "undefined") return THEMES[0];
  const saved = localStorage.getItem("raven-theme");
  // Default to grok-sovereign and migrate the-rot-archive
  if (!saved || saved === "the-rot-archive") {
    return THEMES[0];
  }
  const found = THEMES.find((t) => t.id === saved);
  return found || THEMES[0];
}

export function applyTheme(themeId: string) {
  const theme = THEMES.find((t) => t.id === themeId) || THEMES[0];
  activeThemeId = theme.id;

  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", theme.id);
    localStorage.setItem("raven-theme", theme.id);

    // Apply CSS variables dynamically to root
    const root = document.documentElement;
    root.style.setProperty("--theme-primary", theme.primaryColor);
    root.style.setProperty("--theme-accent", theme.accentColor);
    root.style.setProperty("--theme-bg", theme.bgHex);
    root.style.setProperty("--theme-card", theme.cardHex);
    root.style.setProperty("--theme-border", theme.borderHex);

    if (theme.boneColor) root.style.setProperty("--theme-bone", theme.boneColor);
    if (theme.parchmentColor) root.style.setProperty("--theme-parchment", theme.parchmentColor);
    if (theme.rustColor) root.style.setProperty("--theme-rust", theme.rustColor);
    if (theme.mossColor) root.style.setProperty("--theme-moss", theme.mossColor);
  }

  listeners.forEach((fn) => fn(theme));
}

export function subscribeTheme(fn: (theme: ThemeDefinition) => void) {
  listeners.add(fn);
  fn(getStoredTheme());
  return () => listeners.delete(fn);
}
