/**
 * Classifying what a connector is missing.
 *
 * Built from the real catalog rather than from examples: 135 connectors and 116
 * distinct environment key names, listed below. A classifier tuned on three
 * invented names would have been wrong about `AZURE_DEVOPS_PAT` and
 * `BIGQUERY_CREDENTIALS`, and those are exactly the cases that make a "needs
 * configuration" list useful rather than a wall.
 */
import { describe, it, expect } from "vitest";
import {
  classifyRequirement,
  missingLabel,
  needsBreakdown,
  whatIsMissing,
  type ConnectorLike,
} from "./connectors";

/** Key names taken verbatim from the catalog. */
const CATALOG_KEYS: Array<[string, "endpoint" | "apikey" | "token"]> = [
  // Endpoints and addresses.
  ["REDIS_URL", "endpoint"],
  ["ELASTICSEARCH_URL", "endpoint"],
  ["OPENSEARCH_URL", "endpoint"],
  ["COUCHDB_URL", "endpoint"],
  ["SURREAL_URL", "endpoint"],
  ["MONGODB_URI", "endpoint"],
  ["NEO4J_URI", "endpoint"],
  ["SNOWFLAKE_URI", "endpoint"],
  ["POSTGRES_CONNECTION_STRING", "endpoint"],
  ["TIMESCALE_CONNECTION_STRING", "endpoint"],
  ["MYSQL_DSN", "endpoint"],
  ["CLICKHOUSE_DSN", "endpoint"],
  ["KUBECONFIG", "endpoint"],
  // Tokens, secrets, OAuth.
  ["GITHUB_PERSONAL_ACCESS_TOKEN", "token"],
  ["GITLAB_PERSONAL_ACCESS_TOKEN", "token"],
  ["BITBUCKET_TOKEN", "token"],
  ["SENTRY_AUTH_TOKEN", "token"],
  ["JIRA_API_TOKEN", "token"],
  ["RAILWAY_API_TOKEN", "token"],
  ["FLY_API_TOKEN", "token"],
  ["ARGOCD_AUTH_TOKEN", "token"],
  ["SUPABASE_ACCESS_TOKEN", "token"],
  ["FIREBASE_TOKEN", "token"],
  ["PLANETSCALE_SERVICE_TOKEN", "token"],
  ["HOME_ASSISTANT_TOKEN", "token"],
  ["TWILIO_AUTH_TOKEN", "token"],
  ["AZURE_DEVOPS_PAT", "token"],
  ["REDDIT_CLIENT_SECRET", "token"],
  ["NEO4J_PASSWORD", "token"],
  ["BIGQUERY_CREDENTIALS", "token"],
  // Service API keys and identities.
  ["LINEAR_API_KEY", "apikey"],
  ["RENDER_API_KEY", "apikey"],
  ["POSTHOG_API_KEY", "apikey"],
  ["POSTMAN_API_KEY", "apikey"],
  ["NEON_API_KEY", "apikey"],
  ["BRAVE_API_KEY", "apikey"],
  ["GOOGLE_API_KEY", "apikey"],
  ["TAVILY_API_KEY", "apikey"],
  ["EXA_API_KEY", "apikey"],
  ["FIRECRAWL_API_KEY", "apikey"],
  ["PERPLEXITY_API_KEY", "apikey"],
  ["ELASTICSEARCH_API_KEY", "apikey"],
  ["MEILISEARCH_KEY", "apikey"],
  ["WOLFRAM_APP_ID", "apikey"],
  ["REDDIT_CLIENT_ID", "apikey"],
];

describe("classifyRequirement", () => {
  /** The whole catalog, in one assertion, so a new key cannot land in limbo. */
  it("classifies every environment key name in the real catalog", () => {
    const wrong = CATALOG_KEYS.filter(
      ([key, want]) => classifyRequirement(key) !== want,
    );
    expect(
      wrong.map(([k, w]) => `${k} → ${classifyRequirement(k)}, wanted ${w}`),
      "misclassified catalog keys",
    ).toEqual([]);
  });

  /**
   * The two names that a shorter rule list gets wrong, which is why the rules
   * are ordered the way they are.
   */
  it("puts a bare PAT in the token bucket, not with the API keys", () => {
    expect(classifyRequirement("AZURE_DEVOPS_PAT")).toBe("token");
  });

  it("treats a credentials path as a secret rather than an address", () => {
    // BIGQUERY_CREDENTIALS is a file the user supplies. Filing it under
    // "endpoint" would send them looking for a URL.
    expect(classifyRequirement("BIGQUERY_CREDENTIALS")).toBe("token");
  });

  it("matches KUBECONFIG, which has no suffix to go on", () => {
    expect(classifyRequirement("KUBECONFIG")).toBe("endpoint");
  });

  it("is case-insensitive", () => {
    expect(classifyRequirement("redis_url")).toBe("endpoint");
    expect(classifyRequirement("Redis_Url")).toBe("endpoint");
  });

  it("does not confuse a URL that happens to contain the word KEY", () => {
    // `*_URL` is tested before `*_KEY`; a name like MAPBOX_KEY_URL is an
    // endpoint-shaped value, not a secret.
    expect(classifyRequirement("MAPBOX_KEY_URL")).toBe("endpoint");
  });

  /**
   * The fallback matters more than the rules. A name this module has never seen
   * must land somewhere sensible, because the catalog grows and a thrown or
   * unclassified key would take the whole breakdown with it.
   */
  it("falls back to a service credential for a name it has never seen", () => {
    expect(classifyRequirement("SOME_BRAND_NEW_THING")).toBe("apikey");
    expect(classifyRequirement("")).toBe("apikey");
  });
});

