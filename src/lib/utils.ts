import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import {
  diceStyle,
  dicebearStyles as allDiceStyles,
  avatarBackground,
  type DiceStyle,
} from "$lib/diceStyles";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export type WithoutChild<T> = T extends { child?: any } ? Omit<T, "child"> : T;
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, "children"> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
export type WithoutChildOrChildren<T> = WithoutChildrenOrChild<T>;

// Backend parity uses lowercase roles, but earlier Tauri payloads and
// locally constructed messages can use capitalized variants.
export type ChatRole = "user" | "assistant" | "system" | "tool";

export function normalizeMessageRole(role: unknown): ChatRole | null {
  if (typeof role !== "string") return null;
  const normalized = role.trim().toLowerCase();
  if (
    normalized === "user" ||
    normalized === "assistant" ||
    normalized === "system" ||
    normalized === "tool"
  ) {
    return normalized;
  }
  return null;
}

export function isUserMessage(message: { role?: unknown } | null | undefined): boolean {
  return normalizeMessageRole(message?.role) === "user";
}

export function isAssistantMessage(
  message: { role?: unknown } | null | undefined,
): boolean {
  return normalizeMessageRole(message?.role) === "assistant";
}

// DiceBear & Native Raven Asset helpers

/**
 * The DiceBear API major version to request.
 *
 * 10.x, because that is where the animated styles live. 9.x has no
 * `animationVariant` option, so a 9.x URL for `voxel-bot` returns a still robot
 * and there is no way to ask for the blink — the feature is simply absent from
 * the older major. Every style the app already offered still resolves on 10.x.
 */
import { DICEBEAR_VERSION } from "$lib/diceVersions";

/** Style slugs that are drawn by this app rather than fetched. */
const NATIVE_STYLES = new Set([
  "raven-native",
  "ravenicon",
  "raven-avatar",
  "raven-brandmark",
  "raven-logo-hex",
]);

/** Whether a style is drawn locally instead of requested from DiceBear. */
export function isNativeAvatarStyle(style: string): boolean {
  return NATIVE_STYLES.has(style);
}

/**
 * Build a DiceBear avatar URL.
 *
 * `animated` defaults to the style's own setting, so a caller that knows nothing
 * about animation still gets a live face for `clay` and a still one for
 * `notionists-neutral`. Pass `false` to force a still avatar, which is what a
 * user who has turned motion off gets.
 *
 * The animation is not a separate file: 10.x puts the CSS keyframes inside the
 * SVG and gates them on `prefers-reduced-motion`, so a plain `<img>` plays them
 * and a visitor who asks for less motion sees a still avatar without anything
 * here noticing.
 */
export function getDiceBearUrl(
  seed: string,
  style: string = "avataaars",
  extra: string = "",
  animated?: boolean,
): string {
  if (isNativeAvatarStyle(style)) return "";

  const cleanSeed = seed.trim() || "Agent";
  const meta = diceStyle(style);
  const params = new URLSearchParams({
    seed: cleanSeed,
    // One colour, chosen from the name. See `AVATAR_BACKGROUNDS` for why a list
    // cannot be sent: 10.x validates the raw query string against a single-colour
    // pattern, and an encoded comma list is a 400 that renders as a blank frame.
    backgroundColor: avatarBackground(cleanSeed),
    radius: "50",
    ...Object.fromEntries(new URLSearchParams(extra)),
  });

  // A style we do not know is assumed static rather than sent a speed: an
  // unrecognised `animationVariant` is not an error, it silently yields a still
  // avatar, so asking costs nothing and gains nothing.
  const wantsMotion = animated ?? meta?.animated ?? false;
  if (wantsMotion && meta?.speed) params.set("animationVariant", meta.speed);

  return `https://api.dicebear.com/${DICEBEAR_VERSION}/${style}/svg?${params.toString()}`;
}

/**
 * The avatar style picker list.
 *
 * Re-exported from `$lib/diceStyles`, which is where the list and the animation
 * facts now live. Two call sites still import it from here, and there is no
 * reason to make them care which file a list of styles is written down in.
 */
