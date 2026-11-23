#!/usr/bin/env node
/**
 * UI foundation gate.
 *
 * The design system is enforced, not suggested. Three rules are hard zeros:
 * no raw hex colors in Svelte styles/inline styles, no !important, and no
 * off-scale text. Native <select> use outside components/ui is allowed only at
 * the counts recorded in the baseline, so existing debt cannot grow.
 *
 * Run: node scripts/check-ui-foundation.mjs
 * Update the native-select baseline: --update
 */
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = process.cwd();
const UI_DIR = "src/lib/components/ui/";
const BASELINE_PATH = "scripts/ui-foundation-baseline.json";
const SCALE = new Set(["11", "12", "13", "14", "16", "18", "20", "24"]);

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (entry.endsWith(".svelte")) out.push(full);
  }
  return out;
}

const files = walk("src");
const rawHex = [];
const important = [];
const offScaleText = [];
const nativeSelect = {};

for (const file of files) {
  const src = readFileSync(file, "utf8");
  const styleBlocks = [...src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)]
    .map((match) => match[1])
    .join("\n");
  const inlineStyles = [...src.matchAll(/\bstyle=(?:"([^"]*)"|\{([^}]*)\})/g)]
    .map((match) => match[1] || match[2])
    .join("\n");
  const styled = `${styleBlocks}\n${inlineStyles}`;

  for (const match of styled.matchAll(/#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/g)) {
    rawHex.push(`${file}: ${match[0]}`);
  }
  if (/!important\b/.test(src)) important.push(file);
  for (const match of src.matchAll(/text-\[(\d+(?:\.\d+)?)px\]/g)) {
    if (!SCALE.has(String(parseFloat(match[1])))) {
      offScaleText.push(`${file}: text-[${match[1]}px]`);
    }
  }
  if (!file.startsWith(UI_DIR)) {
    const count = (src.match(/<select\b/g) || []).length;
    if (count > 0) nativeSelect[file] = count;
  }
}

const baseline = existsSync(BASELINE_PATH)
  ? JSON.parse(readFileSync(BASELINE_PATH, "utf8"))
  : { nativeSelect: {} };
const previous = baseline.nativeSelect ?? {};
const nativeSelectGrowth = [];
for (const [file, count] of Object.entries(nativeSelect)) {
  const allowed = previous[file] ?? 0;
  if (count > allowed) {
    nativeSelectGrowth.push(
      `${file}: ${count} native <select> (baseline ${allowed})`,
    );
  }
}

const issues = [];
if (rawHex.length) {
  issues.push(`Raw hex colors in Svelte styles:\n  ${rawHex.join("\n  ")}`);
}
if (important.length) {
  issues.push(`!important is not allowed:\n  ${important.join("\n  ")}`);
}
if (offScaleText.length) {
  issues.push(`Off-scale text sizes:\n  ${offScaleText.join("\n  ")}`);
}
if (nativeSelectGrowth.length) {
  issues.push(
    `Native <select> outside components/ui grew beyond baseline:\n  ${nativeSelectGrowth.join("\n  ")}`,
  );
}

if (process.argv.includes("--update")) {
  writeFileSync(
    BASELINE_PATH,
    `${JSON.stringify({ nativeSelect }, null, 2)}\n`,
  );
  console.log(`Updated ${BASELINE_PATH} (${Object.keys(nativeSelect).length} files).`);
  process.exit(0);
}

if (issues.length) {
  console.error("UI foundation gate failed.\n");
  console.error(issues.join("\n\n"));
  console.error(
    "\nUse design tokens (var(--…)), the text scale, and components/ui. " +
      "Only the native-select baseline can be refreshed with --update.",
  );
  process.exit(1);
}

console.log(
  `UI foundation OK (${files.length} Svelte files, 0 raw hex, 0 !important, ` +
    `0 off-scale text, native selects within baseline).`,
);
