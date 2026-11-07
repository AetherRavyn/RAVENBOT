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

/**
 * Whether a canvas colour is light.
 *
 * This is measured rather than declared so every theme added later — including
 * ones from a user-supplied palette — gets the right `color-scheme`, native
 * scrollbars, and focus/overlay treatment without a second registry to keep in
 * sync.
 */
export function isLightColor(hex: string): boolean {
  return luminance(hex) > 0.5;
}

/** WCAG 2.1 relative contrast between two hex colours. 1 … 21. */
export function contrastRatio(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/**
 * Push `hex` away from `bg` until it clears `ratio`, or until it runs out of
 * room.
 *
 * ## Why this exists
 *
 * The text ramp used to be built by mixing fixed proportions towards the
 * background — `--text-muted` was `mixHex(muted, bg, 0.28)`. That is a
 * *relative* recipe, so its contrast is whatever the theme's own muted colour
 * happens to produce, and for every dark theme in the catalogue it produced
 * 2.6–4.0:1. Just under the 4.5:1 that WCAG AA asks of body text, which means
 * every piece of secondary text in the app failed and nothing said so.
 *
 * A themeable token cannot be specified as a mix ratio and also be guaranteed
 * to pass. So the ramp is specified as a *ratio*, and this is what turns the
 * ratio back into a colour: step the candidate away from its background until
 * it clears the floor. It costs a few dozen arithmetic operations once per
 * theme change and it makes the guarantee hold for every theme, including ones
 * added later, which is the part a hand-tuned hex cannot do.
 *
 * Direction is chosen from the background, not from the candidate, so a theme
 * with an unusual primary colour still moves the right way.
 */
export function ensureContrast(hex: string, bg: string, ratio: number): string {
  if (contrastRatio(hex, bg) >= ratio) return hex;
  const dark = luminance(bg) < 0.5;
  // Fully clear and fully dark are the two stops available.
  const target = dark ? "#ffffff" : "#000000";
  // 2% steps: fine enough that the result is indistinguishable from a
  // continuous solve, coarse enough to be obviously correct and terminate.
  for (let step = 0.02; step <= 1.0001; step += 0.02) {
    const candidate = mixHex(hex, target, step);
    if (contrastRatio(candidate, bg) >= ratio) return candidate;
  }
  return target;
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
    // The `id` is what a user's saved preference stores, so it stays `openbot`
    // even though nothing is called that on screen any more — renaming it would
    // silently drop everyone back to the default.
    id: "openbot",
    // Named for the look, like every other theme here (Onyx, Dracula, Nord).
    // It was called "OpenBot", which put another product's name in the middle
    // of RAVENBOT's own title bar.
    name: "Neutral Dark",
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
    description: "Neutral dark system — greyscale canvas, blue accent, light primary button",
    brand: flatBrand("RAVEN", "BOT", "LOCAL OS", "Sovereign Local-First Agent OS", "A persistent fleet of agents that live on your machine"),
  },
  {
    id: "paper-light",
    name: "Paper Light",
    category: "Neutral",
    primaryColor: "#2563eb",
    accentColor: "#1d4ed8",
    secondaryAccent: "#047857",
    buttonHex: "#18181b",
    buttonForegroundHex: "#ffffff",
    bgHex: "#f4f4f5",
    cardHex: "#ffffff",
    borderHex: "#e4e4e7",
    textColor: "#18181b",
    mutedTextColor: "#71717a",
    description: "Paper-light canvas — zinc ink, a blue accent, and a dark primary button",
    brand: flatBrand("RAVEN", "BOT", "DAYLIGHT", "Local-first agent OS in daylight", "A quiet light workspace for focused agent work"),
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

/**
 * The contrast floor every body-text token in the ramp is held to.
 *
 * WCAG 2.1 AA asks 4.5:1 for text under 18.66px. The text ramp is specified
 * against that number rather than against whatever each theme happened to
 * produce, so the guarantee is a property of the theme engine rather than a
 * coincidence of the current palette.
 */
export const AA_TEXT = 4.5;

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
    // Named because the contrast floor below is measured against it, and a
    // reader of `ensureContrast(…, surface2, …)` should not have to go and
    // find where surface2 came from.
    const surface2 = mixHex(card, text, 0.05);
    const light = isLightColor(bg);

    // Native controls follow the canvas. Without this, a light theme inherits
    // dark scrollbars, dark form controls and dark `::selection`, which is the
    // difference between a light theme and a dark theme with white panels.
    root.style.colorScheme = light ? "light" : "dark";
    root.setAttribute("data-color-scheme", light ? "light" : "dark");

    // Raw theme colours (consumed by layout chrome + components)
    root.style.setProperty("--theme-primary", accent);
    root.style.setProperty("--theme-accent", accentText);
    root.style.setProperty("--theme-bg", bg);
    root.style.setProperty("--theme-card", card);
    root.style.setProperty("--theme-border", border);

    // Flat brand tokens — the BLUE accent used for focus, selection, links.
    root.style.setProperty("--brand", accent);
    root.style.setProperty("--brand-hover", mixHex(accent, light ? "#000000" : "#ffffff", 0.15));
    root.style.setProperty("--brand-text", accentText);
    root.style.setProperty("--brand-2", accentText);
    root.style.setProperty("--brand-3", theme.secondaryAccent || accentText);
    root.style.setProperty("--brand-soft", mixHex(bg, accent, 0.16));
    root.style.setProperty("--brand-strong", mixHex(bg, accent, 0.34));
    root.style.setProperty("--brand-glow", "none");
    // Light primary-button / user-bubble surface (OpenBot's #f0f0f0).
    root.style.setProperty("--surface-light", button);
    root.style.setProperty("--surface-light-hover", light ? mixHex(button, "#ffffff", 0.12) : "#ffffff");
    root.style.setProperty("--text-on-light", buttonFg);

    // Semantic surfaces (solid)
    root.style.setProperty("--surface-0", bg);
    root.style.setProperty("--surface-1", card);
    root.style.setProperty("--surface-2", surface2);
    root.style.setProperty("--surface-3", mixHex(card, text, 0.1));
    root.style.setProperty("--surface-4", mixHex(card, text, 0.16));
    root.style.setProperty("--text-primary", text);
    // Secondary and tertiary are held to the same floor as muted. Mixing
    // toward the muted colour is a recipe, not a guarantee: on Paper Light it
    // landed at 4.36:1 on the raised surface.
    const secondary = ensureContrast(mixHex(text, muted, 0.18), surface2, AA_TEXT);
    const tertiary = ensureContrast(muted, surface2, AA_TEXT);
    root.style.setProperty("--text-secondary", secondary);
    root.style.setProperty("--text-tertiary", tertiary);
    // The three steps below the primary are *specified as contrast ratios*,
    // not as mix proportions — see `ensureContrast` for why, and for what it
    // cost to get this wrong first.
    //
    // The reference surface is `surface-2`, not the canvas. Secondary text
    // overwhelmingly sits on a panel or a raised row rather than on the bare
    // background, and measuring against the canvas is how a ramp ends up
    // passing everywhere it was checked and failing everywhere it was used.
    root.style.setProperty("--text-muted", ensureContrast(mixHex(muted, bg, 0.28), surface2, AA_TEXT));
    // `faint` is decoration and placeholder text, which WCAG treats as
    // non-text: the 3:1 large-text floor, not the 4.5:1 body floor.
    root.style.setProperty("--text-faint", ensureContrast(mixHex(muted, bg, 0.45), surface2, 3));
    root.style.setProperty("--hairline", border);
    root.style.setProperty("--hairline-strong", mixHex(border, text, 0.14));

    if (light) {
      applyLightRoleTokens(root);
    } else {
      clearLightRoleTokens(root);
    }

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
    root.style.setProperty("--muted-foreground", hexToHslTriplet(tertiary));
    // --accent stays a neutral hover surface (OpenBot never uses a saturated
    // hover); focus/selection come from --ring below.
    root.style.setProperty("--accent", hexToHslTriplet(mixHex(card, text, 0.08)));
    root.style.setProperty("--accent-foreground", hexToHslTriplet(text));
    root.style.setProperty("--destructive", hexToHslTriplet(light ? "#c02b3f" : "#ff96a0"));
    root.style.setProperty("--destructive-foreground", hexToHslTriplet(light ? "#ffffff" : "#141414"));
    root.style.setProperty("--border", hexToHslTriplet(border));
    root.style.setProperty("--input", hexToHslTriplet(border));
    // Focus ring = the BLUE accent (or the theme's accent colour).
    root.style.setProperty("--ring", hexToHslTriplet(accent));
  }

  listeners.forEach((fn) => fn(theme));
}

