import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/**
 * Guard against a real dev-server crash.
 *
 * `@tailwindcss/vite` (v4) runs an `enforce: "pre"` transform whose id filter
 * matches Svelte's `*.svelte?svelte&type=style&lang.css` virtual module. When
 * that module is requested before the component has been compiled, Vite falls
 * back to reading the raw `.svelte` file and Tailwind tries to parse the whole
 * component — script and all — as CSS, killing the dev server with
 * `Invalid declaration: <first js token>`.
 *
 * The fix is to keep all styling in global CSS (`src/lib/styles/components.css`)
 * and Tailwind utility classes, never in a Svelte `<style>` block. This test
 * fails loudly if one is reintroduced.
 */

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      walk(full, out);
    } else if (entry.endsWith(".svelte")) {
      out.push(full);
    }
  }
  return out;
}

describe("component styles", () => {
  it("no .svelte component uses a <style> block", () => {
    const files = walk("src");
    const offenders = files.filter((f) =>
      /<style(\s|>)/.test(readFileSync(f, "utf8")),
    );
    expect(
      offenders,
      `Move these components' CSS into src/lib/styles/components.css — a <style> block breaks the Tailwind/Vite dev server:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });
});