export function dicebearStyles(): DiceStyle[] {
  return allDiceStyles();
}

export type { DiceStyle, DiceSpeed } from "$lib/diceStyles";

// Office templates for chatrooms with rank-based distribution
export const OFFICE_TEMPLATES = {
  "rot-archive": {
    name: "The Rot Archive",
    icon: "book-open",
    description: "Forbidden alchemical & anatomical research team",
    ranks: [
      { rank: "Grand Archivist", specialty: "Forbidden Codex & Grimoire Curation", color: "#8B1E1E", style: "bottts" },
      { rank: "Plague Surgeon", specialty: "Anatomical Mutation & Pathology", color: "#2D3F31", style: "personas" },
      { rank: "Hermetic Alchemist", specialty: "Transmutation & Elixir Synthesis", color: "#C8B89B", style: "lorelei" },
      { rank: "Inquisitor", specialty: "Seal Verification & Containment", color: "#5C3B2E", style: "adventurer" },
      { rank: "Scribe", specialty: "Marginalia & Cypher Decryption", color: "#E5E0D8", style: "notionists" },
    ],
  },
  "it-office": {
    name: "IT Office",
    icon: "laptop",
    description: "Full-stack engineering team",
    ranks: [
      { rank: "CTO", specialty: "System Architecture", color: "#8b5cf6", style: "bottts" },
      { rank: "Tech Lead", specialty: "Backend & DevOps", color: "#6366f1", style: "avataaars" },
      { rank: "Senior Dev", specialty: "Frontend & Mobile", color: "#06b6d4", style: "personas" },
      { rank: "QA Engineer", specialty: "Testing & Automation", color: "#10b981", style: "lorelei" },
      { rank: "DevOps", specialty: "Infrastructure", color: "#f59e0b", style: "identicon" },
    ],
  },
  "marketing": {
    name: "Marketing Agency",
    icon: "trending-up",
    description: "Growth & brand team",
    ranks: [
      { rank: "CMO", specialty: "Strategy & Brand", color: "#ec4899", style: "bottts" },
      { rank: "Growth Lead", specialty: "Performance & Analytics", color: "#f59e0b", style: "avataaars" },
      { rank: "Content Creator", specialty: "Copy & Video", color: "#06b6d4", style: "lorelei" },
      { rank: "Designer", specialty: "UI/UX & Visual", color: "#8b5cf6", style: "personas" },
      { rank: "SEO Specialist", specialty: "Organic & Research", color: "#6366f1", style: "adventurer" },
    ],
  },
  "sales": {
    name: "Sales Office",
    icon: "briefcase",
    description: "Revenue & outreach",
    ranks: [
      { rank: "VP Sales", specialty: "Strategy & Closing", color: "#f59e0b", style: "bottts" },
      { rank: "Account Exec", specialty: "Outbound & Demos", color: "#6366f1", style: "avataaars" },
      { rank: "SDR", specialty: "Prospecting & Qualification", color: "#10b981", style: "personas" },
      { rank: "CS Manager", specialty: "Retention & Upsell", color: "#06b6d4", style: "lorelei" },
    ],
  },
  "design": {
    name: "Design Studio",
    icon: "palette",
    description: "Product & brand design",
    ranks: [
      { rank: "Design Director", specialty: "Vision & System", color: "#ec4899", style: "bottts" },
      { rank: "Product Designer", specialty: "UX & Flows", color: "#6366f1", style: "avataaars" },
      { rank: "Brand Designer", specialty: "Identity & Assets", color: "#f59e0b", style: "lorelei" },
      { rank: "Motion Designer", specialty: "Animation & Prototype", color: "#8b5cf6", style: "adventurer" },
    ],
  },
  "custom": {
    name: "Custom Office",
    icon: "building-2",
    description: "Build your own team",
    ranks: [],
  },
} as const;

export type OfficeTemplateKey = keyof typeof OFFICE_TEMPLATES;

