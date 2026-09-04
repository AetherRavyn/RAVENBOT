import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export type WithoutChild<T> = T extends { child?: any } ? Omit<T, "child"> : T;
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, "children"> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
export type WithoutChildOrChildren<T> = WithoutChildrenOrChild<T>;

// DiceBear & Native Raven Asset helpers
export function getDiceBearUrl(seed: string, style: string = "avataaars", extra: string = ""): string {
  if (style === "raven-native" || style === "ravenicon" || style === "raven-avatar" || style === "raven-brandmark" || style === "raven-logo-hex") {
    return "/ravenicon.png";
  }

  const cleanSeed = seed.trim() || "Agent";
  const base = `https://api.dicebear.com/9.x/${style}/svg`;
  const params = new URLSearchParams({
    seed: cleanSeed,
    backgroundColor: "8B1E1E,C8B89B,2D3F31,5C3B2E,6366f1,8b5cf6,06b6d4,ec4899",
    radius: "50",
    ...Object.fromEntries(new URLSearchParams(extra)),
  });
  return `${base}?${params.toString()}`;
}

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

export function dicebearStyles(): { value: string; label: string; category: string; description: string }[] {
  return [
    { value: "bottts", label: "Bottts (Robots)", category: "Robots & AI", description: "Androids & AI bots" },
    { value: "avataaars", label: "Avataaars", category: "Characters", description: "Modern illustrated avatars" },
    { value: "personas", label: "Personas", category: "Characters", description: "Clean corporate personas" },
    { value: "lorelei", label: "Lorelei", category: "Characters", description: "Anime & illustrated faces" },
    { value: "adventurer", label: "Adventurer", category: "Fantasy", description: "RPG heroes & adventurers" },
    { value: "micah", label: "Micah", category: "Modern", description: "Minimalist vector avatars" },
    { value: "notionists", label: "Notionists", category: "Modern", description: "Notion-style line avatars" },
    { value: "open-peeps", label: "Open Peeps", category: "Doodles", description: "Hand-drawn diverse doodles" },
    { value: "pixel-art", label: "Pixel Art", category: "Retro", description: "Retro 8-bit characters" },
    { value: "big-smile", label: "Big Smile", category: "Expressive", description: "Joyful smiling characters" },
    { value: "croodles", label: "Croodles", category: "Doodles", description: "Playful artistic sketches" },
    { value: "dylan", label: "Dylan", category: "Modern", description: "Stylized expressive avatars" },
    { value: "identicon", label: "Identicon", category: "Geometric", description: "Cryptographic geometric patterns" },
    { value: "shapes", label: "Shapes", category: "Geometric", description: "Abstract Bauhaus shapes" },
    { value: "rings", label: "Rings", category: "Geometric", description: "Concentric radiant rings" },
    { value: "thumbs", label: "Thumbs", category: "Playful", description: "Fun character thumbs" },
    { value: "fun-emoji", label: "Fun Emoji", category: "Playful", description: "Cheerful 3D emoji faces" },
    { value: "ravenicon", label: "Raven Cyber", category: "Sovereign", description: "Native Raven OS Emblem" },
  ];
}
