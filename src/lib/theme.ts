// RAVENBOT Theme Engine — flat, palette-accurate themes.
//
// Every theme is a solid 8-bit palette: one background, one card, one border,
// one primary, one accent, one text colour. No gradients, no neon blends.
// Applying a theme also rewrites the semantic design tokens (surfaces, text,
// shadcn triplets) so the entire UI — buttons, borders, focus rings — follows
// the active palette.

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
  /** Solid accent used for focus, selection, icons and links (`--brand`). */
  primaryColor: string;
  /** Secondary solid accent. */
  accentColor: string;
  secondaryAccent?: string;
  /**
   * Fill for the primary action button and the light user-message bubble.
   * OpenBot draws these as a *light* Apple-style button (#f0f0f0) and keeps the
   * theme's `primaryColor` for focus/selection instead — so this is separate
   * from `primaryColor`. Falls back to `primaryColor` when omitted (legacy
   * themes behave exactly as before).
   */
  buttonHex?: string;
  /** Text colour on `buttonHex`. Derived from luminance when omitted. */
  buttonForegroundHex?: string;
  bgHex: string;
  cardHex: string;
  borderHex: string;
  /** Explicit text colour (defaults to a readable tone for the background). */
  textColor?: string;
  mutedTextColor?: string;
  description: string;
  brand: ThemeBrandIdentity;
}

/* ── Colour helpers (flat, no gradients) ──────────────────────────────── */

function hexToRgb(hex: string): [number, number, number] {
  let h = hex.replace("#", "").trim();
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const n = parseInt(h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  let h = 0;
  let s = 0;
  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case r: h = (g - b) / d + (g < b ? 6 : 0); break;
      case g: h = (b - r) / d + 2; break;
      default: h = (r - g) / d + 4;
    }
    h /= 6;
  }
  return [h * 360, s * 100, l * 100];
}

/** Convert a hex colour to a bare `H S% L%` triplet for `hsl(var(--x))`. */
export function hexToHslTriplet(hex: string): string {
  const [h, s, l] = rgbToHsl(...hexToRgb(hex));
  return `${Math.round(h)} ${Math.round(s)}% ${Math.round(l)}%`;
}

function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Readable solid text colour on a given background. */
function readableOn(hex: string): string {
  return luminance(hex) > 0.45 ? "#0b0b0f" : "#ffffff";
}

/** Flat colour blend — a solid hex, never a gradient. */
function mixHex(a: string, b: string, amount: number): string {
  const [r1, g1, b1] = hexToRgb(a);
  const [r2, g2, b2] = hexToRgb(b);
  const m = (x: number, y: number) => Math.round(x + (y - x) * amount);
  const to = (n: number) => n.toString(16).padStart(2, "0");
  return `#${to(m(r1, r2))}${to(m(g1, g2))}${to(m(b1, b2))}`;
}

/** Shared flat brand identity so palette themes stay short and consistent. */
function flatBrand(
  brandTitle: string,
  brandAccent: string,
  badgeLabel: string,
  subtitle: string,
  tagline: string,
  logoType: ThemeBrandIdentity["logoType"] = "image",
): ThemeBrandIdentity {
  return {
    logoType,
    logoImage: "/ravenicon.png",
    brandTitle,
    brandAccent,
    badgeLabel,
    subtitle,
    tagline,
    action1Title: "Command Palette",
    action1Desc: "Search the fleet, dispatch actions, launch tools",
    action1Icon: "sparkles",
    action2Title: "Settings & Keys",
    action2Desc: "Configure providers, models, connectors and themes",
    protocolTitle: "Local-First Protocol",
    protocolDesc: "Everything runs on your machine — zero telemetry, zero cloud",
    protocolTags: ["Local-First", "Sandboxed", "Private"],
    statusLabel: "SYSTEM READY",
    statusDesc: "All systems operational",
    buttonBorderRadius: "rounded-xl",
    buttonClass: "border-[var(--hairline)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] text-[var(--text-primary)]",
    cardClass: "bg-[var(--theme-card)] border-[var(--theme-border)]",
  };
}

