#!/usr/bin/env node
/**
 * The accessible name is user-facing copy, not a debug string.
 *
 * A literal `aria-label="Close"` is invisible to the translation pipeline and
 * ships English to every locale. This gate keeps the older surfaces from
 * drifting back: every Svelte `aria-label` must be an expression, usually
 * `aria-label={t("…")}`. Brand names and other deliberately untranslated
 * values can be expressions too.
 *
 * It intentionally does not inspect `title`, `placeholder`, or visible text —
 * those are a larger i18n pass and a false failure here would make the gate
 * easy to ignore.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = "src";
const LITERAL = /aria-label\s*=\s*["'][A-Za-z]/;

function* svelteFiles(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    const stat = statSync(path);
    if (stat.isDirectory()) yield* svelteFiles(path);
    else if (name.endsWith(".svelte")) yield path;
  }
}

const offenders = [];
let files = 0;
for (const path of svelteFiles(ROOT)) {
  files += 1;
  const lines = readFileSync(path, "utf8").split("\n");
  lines.forEach((line, index) => {
    if (LITERAL.test(line)) offenders.push(`${path}:${index + 1}: ${line.trim()}`);
  });
}

if (offenders.length > 0) {
  console.error("Hardcoded aria-label values found:");
  for (const offender of offenders) console.error(`  ${offender}`);
  console.error("\nUse aria-label={t(\"a11y.…\")} so the label follows the locale.");
  process.exit(1);
}

console.log(`aria-label gate OK (${files} Svelte files, no literal English names).`);
