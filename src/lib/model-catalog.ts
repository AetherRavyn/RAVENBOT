// Shared model catalog — single source of truth for every model picker.
// Backend: `get_model_catalog` (src-tauri, frozen). Because the Rust catalog
// cannot ship model metadata, this module layers a CURATED table of
// provider/model facts (context window, $/1M pricing, capabilities) on top of
// whatever the backend returns, and live discovery
// (`fetch_provider_models` → DiscoveredModel) always wins over the curated
// static data. OpenBot pattern: runtime discovery beats any shipped catalog;
// when a price/window is unknown we render nothing rather than guess.
//
// Curated facts verified against provider pricing pages 2026-09-25
// (platform.claude.com, developers.openai.com, ai.google.dev, docs.x.ai,
// console.groq.com, api-docs.deepseek.com). Do not invent values — leave them
// undefined and the UI will show "—".

import { invoke } from "@tauri-apps/api/core";

export interface CatalogModel {
  id: string;
  name: string;
  is_free?: boolean;
  supports_vision?: boolean;
  supports_tools?: boolean;
  /** Chain-of-thought / thinking model — surfaced as a badge. */
  reasoning?: boolean;
  /** Input context window in tokens (not max output). */
  context_window?: number;
  /** USD per 1M input tokens. undefined = unknown, never a guess. */
  input_cost_per_1m?: number;
  /** USD per 1M output tokens. */
  output_cost_per_1m?: number;
}

export interface CatalogProvider {
  id: string;
  name: string;
  icon: string;
  description: string;
  key_placeholder: string;
  key_url: string;
  key_env: string;
  supports_discovery: boolean;
  keyless: boolean;
  default_model: string;
  fallback_models: CatalogModel[];
  /** User-defined provider (P8) — merged in from `list_custom_providers`. */
  custom?: boolean;
  /** Custom only: "openai" | "anthropic" | "ollama". */
  kind?: string;
  base_url?: string;
  /** Custom only: provider-level tool capability (from the settings column). */
  supports_tools?: boolean;
}

let cache: CatalogProvider[] | null = null;
let inflight: Promise<CatalogProvider[]> | null = null;

/** Latest live (discovered) models per provider id — every surface that
 *  fetches feeds this, so meta lookups see fresh data from anywhere. */
let discoveredCache: Record<string, CatalogModel[]> = {};

/** Turn `list_custom_providers` rows into catalog entries (no secrets cross). */
async function fetchCustoms(): Promise<CatalogProvider[]> {
  try {
    const rows = await invoke<any[]>("list_custom_providers");
    if (!Array.isArray(rows)) return [];
    return rows
      .filter((r) => r && r.enabled !== false)
      .map<CatalogProvider>((r) => ({
        id: String(r.id),
        name: r.display_name || String(r.id),
        icon: "🧩",
        description: r.base_url || "",
        key_placeholder: r.kind === "ollama" ? "" : "sk-...",
        key_url: "",
        key_env: "",
        supports_discovery: true,
        keyless: r.kind === "ollama",
        default_model: r.default_model || "",
        fallback_models: r.default_model
          ? [{ id: String(r.default_model), name: String(r.default_model), supports_tools: r.supports_tools !== false }]
          : [],
        custom: true,
        kind: r.kind,
        base_url: r.base_url,
        supports_tools: r.supports_tools !== false,
      }));
  } catch {
    return []; // pre-P8 backend or IPC unavailable — built-ins only
  }
}