export const THEMES: ThemeDefinition[] = [
  {
    // The OpenBot-look default: neutral greyscale canvas, #212121 surfaces,
    // a single blue accent for focus/selection, and a LIGHT primary button.
    // This is the look the UI is designed against; every other theme below
    // simply re-tints the same layout.
    id: "openbot",
    name: "OpenBot",
    category: "Neutral",
    primaryColor: "#79b8ff",
    accentColor: "#a6d2ff",
    secondaryAccent: "#9ae6b4",
    buttonHex: "#f0f0f0",
    buttonForegroundHex: "#141414",
    bgHex: "#141414",
    cardHex: "#212121",
    borderHex: "#2a2a2a",
    textColor: "#ffffff",
    mutedTextColor: "#9a9aa0",
    description: "OpenBot's neutral dark system — greyscale canvas, blue accent, light primary button",
    brand: flatBrand("RAVEN", "BOT", "LOCAL OS", "Sovereign Local-First Agent OS", "A persistent fleet of agents that live on your machine"),
  },
  {
    id: "grok-sovereign",
    name: "Grok Sovereign",
    category: "Neutral",
    primaryColor: "#e4e4e7",
    accentColor: "#38bdf8",
    secondaryAccent: "#a1a1aa",
    bgHex: "#000000",
    cardHex: "#0a0a0c",
    borderHex: "#242427",
    textColor: "#f4f4f5",
    mutedTextColor: "#8a8a93",
    description: "OLED black with a titanium silver primary and a single cyan accent",
    brand: flatBrand("RAVEN", "BOT", "SOVEREIGN OS", "Autonomous Local-First Agent OS", "Persistent Fleet of Sovereign Agents Living On Your Machine", "svg-grok"),
  },
  {
    id: "onyx",
    name: "Onyx",
    category: "Neutral",
    primaryColor: "#fafafa",
    accentColor: "#a1a1aa",
    secondaryAccent: "#71717a",
    bgHex: "#000000",
    cardHex: "#0b0b0d",
    borderHex: "#26262b",
    textColor: "#fafafa",
    mutedTextColor: "#8b8b93",
    description: "Pure true-black monochrome — maximum contrast, zero colour noise",
    brand: flatBrand("ONYX", "OS", "MONOCHROME", "Pure black monochrome workspace", "High-contrast local agent fleet"),
  },
  {
    id: "dracula",
    name: "Dracula",
    category: "Classic",
    primaryColor: "#bd93f9",
    accentColor: "#ff79c6",
    secondaryAccent: "#8be9fd",
    bgHex: "#282a36",
    cardHex: "#21222c",
    borderHex: "#44475a",
    textColor: "#f8f8f2",
    mutedTextColor: "#6272a4",
    description: "The canonical Dracula palette — purple, pink and cyan on #282a36",
    brand: flatBrand("DRACULA", "BOT", "NIGHT THEME", "Dracula official palette", "Purple, pink and cyan on charcoal"),
  },
  {
    id: "rose-pine",
    name: "Rosé Pine",
    category: "Classic",
    primaryColor: "#c4a7e7",
    accentColor: "#eb6f92",
    secondaryAccent: "#9ccfd8",
    bgHex: "#191724",
    cardHex: "#1f1d2e",
    borderHex: "#26233a",
    textColor: "#e0def4",
    mutedTextColor: "#6e6a86",
    description: "Rosé Pine — all natural pine, faux fur and a bit of soho vibes",
    brand: flatBrand("ROSÉ", "PINE", "MAIN", "Rosé Pine official palette", "Soothing pastel theme for the high-spirited"),
  },
  {
    id: "rose-pine-moon",
    name: "Rosé Pine Moon",
    category: "Classic",
    primaryColor: "#c4a7e7",
    accentColor: "#ea9a97",
    secondaryAccent: "#9ccfd8",
    bgHex: "#232136",
    cardHex: "#2a273f",
    borderHex: "#393552",
    textColor: "#e0def4",
    mutedTextColor: "#6e6a86",
    description: "Rosé Pine Moon — the darker, softer sibling of Rosé Pine",
    brand: flatBrand("ROSÉ", "MOON", "MOON", "Rosé Pine Moon palette", "Darker Rosé Pine for late sessions"),
  },
  {
    id: "nord",
    name: "Nord",
    category: "Classic",
    primaryColor: "#88c0d0",
    accentColor: "#81a1c1",
    secondaryAccent: "#a3be8c",
    bgHex: "#2e3440",
    cardHex: "#3b4252",
    borderHex: "#4c566a",
    textColor: "#eceff4",
    mutedTextColor: "#d8dee9",
    description: "Nord — an arctic, north-bluish colour palette",
    brand: flatBrand("NORD", "OS", "ARCTIC", "Arctic north-bluish palette", "Cool, calm and collected agents"),
  },
  {
    id: "gruvbox",
    name: "Gruvbox Dark",
    category: "Classic",
    primaryColor: "#d79921",
    accentColor: "#b8bb26",
    secondaryAccent: "#83a598",
    bgHex: "#282828",
    cardHex: "#32302f",
    borderHex: "#504945",
    textColor: "#ebdbb2",
    mutedTextColor: "#a89984",
    description: "Gruvbox Dark — retro groove warm earth tones",
    brand: flatBrand("GRUVBOX", "DARK", "RETRO", "Retro groove warm palette", "Warm earth tones for focused work"),
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    category: "Classic",
    primaryColor: "#7aa2f7",
    accentColor: "#bb9af7",
    secondaryAccent: "#7dcfff",
    bgHex: "#1a1b26",
    cardHex: "#1f2335",
    borderHex: "#292e42",
    textColor: "#c0caf5",
    mutedTextColor: "#565f89",
    description: "Tokyo Night — a clean, dark theme celebrating the lights of downtown Tokyo",
    brand: flatBrand("TOKYO", "NIGHT", "CITY LIGHTS", "Neon city nights palette", "Blue and violet city glow"),
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    category: "Classic",
    primaryColor: "#cba6f7",
    accentColor: "#f5c2e7",
    secondaryAccent: "#94e2d5",
    bgHex: "#1e1e2e",
    cardHex: "#181825",
    borderHex: "#313244",
    textColor: "#cdd6f4",
    mutedTextColor: "#a6adc8",
    description: "Catppuccin Mocha — soothed pastels for the late-night coder",
    brand: flatBrand("CATPPUCCIN", "MOCHA", "PASTEL", "Soothing pastel theme", "Soft pastels on deep espresso"),
  },
  {
    id: "one-dark",
    name: "One Dark",
    category: "Classic",
    primaryColor: "#61afef",
    accentColor: "#c678dd",
    secondaryAccent: "#98c379",
    bgHex: "#282c34",
    cardHex: "#21252b",
    borderHex: "#3e4451",
    textColor: "#abb2bf",
    mutedTextColor: "#5c6370",
    description: "Atom One Dark — the classic balanced dark theme",
    brand: flatBrand("ONE", "DARK", "ATOM", "Balanced dark theme", "The classic Atom editor palette"),
  },
  {
    id: "solarized-dark",
    name: "Solarized Dark",
    category: "Classic",
    primaryColor: "#268bd2",
    accentColor: "#2aa198",
    secondaryAccent: "#b58900",
    bgHex: "#002b36",
    cardHex: "#073642",
    borderHex: "#586e75",
    textColor: "#93a1a1",
    mutedTextColor: "#657b83",
    description: "Solarized Dark — precision colours with balanced contrast",
    brand: flatBrand("SOLARIZED", "DARK", "PRECISION", "Precision-balanced palette", "Designed for long reading sessions"),
  },
  {
    id: "cyber-cyan",
    name: "Cyber Cyan",
    category: "Accent",
    primaryColor: "#06b6d4",
    accentColor: "#38bdf8",
    secondaryAccent: "#3b82f6",
    bgHex: "#030712",
    cardHex: "#081221",
    borderHex: "#123a5c",
    textColor: "#d7f9ff",
    mutedTextColor: "#5e9dbb",
    description: "Flat cyan on a deep ocean navy",
    brand: flatBrand("CYBER", "CORE", "QUANTUM", "Cyan agent mainframe", "Flat neon cyan, no gradients", "svg-cyber"),
  },
  {
    id: "emerald",
    name: "Emerald Matrix",
    category: "Accent",
    primaryColor: "#10b981",
    accentColor: "#34d399",
    secondaryAccent: "#059669",
    bgHex: "#03110a",
    cardHex: "#071a10",
    borderHex: "#124a2c",
    textColor: "#c7f9e5",
    mutedTextColor: "#4ea87e",
    description: "Flat phosphor green on near-black",
    brand: flatBrand("EMERALD", "MATRIX", "TERMINAL", "Phosphor terminal green", "Flat green terminal aesthetic", "svg-matrix"),
  },
];

