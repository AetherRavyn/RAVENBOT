import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * This repository is also the app data root. The dev server must ignore
 * SQLite/WAL churn and agent workspaces, or every database write reloads the
 * whole UI.
 */
describe("dev server watch", () => {
  it("ignores runtime databases and app-data directories", () => {
    const config = readFileSync(join(process.cwd(), "vite.config.js"), "utf8");
    for (const pattern of [
      "**/*.sqlite",
      "**/*.sqlite-shm",
      "**/*.sqlite-wal",
      "**/*.db",
      "**/*.db-shm",
      "**/*.db-wal",
      "projects/**",
      "config/**",
      "logs/**",
      "cache/**",
      "keys/**",
      "skills/**",
    ]) {
      expect(config).toContain(pattern);
    }
  });
});