export async function getCatalog(force = false): Promise<CatalogProvider[]> {
  if (cache && cache.length > 0 && !force) return cache;
  if (inflight && !force) return inflight;
  inflight = invoke<CatalogProvider[]>("get_model_catalog")
    .then((list) => (Array.isArray(list) && list.length > 0 ? list : minimalCatalog()))
    .catch((err) => {
      console.warn("get_model_catalog IPC unavailable, using curated static catalog:", err);
      return minimalCatalog();
    })
    .then(async (base) => {
      const merged = curate(base);
      for (const c of await fetchCustoms()) {
        if (!merged.some((p) => p.id.toLowerCase() === c.id)) merged.push(c);
      }
      cache = merged;
      return cache;
    })
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

export function getCachedCatalog(): CatalogProvider[] | null {
  return cache || curate(minimalCatalog());
}

export function providerById(list: CatalogProvider[], id: string): CatalogProvider | undefined {
  if (!id) return undefined;
  const target = id.toLowerCase();
  return list.find((p) => {
    const pid = p.id.toLowerCase();
    if (pid === target) return true;
    if (target === "claude" && pid === "anthropic") return true;
    if (target === "anthropic" && pid === "claude") return true;
    if (target === "grok" && pid === "xai") return true;
    if (target === "xai" && pid === "grok") return true;
    if ((target === "open_router" || target === "open-router") && pid === "openrouter") return true;
    return false;
  });
}

/** Normalize backend CatalogModel / DiscoveredModel JSON (snake or camel). */
function normalizeModel(raw: any): CatalogModel {
  const ctx = raw.context_window ?? raw.contextWindow;
  const inC = raw.input_cost_per_1m ?? raw.inputCostPer1m;
  const outC = raw.output_cost_per_1m ?? raw.outputCostPer1m;
  return {
    id: String(raw.id ?? ""),
    name: raw.name || raw.id || "",
    is_free: Boolean(raw.is_free ?? raw.isFree ?? (inC === 0 && outC === 0)),
    supports_vision: Boolean(raw.supports_vision ?? raw.supportsVision),
    supports_tools: Boolean(raw.supports_tools ?? raw.supportsTools),
    reasoning: Boolean(raw.reasoning),
    context_window: typeof ctx === "number" && ctx > 0 ? ctx : undefined,
    input_cost_per_1m: typeof inC === "number" ? inC : undefined,
    output_cost_per_1m: typeof outC === "number" ? outC : undefined,
  };
}

/** Merge curated meta under live discovery (live fields win when present). */
function mergeMeta(curated: CatalogModel | undefined, live: CatalogModel): CatalogModel {
  if (!curated) return live;
  return {
    ...curated,
    ...live,
    context_window: live.context_window ?? curated.context_window,
    input_cost_per_1m: live.input_cost_per_1m ?? curated.input_cost_per_1m,
    output_cost_per_1m: live.output_cost_per_1m ?? curated.output_cost_per_1m,
    supports_vision: live.supports_vision || Boolean(curated.supports_vision),
    supports_tools: live.supports_tools || Boolean(curated.supports_tools),
    reasoning: live.reasoning || Boolean(curated.reasoning),
    is_free: live.is_free || Boolean(curated.is_free),
  };
}

/** Remember a live discovery result for meta lookups elsewhere. */
export function noteDiscovery(providerId: string, rawModels: any[]): void {
  if (!providerId || !Array.isArray(rawModels)) return;
  discoveredCache[providerId.toLowerCase()] = rawModels.map(normalizeModel);
}

/**
 * Best-known metadata for one model: live discovery first, then the curated
 * lists, then a vendor-prefix hop for gateways (openrouter "anthropic/x" →
 * anthropic's own entry). Returns null when we genuinely know nothing.
 */
export function modelMetaFor(providerId: string, modelId: string): CatalogModel | null {
  if (!providerId || !modelId) return null;
  const pid = providerId.toLowerCase();
  const found =
    discoveredCache[pid]?.find((m) => m.id === modelId) ??
    curatedFor(pid).find((m) => m.id === modelId);
  if (found) return found;
  // Gateway ids look like "<vendor>/<model>" — inherit the vendor's facts.
  const slash = modelId.indexOf("/");
  if (slash > 0) {
    const vendor = modelId.slice(0, slash).toLowerCase();
    const sub = modelId.slice(slash + 1);
    const viaVendor =
      discoveredCache[vendor]?.find((m) => m.id === sub || sub.startsWith(m.id)) ??
      curatedFor(vendor).find((m) => m.id === sub || sub.startsWith(m.id));
    if (viaVendor) return viaVendor;
  }
  return null;
}

function curatedFor(pid: string): CatalogModel[] {
  const entry = CURATED[pid];
  if (entry) return entry.models;
  const base = cache?.find((p) => p.id.toLowerCase() === pid);
  return base?.fallback_models ?? [];
}

/** Live models for a provider: discovered API list wins, catalog fallbacks
 *  otherwise. Never returns a bare empty list while loading is settled. */
export function modelsFor(
  provider: CatalogProvider | undefined,
  discovered: any[],
  loading: boolean,
): CatalogModel[] {
  if (discovered && discovered.length > 0) {
    if (provider) noteDiscovery(provider.id, discovered);
    const curated = provider ? curatedFor(provider.id.toLowerCase()) : [];
    return discovered.map((raw: any) => {
      const live = normalizeModel(raw);
      const stat = curated.find((c) => c.id === live.id);
      return stat ? mergeMeta(stat, live) : live;
    });
  }
  if (loading) return [];
  return provider?.fallback_models ?? [];
}

/** Provider temperature ceiling: gateways with strict validation (notably
 *  CommandCode ≤ 1) reject higher values with a 400. The Rust providers also
 *  clamp server-side — this keeps the slider honest up front. */
export function temperatureMax(providerId: string): number {
  if (providerId === "commandcode") return 1;
  return 2;
}

/** "200K" / "1M" for a token count; "" when unknown. Binary rounding:
 *  131072 → "128K", 1048576 → "1M". */
export function formatContext(tokens?: number | null): string {
  if (!tokens || tokens <= 0) return "";
  if (tokens >= 1024 * 1024) {
    const m = tokens / (1024 * 1024);
    return `${m % 1 === 0 ? m : m.toFixed(1)}M`;
  }
  return `${Math.round(tokens / 1024)}K`;
}

/** "$2 / $10" (input / output per 1M tokens); null when either side is
 *  unknown — we never render a guessed price. */
export function formatPricing(m: CatalogModel | null | undefined): string | null {
  if (!m || m.input_cost_per_1m === undefined || m.output_cost_per_1m === undefined) return null;
  const trim = (n: number) => (Number.isInteger(n) ? `${n}` : n.toFixed(2).replace(/0$/, ""));
  return `$${trim(m.input_cost_per_1m)} / $${trim(m.output_cost_per_1m)}`;
}

/** One-line honest summary for a model row: "1M · $2/$10/1M" or a piece of it. */
export function modelSummary(m: CatalogModel | null | undefined): string {
  if (!m) return "";
  const parts: string[] = [];
  const ctx = formatContext(m.context_window);
  if (ctx) parts.push(ctx);
  const price = formatPricing(m);
  if (price) parts.push(`${price}/1M`);
  if (m.is_free) parts.push("free");
  return parts.join(" · ");
}

// ── Curated provider facts layered over the frozen backend catalog ─────────
// Only replaces fallback_models/default_model/description for the ids listed.
// Gateways we cannot verify (commandcode, opencode, cline, tokenrouter, mimo)
// keep whatever the backend ships.

interface CuratedProvider {
  description: string;
  default_model: string;
  models: CatalogModel[];
}

const mm = (
  id: string,
  name: string,
  extra: Partial<CatalogModel> = {},
): CatalogModel => ({ id, name, ...extra });

export const CURATED: Record<string, CuratedProvider> = {
  anthropic: {
    description: "Claude Fable 5, Opus 5, Sonnet 5 & Haiku 4.5 — 1M-context frontier models",
    default_model: "claude-sonnet-5",
    models: [
      mm("claude-fable-5", "Claude Fable 5", { context_window: 1048576, input_cost_per_1m: 10, output_cost_per_1m: 50, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("claude-opus-5", "Claude Opus 5", { context_window: 1048576, input_cost_per_1m: 4, output_cost_per_1m: 20, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("claude-sonnet-5", "Claude Sonnet 5", { context_window: 1048576, input_cost_per_1m: 2, output_cost_per_1m: 10, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("claude-haiku-4-5", "Claude Haiku 4.5", { context_window: 204800, input_cost_per_1m: 1, output_cost_per_1m: 5, supports_vision: true, supports_tools: true }),
    ],
  },
  openai: {
    description: "GPT-6 (Astra, Sol, Luna) + GPT-5 Pro & o3 reasoning",
    default_model: "gpt-6-sol",
    models: [
      mm("gpt-6-astra", "GPT-6 Astra", { context_window: 278528, input_cost_per_1m: 10, output_cost_per_1m: 50, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("gpt-6-sol", "GPT-6 Sol", { context_window: 278528, input_cost_per_1m: 2, output_cost_per_1m: 10, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("gpt-6-luna", "GPT-6 Luna", { context_window: 278528, input_cost_per_1m: 0.1, output_cost_per_1m: 0.5, supports_vision: true, supports_tools: true }),
      mm("gpt-5-pro", "GPT-5 Pro", { context_window: 278528, input_cost_per_1m: 15, output_cost_per_1m: 120, supports_tools: true, reasoning: true }),
      mm("o3-mini", "o3-mini", { context_window: 278528, input_cost_per_1m: 1.1, output_cost_per_1m: 4.4, supports_tools: true, reasoning: true }),
    ],
  },
  gemini: {
    description: "Gemini 3.5 Flash & 2.5 Pro/Flash with up to 1M context",
    default_model: "gemini-2.5-flash",
    models: [
      mm("gemini-3.5-flash", "Gemini 3.5 Flash", { context_window: 1048576, input_cost_per_1m: 1.5, output_cost_per_1m: 9, supports_vision: true, supports_tools: true }),
      mm("gemini-3.1-flash-lite", "Gemini 3.1 Flash Lite", { context_window: 1048576, input_cost_per_1m: 0.25, output_cost_per_1m: 1.5, supports_vision: true, supports_tools: true }),
      mm("gemini-2.5-pro", "Gemini 2.5 Pro", { context_window: 1048576, input_cost_per_1m: 1.25, output_cost_per_1m: 10, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("gemini-2.5-flash", "Gemini 2.5 Flash", { context_window: 1048576, input_cost_per_1m: 0.3, output_cost_per_1m: 2.5, supports_vision: true, supports_tools: true }),
      mm("gemini-2.5-flash-lite", "Gemini 2.5 Flash Lite", { context_window: 1048576, input_cost_per_1m: 0.1, output_cost_per_1m: 0.4, supports_vision: true, supports_tools: true }),
    ],
  },
  deepseek: {
    description: "DeepSeek V4.1 Flash & V4 Pro — 1M context at open-frontier prices",
    default_model: "deepseek-chat",
    models: [
      mm("deepseek-chat", "DeepSeek V4.1 Flash", { context_window: 1048576, input_cost_per_1m: 0.3, output_cost_per_1m: 1.2, supports_tools: true }),
      mm("deepseek-reasoner", "DeepSeek V4 Pro (Reasoner)", { context_window: 1048576, input_cost_per_1m: 1.32, output_cost_per_1m: 3.96, supports_tools: true, reasoning: true }),
    ],
  },
  xai: {
    description: "Grok 4.7 & 4.3 — long-context models with live X data grounding",
    default_model: "grok-4.7",
    models: [
      mm("grok-4.7", "Grok 4.7", { context_window: 512000, input_cost_per_1m: 2, output_cost_per_1m: 6, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("grok-4.3", "Grok 4.3", { context_window: 1048576, input_cost_per_1m: 1.25, output_cost_per_1m: 2.5, supports_vision: true, supports_tools: true }),
    ],
  },
  groq: {
    description: "LPU-speed inference: Llama 3.3/3.1 and gpt-oss",
    default_model: "llama-3.3-70b-versatile",
    models: [
      mm("llama-3.3-70b-versatile", "Llama 3.3 70B Versatile", { context_window: 131072, supports_tools: true }),
      mm("llama-3.1-8b-instant", "Llama 3.1 8B Instant", { context_window: 131072, supports_tools: true }),
      mm("openai/gpt-oss-120b", "gpt-oss 120B", { context_window: 131072, supports_tools: true, reasoning: true }),
      mm("openai/gpt-oss-20b", "gpt-oss 20B", { context_window: 131072, supports_tools: true, reasoning: true }),
    ],
  },
  mistral: {
    description: "Mistral Large 3, Medium 3.5, Small & Codestral",
    default_model: "mistral-large-latest",
    models: [
      mm("mistral-large-latest", "Mistral Large", { supports_tools: true }),
      mm("mistral-medium-latest", "Mistral Medium 3.5", { supports_tools: true }),
      mm("mistral-small-latest", "Mistral Small", { supports_tools: true }),
      mm("codestral-latest", "Codestral", { context_window: 262144, supports_tools: true }),
    ],
  },
  openrouter: {
    description: "300+ models, unified API, pay-as-you-go aggregator",
    default_model: "anthropic/claude-sonnet-5",
    models: [
      mm("anthropic/claude-sonnet-5", "Claude Sonnet 5", { context_window: 1048576, input_cost_per_1m: 2, output_cost_per_1m: 10, supports_vision: true, supports_tools: true, reasoning: true }),
      mm("openai/gpt-6-sol", "GPT-6 Sol", { context_window: 278528, input_cost_per_1m: 2, output_cost_per_1m: 10, supports_vision: true, supports_tools: true }),
      mm("google/gemini-2.5-flash", "Gemini 2.5 Flash", { context_window: 1048576, input_cost_per_1m: 0.3, output_cost_per_1m: 2.5, supports_vision: true, supports_tools: true }),
      mm("deepseek/deepseek-chat", "DeepSeek V4.1 Flash", { context_window: 1048576, input_cost_per_1m: 0.3, output_cost_per_1m: 1.2, supports_tools: true }),
      mm("meta-llama/llama-3.3-70b-instruct", "Llama 3.3 70B", { context_window: 131072, supports_tools: true }),
    ],
  },
  together: {
    description: "Fast inference for leading open-source LLMs",
    default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
    models: [
      mm("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B Turbo", { context_window: 131072, supports_tools: true }),
      mm("deepseek-ai/DeepSeek-R1", "DeepSeek R1 (Together)", { context_window: 163840, reasoning: true }),
      mm("Qwen/Qwen2.5-72B-Instruct-Turbo", "Qwen 2.5 72B Turbo", { context_window: 131072 }),
    ],
  },
  perplexity: {
    description: "Sonar search models with live internet citations & grounding",
    default_model: "sonar",
    models: [
      mm("sonar", "Sonar", { context_window: 131072 }),
      mm("sonar-pro", "Sonar Pro", { context_window: 204800 }),
      mm("sonar-reasoning-pro", "Sonar Reasoning Pro", { context_window: 131072, reasoning: true }),
    ],
  },
  cohere: {
    description: "Command R+ & Command R enterprise multilingual & tool models",
    default_model: "command-r-plus",
    models: [
      mm("command-r-plus", "Command R+", { context_window: 131072, supports_tools: true }),
      mm("command-r", "Command R", { context_window: 131072, supports_tools: true }),
    ],
  },
  ollama: {
    description: "100% sovereign offline inference — live list via probe_ollama",
    default_model: "llama3.1",
    models: [
      mm("llama3.1", "Llama 3.1", { context_window: 131072, is_free: true }),
      mm("llama3.1:8b", "Llama 3.1 8B", { context_window: 131072, is_free: true }),
      mm("mistral", "Mistral 7B", { context_window: 32768, is_free: true }),
      mm("deepseek-r1:8b", "DeepSeek R1 8B", { context_window: 131072, is_free: true, reasoning: true }),
    ],
  },
};

function curate(list: CatalogProvider[]): CatalogProvider[] {
  return list.map((p) => {
    const c = CURATED[p.id.toLowerCase()];
    if (!c) return p;
    return {
      ...p,
      description: c.description ?? p.description,
      default_model: c.default_model,
      fallback_models: c.models,
    };
  });
}

export function minimalCatalog(): CatalogProvider[] {
  const m = (id: string, name: string): CatalogModel => ({ id, name });
  const mv = (id: string, name: string, supports_vision = true): CatalogModel => ({ id, name, supports_vision });
  const mf = (id: string, name: string): CatalogModel => ({ id, name, is_free: true });

  const provider = (
    id: string,
    name: string,
    icon: string,
    description: string,
    key_placeholder: string,
    key_url: string,
    key_env: string,
    default_model: string,
    fallback_models: CatalogModel[],
    keyless = false,
  ): CatalogProvider => ({
    id, name, icon, description,
    key_placeholder, key_url, key_env,
    supports_discovery: true, keyless, default_model, fallback_models,
  });

  return [
    provider("commandcode", "Command Code", "⚡", "OpenAI & Anthropic proxy, high-rate limits, free models", "cc_... or rc_...", "https://commandcode.ai", "COMMANDCODE_API_KEY", "claude-sonnet-4-6", [
      mv("claude-sonnet-4-6", "Claude Sonnet 4 (CC)"),
      m("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash (CC)"),
      mf("longcat-2.0:free", "LongCat 2.0 (Free)"),
    ]),
    provider("opencode", "OpenCode", "💻", "OpenCode Zen coding agent & multi-model LLM gateway", "sk-...", "https://opencode.ai/auth", "OPENCODE_API_KEY", "claude-sonnet-4-5", [
      mv("claude-sonnet-4-5", "Claude Sonnet 4.5"),
      mv("gpt-5", "GPT-5"),
      m("deepseek-v4-pro", "DeepSeek V4 Pro"),
      m("gemini-3-flash", "Gemini 3 Flash"),
    ]),
    provider("anthropic", "Anthropic Claude", "🧠", CURATED.anthropic.description, "sk-ant-...", "https://console.anthropic.com", "ANTHROPIC_API_KEY", CURATED.anthropic.default_model, CURATED.anthropic.models),
    provider("cline", "Cline", "🤖", "Autonomous coding agent gateway & multi-model routing", "sk-...", "https://cline.bot", "CLINE_API_KEY", "claude-sonnet-4-5", [
      mv("claude-sonnet-4-5", "Claude Sonnet 4.5"),
      mv("openai/gpt-4o", "GPT-4o"),
      m("deepseek/deepseek-chat", "DeepSeek Chat"),
    ]),
    provider("openrouter", "OpenRouter", "🌐", CURATED.openrouter.description, "sk-or-v1-...", "https://openrouter.ai", "OPENROUTER_API_KEY", CURATED.openrouter.default_model, CURATED.openrouter.models),
    provider("openai", "OpenAI", "🔵", CURATED.openai.description, "sk-proj-...", "https://platform.openai.com", "OPENAI_API_KEY", CURATED.openai.default_model, CURATED.openai.models),
    provider("gemini", "Google Gemini", "✨", CURATED.gemini.description, "AIzaSy...", "https://aistudio.google.com", "GEMINI_API_KEY", CURATED.gemini.default_model, CURATED.gemini.models),
    provider("deepseek", "DeepSeek", "🐋", CURATED.deepseek.description, "sk-...", "https://platform.deepseek.com", "DEEPSEEK_API_KEY", CURATED.deepseek.default_model, CURATED.deepseek.models),
    provider("groq", "Groq LPU", "🚀", CURATED.groq.description, "gsk_...", "https://console.groq.com", "GROQ_API_KEY", CURATED.groq.default_model, CURATED.groq.models),
    provider("xai", "xAI Grok", "🌌", CURATED.xai.description, "xai-...", "https://console.x.ai", "XAI_API_KEY", CURATED.xai.default_model, CURATED.xai.models),
    provider("mistral", "Mistral AI", "🌪️", CURATED.mistral.description, "...", "https://console.mistral.ai", "MISTRAL_API_KEY", CURATED.mistral.default_model, CURATED.mistral.models),
    provider("together", "Together AI", "🤝", CURATED.together.description, "...", "https://api.together.xyz", "TOGETHER_API_KEY", CURATED.together.default_model, CURATED.together.models),
    provider("perplexity", "Perplexity AI", "🔍", CURATED.perplexity.description, "pplx-...", "https://perplexity.ai", "PERPLEXITY_API_KEY", CURATED.perplexity.default_model, CURATED.perplexity.models),
    provider("cohere", "Cohere", "🌲", CURATED.cohere.description, "...", "https://cohere.com", "COHERE_API_KEY", CURATED.cohere.default_model, CURATED.cohere.models),
    provider("mimo", "Xiaomi MiMo", "📱", "High-efficiency multilingual reasoning models & edge AI", "...", "https://api.mimo.mi.com", "MIMO_API_KEY", "mimo-v2.5", [
      m("mimo-v2.5", "MiMo V2.5"),
      m("mimo-v2.5-pro", "MiMo V2.5 Pro"),
    ]),
    provider("ollama", "Ollama (Local)", "🦙", CURATED.ollama.description, "http://localhost:11434", "https://ollama.com", "OLLAMA_HOST", CURATED.ollama.default_model, CURATED.ollama.models, true),
    provider("tokenrouter", "TokenRouter", "🧭", "Unified multi-model hub — OpenAI/Claude/Gemini-compatible with global routing & failover", "sk-...", "https://www.tokenrouter.com/console/token", "TOKENROUTER_API_KEY", "deepseek/deepseek-v4.1-flash", [
      m("deepseek/deepseek-v4.1-flash", "DeepSeek V4.1 Flash"),
      m("z-ai/glm-5.3", "GLM 5.3"),
      m("qwen/qwen3.8-max", "Qwen 3.8 Max"),
      m("moonshotai/kimi-k2.7-code", "Kimi K2.7 Code"),
      mv("openai/gpt-5-mini", "GPT-5 Mini"),
      m("x-ai/grok-4.6", "Grok 4.6"),
      mf("z-ai/glm-5.3-free", "GLM 5.3 (Free)"),
    ]),
  ];
}
