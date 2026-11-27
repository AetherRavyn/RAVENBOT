#!/usr/bin/env node
/**
 * No-payment gate.
 *
 * RAVENBOT is MIT and has no paywall, license gate or built-in payment rail.
 * Optional third-party catalogs (MCP/OpenAPI) may list Stripe or PayPal as
 * integrations, but the desktop app must not depend on a payment SDK. This gate
 * fails if a payment package appears in any package.json in the repository.
 *
 * Run: node scripts/check-no-payments.mjs
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const SKIP = new Set(["node_modules", "target", "build", ".git", ".svelte-kit"]);
const FORBIDDEN = [
  "stripe",
  "@stripe/stripe-js",
  "@stripe/react-stripe-js",
  "paypal",
  "@paypal/paypal-js",
  "razorpay",
  "braintree",
  "@square/web-payments-sdk",
  "@paddle/paddle-js",
  "@lemonsqueezy/lemonsqueezy.js",
  "revenuecat",
  "@revenuecat/purchases-js",
  "checkout.com",
];

const failures = [];

function walk(dir) {
  for (const entry of readdirSync(dir)) {
    if (SKIP.has(entry)) continue;
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) walk(full);
    else if (entry === "package.json") inspect(full);
  }
}

function inspect(file) {
  let parsed;
  try {
    parsed = JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    failures.push(`${relative(ROOT, file)}: invalid package.json (${error.message})`);
    return;
  }
  const deps = { ...parsed.dependencies, ...parsed.devDependencies, ...parsed.peerDependencies };
  for (const name of Object.keys(deps)) {
    if (FORBIDDEN.includes(name)) {
      failures.push(`${relative(ROOT, file)}: payment SDK dependency "${name}"`);
    }
  }
}

walk(ROOT);

if (failures.length > 0) {
  console.error("No-payment gate failed:");
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}
console.log("No-payment gate OK (no payment SDK dependencies).");