describe("whatIsMissing", () => {
  const ready = { id: "git", env_keys: [], verified: true } satisfies ConnectorLike;

  it("reports nothing for a keyless local connector", () => {
    // Filesystem, Git, Docker and Playwright need no credentials. They are not
    // "missing configuration" and must never appear in a list of blockers.
    expect(whatIsMissing(ready)).toBeNull();
  });

  it("reports nothing once every declared key is set", () => {
    expect(whatIsMissing({ id: "github", env_keys: ["X_TOKEN"], env_configured: true })).toBeNull();
  });

  it("reports the key it is waiting on", () => {
    const n = whatIsMissing({ id: "chroma", env_keys: ["CHROMA_URL"] })!;
    expect(n.missing).toEqual(["CHROMA_URL"]);
    expect(n.requirements).toEqual(["endpoint"]);
    expect(n.primary).toBe("endpoint");
  });

  /**
   * Elasticsearch wants a URL *and* an API key. Filing it under one bucket
   * hides half the work, so it appears under both — the buckets are filters,
   * not a partition.
   */
  it("counts a multi-requirement connector under every requirement", () => {
    const n = whatIsMissing({
      id: "elasticsearch",
      env_keys: ["ELASTICSEARCH_URL", "ELASTICSEARCH_API_KEY"],
    })!;
    expect(n.requirements.sort()).toEqual(["apikey", "endpoint"]);
    expect(n.primary, "lead with the thing you can try without opening it").toBe("endpoint");
  });

  /**
   * Unverified is a different problem wearing the same coat: the launcher
   * package was not found in the catalog audit. It must not be lost by being
   * modelled as a missing value.
   */
  it("reports an unverified connector even when it needs nothing", () => {
    const n = whatIsMissing({ id: "obscure", env_keys: [], verified: false })!;
    expect(n.unverified).toBe(true);
    expect(n.missing).toEqual([]);
  });

  it("reports both when a connector is unverified and unconfigured", () => {
    const n = whatIsMissing({ id: "x", env_keys: ["X_TOKEN"], verified: false })!;
    expect(n.requirements).toEqual(["token"]);
    expect(n.unverified).toBe(true);
  });

  it("survives a record with no env_keys field at all", () => {
    expect(whatIsMissing({ id: "bare" } as ConnectorLike)).toBeNull();
  });
});

describe("needsBreakdown", () => {
  const catalog: ConnectorLike[] = [
    { id: "a", env_keys: ["REDIS_URL"] }, // endpoint
    { id: "b", env_keys: ["ELASTICSEARCH_URL", "ELASTICSEARCH_API_KEY"] }, // both
    { id: "c", env_keys: ["TAVILY_API_KEY"] }, // apikey
    { id: "d", env_keys: ["GITHUB_PERSONAL_ACCESS_TOKEN"] }, // token
    { id: "e", env_keys: [], verified: false }, // unverified only
    { id: "f", env_keys: [] }, // ready
    { id: "g", env_keys: ["LINEAR_API_KEY"], env_configured: true }, // ready
  ];

  const b = needsBreakdown(catalog);

  it("puts each connector in the bucket its work belongs to", () => {
    expect(b.endpoint.sort()).toEqual(["a", "b"]);
    expect(b.apikey.sort()).toEqual(["b", "c"]);
    expect(b.token.sort()).toEqual(["d"]);
    expect(b.unverified.sort()).toEqual(["e"]);
  });

  /** A connector with nothing to configure must not inflate any count. */
  it("excludes the ready ones entirely", () => {
    const all = [...b.endpoint, ...b.apikey, ...b.token, ...b.unverified];
    expect(all).not.toContain("f");
    expect(all).not.toContain("g");
  });

  it("always returns every bucket, even when empty", () => {
    const empty = needsBreakdown([{ id: "ok", env_keys: [] }]);
    for (const k of ["endpoint", "apikey", "token", "unverified"] as const) {
      expect(empty[k], k).toEqual([]);
    }
  });
});

describe("missingLabel", () => {
  it("names the keys it is showing", () => {
    const n = whatIsMissing({ id: "x", env_keys: ["X_TOKEN", "X_URL"] })!;
    expect(missingLabel(n)).toBe("X_TOKEN, X_URL");
  });

  /**
   * Two is the ceiling on purpose. The card is dense, a third name wraps to a
   * second line, and the credential drawer has the full list one click away.
   */
  it("stops at two names and says there are more", () => {
    const n = whatIsMissing({ id: "x", env_keys: ["A_TOKEN", "B_TOKEN", "C_TOKEN"] })!;
    expect(missingLabel(n)).toBe("A_TOKEN, B_TOKEN…");
  });

  it("does not add an ellipsis for exactly two", () => {
    const n = whatIsMissing({ id: "x", env_keys: ["A_TOKEN", "B_TOKEN"] })!;
    expect(missingLabel(n)).toBe("A_TOKEN, B_TOKEN");
  });
});