/**
 * Role tokens that cannot be derived by mixing the palette.
 *
 * Status colours, washes, overlays and a few data surfaces were tuned against
 * a dark canvas. A light theme needs its own values: the same pastel mint that
 * reads on #141414 is nearly invisible on #ffffff. Keeping the list in one
 * place is what makes a light theme a first-class member of the catalogue
 * rather than a dark theme with an inverted background.
 */
const LIGHT_ROLE_TOKENS: Record<string, string> = {
  "--hover-wash": "rgba(24, 24, 27, 0.055)",
  "--overlay-bg": "rgba(244, 244, 245, 0.94)",
  "--dialog-overlay": "rgba(15, 23, 42, 0.35)",
  "--dialog-ring": "rgba(24, 24, 27, 0.08)",
  "--glass": "rgba(255, 255, 255, 0.8)",
  "--image-outline": "rgba(24, 24, 27, 0.12)",
  "--scroll-edge-overlay": "rgba(255, 255, 255, 0.86)",
  "--hairline-faint": "rgba(24, 24, 27, 0.045)",
  "--surface-elevated": "#ffffff",
  "--rail-selected": "#18181b",
  "--scrim": "rgba(15, 23, 42, 0.38)",

  "--status-idle": "#71717a",
  "--status-thinking": "#6d4bc4",
  "--status-running": "#2563eb",
  "--status-waiting": "#c02b3f",
  "--status-paused": "#52525b",
  "--status-success": "#157347",
  "--status-danger": "#c02b3f",

  "--success-text": "#157347",
  "--success-soft": "rgba(21, 115, 71, 0.1)",
  "--success-wash": "rgba(21, 115, 71, 0.045)",
  "--warning-text": "#8a5a00",
  "--warning-soft": "rgba(180, 120, 0, 0.12)",
  "--danger-text": "#c02b3f",
  "--danger-hover": "#a52236",
  "--danger-soft": "rgba(192, 43, 63, 0.08)",
  "--danger-soft-hover": "rgba(192, 43, 63, 0.14)",
  "--danger-focus": "rgba(192, 43, 63, 0.32)",
  "--danger-border": "rgba(192, 43, 63, 0.4)",

  "--question-option": "#0f6f73",
  "--question-option-soft": "rgba(15, 111, 115, 0.1)",
  "--question-option-hover": "rgba(15, 111, 115, 0.15)",
  "--question-option-pressed": "rgba(15, 111, 115, 0.2)",
  "--question-badge-text": "#0b5c60",
  "--question-badge-soft": "rgba(15, 111, 115, 0.12)",
  "--message-badge-text": "#1d4ed8",
  "--message-badge-soft": "rgba(37, 99, 235, 0.1)",
  "--approval-badge-soft": "rgba(180, 120, 0, 0.12)",
  "--badge-new": "#b0246e",
  "--badge-new-soft": "rgba(176, 36, 110, 0.1)",
  "--unread-surface": "#1d4ed8",
  "--unread-text": "#ffffff",

  "--data-table-border": "#e4e4e7",
  "--data-table-header-text": "#52525b",
  "--data-table-text": "#18181b",
  "--text-readable": "rgba(24, 24, 27, 0.56)",
  "--text-placeholder": "#71717a",
  "--text-muted-on-light": "rgba(255, 255, 255, 0.62)",
  "--scrollbar-thumb": "rgba(24, 24, 27, 0.16)",
  "--scrollbar-thumb-hover": "rgba(24, 24, 27, 0.28)",

  // Tailwind status aliases are HSL triplets consumed by utilities like
  // `text-warning` and `bg-warning/10`. They are not derived from the four
  // palette colours, so a light canvas needs its own readable set.
  "--success": hexToHslTriplet("#157347"),
  "--warning": hexToHslTriplet("#8a5a00"),
  "--danger": hexToHslTriplet("#c02b3f"),
  "--info": hexToHslTriplet("#1d4ed8"),
};

function applyLightRoleTokens(root: HTMLElement) {
  for (const [name, value] of Object.entries(LIGHT_ROLE_TOKENS)) {
    root.style.setProperty(name, value);
  }
}

/**
 * Removing the inline overrides is the inverse of applying them. Without this,
 * trying Paper Light and then picking a dark theme would leave the light
 * status ladder, washes and scrollbar colours inline for the rest of the
 * session — the palette would switch while half the role tokens did not.
 */
function clearLightRoleTokens(root: HTMLElement) {
  for (const name of Object.keys(LIGHT_ROLE_TOKENS)) {
    root.style.removeProperty(name);
  }
}

export function subscribeTheme(fn: (theme: ThemeDefinition) => void) {
  listeners.add(fn);
  fn(getStoredTheme());
  return () => listeners.delete(fn);
}
