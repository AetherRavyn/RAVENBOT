import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  // NOTE: components must not use Svelte `<style>` blocks. @tailwindcss/vite
  // matches the `&lang.css` virtual style module and can parse raw component
  // source as CSS; global styles live in src/lib/styles/components.css.
  plugins: [tailwindcss(), sveltekit()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : {
          overlay: false,
        },
    watch: {
      // This checkout doubles as the app data root, so the runtime writes
      // accounts.sqlite, WAL/SHM files, logs, caches and agent workspaces into
      // the repository. Vite must not treat those as source changes.
      ignored: [
        "**/src-tauri/**",
        "**/*.sqlite",
        "**/*.sqlite-shm",
        "**/*.sqlite-wal",
        "**/*.sqlite-journal",
        "**/*.db",
        "**/*.db-shm",
        "**/*.db-wal",
        "**/*.db-journal",
        "projects/**",
        "config/**",
        "logs/**",
        "cache/**",
        "keys/**",
        "skills/**",
      ],
    },
  },
  ssr: {
    noExternal: true,
  },
  preview: {
    port: 1420,
  },
  optimizeDeps: {
    exclude: ["@tauri-apps/api"],
  },
});