let activeThemeId = "openbot";
const listeners = new Set<(theme: ThemeDefinition) => void>();

export function getStoredTheme(): ThemeDefinition {
  if (typeof window === "undefined") return THEMES[0];
  const saved = localStorage.getItem("raven-theme");
  if (!saved) return THEMES[0];
  return THEMES.find((t) => t.id === saved) || THEMES[0];
}

export function applyTheme(themeId: string) {
  const theme = THEMES.find((t) => t.id === themeId) || THEMES[0];
  activeThemeId = theme.id;

  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", theme.id);
    localStorage.setItem("raven-theme", theme.id);

    const root = document.documentElement;
    const bg = theme.bgHex;
    const card = theme.cardHex;
    const border = theme.borderHex;
    const accent = theme.primaryColor; // focus / selection / icons (`--brand`)
    const accentText = theme.accentColor;
    // The LIGHT primary-button fill. Falls back to primaryColor for themes
    // that predate buttonHex, so they render exactly as they always did.
    const button = theme.buttonHex ?? theme.primaryColor;
    const buttonFg = theme.buttonForegroundHex ?? readableOn(button);
    const text = theme.textColor || readableOn(bg);
    const muted = theme.mutedTextColor || mixHex(text, bg, 0.5);

    // Raw theme colours (consumed by layout chrome + components)
    root.style.setProperty("--theme-primary", accent);
    root.style.setProperty("--theme-accent", accentText);
    root.style.setProperty("--theme-bg", bg);
    root.style.setProperty("--theme-card", card);
    root.style.setProperty("--theme-border", border);

    // Flat brand tokens — the BLUE accent used for focus, selection, links.
    root.style.setProperty("--brand", accent);
    root.style.setProperty("--brand-hover", mixHex(accent, "#ffffff", 0.15));
    root.style.setProperty("--brand-text", accentText);
    root.style.setProperty("--brand-2", accentText);
    root.style.setProperty("--brand-3", theme.secondaryAccent || accentText);
    root.style.setProperty("--brand-soft", mixHex(bg, accent, 0.16));
    root.style.setProperty("--brand-strong", mixHex(bg, accent, 0.34));
    root.style.setProperty("--brand-glow", "none");
    // Light primary-button / user-bubble surface (OpenBot's #f0f0f0).
    root.style.setProperty("--surface-light", button);
    root.style.setProperty("--text-on-light", buttonFg);

    // Semantic surfaces (solid)
    root.style.setProperty("--surface-0", bg);
    root.style.setProperty("--surface-1", card);
    root.style.setProperty("--surface-2", mixHex(card, text, 0.05));
    root.style.setProperty("--surface-3", mixHex(card, text, 0.1));
    root.style.setProperty("--surface-4", mixHex(card, text, 0.16));
    root.style.setProperty("--text-primary", text);
    root.style.setProperty("--text-secondary", mixHex(text, muted, 0.18));
    root.style.setProperty("--text-tertiary", muted);
    root.style.setProperty("--text-muted", mixHex(muted, bg, 0.28));
    root.style.setProperty("--text-faint", mixHex(muted, bg, 0.45));
    root.style.setProperty("--hairline", border);
    root.style.setProperty("--hairline-strong", mixHex(border, text, 0.14));

    // shadcn/theme triplets so buttons, rings, borders, cards all follow suit
    root.style.setProperty("--background", hexToHslTriplet(bg));
    root.style.setProperty("--foreground", hexToHslTriplet(text));
    root.style.setProperty("--card", hexToHslTriplet(card));
    root.style.setProperty("--card-foreground", hexToHslTriplet(text));
    root.style.setProperty("--popover", hexToHslTriplet(card));
    root.style.setProperty("--popover-foreground", hexToHslTriplet(text));
    // --primary is the BUTTON fill (light), not the accent.
    root.style.setProperty("--primary", hexToHslTriplet(button));
    root.style.setProperty("--primary-foreground", hexToHslTriplet(buttonFg));
    root.style.setProperty("--secondary", hexToHslTriplet(mixHex(card, text, 0.08)));
    root.style.setProperty("--secondary-foreground", hexToHslTriplet(text));
    root.style.setProperty("--muted", hexToHslTriplet(mixHex(card, text, 0.06)));
    root.style.setProperty("--muted-foreground", hexToHslTriplet(muted));
    // --accent stays a neutral hover surface (OpenBot never uses a saturated
    // hover); focus/selection come from --ring below.
    root.style.setProperty("--accent", hexToHslTriplet(mixHex(card, text, 0.08)));
    root.style.setProperty("--accent-foreground", hexToHslTriplet(text));
    root.style.setProperty("--destructive", hexToHslTriplet("#ff96a0"));
    root.style.setProperty("--destructive-foreground", hexToHslTriplet("#141414"));
    root.style.setProperty("--border", hexToHslTriplet(border));
    root.style.setProperty("--input", hexToHslTriplet(border));
    // Focus ring = the BLUE accent (or the theme's accent colour).
    root.style.setProperty("--ring", hexToHslTriplet(accent));
  }

  listeners.forEach((fn) => fn(theme));
}

export function subscribeTheme(fn: (theme: ThemeDefinition) => void) {
  listeners.add(fn);
  fn(getStoredTheme());
  return () => listeners.delete(fn);
}
