import { defineConfig } from "vitest/config";
import { svelte, vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

/**
 * The test suite.
 *
 * Deliberately *not* `vite.config.js`. Svelte ships a client and a server
 * entry, chosen by export condition, and the app config resolves to the server
 * one — it sets `ssr.noExternal` for SvelteKit's static build. The server entry
 * has no `mount`, so a test that renders a component dies with
 * `lifecycle_function_unavailable` before it can assert anything. Folding a
 * browser condition into the app config would fix the test and change how the
 * production bundle resolves Svelte, so the two are kept apart.
 *
 * The sveltekit plugin is absent here too, which is what lets `resolve.alias`
 * carry `$lib` directly and keeps the suite from depending on SvelteKit's
 * generated types.
 *
 * Which environment a test gets is its filename. Most of the suite is pure logic
 * — the lexer, the URL guard, the reveal budget, the avatar geometry, the
 * handoff store — and pays nothing for a DOM it never touches. Only a suite
 * that actually mounts a component is named `*.dom.test.ts` and gets jsdom.
 */

const lib = fileURLToPath(new URL("./src/lib", import.meta.url));

export default defineConfig({
  plugins: [svelte({ preprocess: vitePreprocess(), hot: false })],
  resolve: {
    // The whole reason this file exists rather than a `test` block in the app
    // config. Without it `svelte` resolves to `index-server.js`.
    conditions: ["browser"],
    alias: { $lib: lib },
  },
  test: {
    environment: "node",
    environmentMatchGlobs: [["**/*.dom.test.ts", "jsdom"]],
    include: ["src/**/*.test.ts"],
    exclude: ["**/node_modules/**", "src-tauri/**", "target/**"],
  },
});
