/**
 * What a connector is missing, and why.
 *
 * ## Why this is not a key/value map
 *
 * The connector centre's filter used to be called "Needs Keys" and counted one
 * thing: `env_keys.length > 0 && !env_configured`. That is a true statement
 * about 112 connectors and a useless one to act on, because *what* is missing
 * is the whole difference between a list you can work through and a wall.
 * `GITHUB_PERSONAL_ACCESS_TOKEN` and `ELASTICSEARCH_URL` are both "a key", and
 * the work you do about them is not remotely the same.
 *
 * So the missing thing is classified by the shape of its name. That is a
 * heuristic and it is the right one here, for three reasons: the catalog is
 * 135 connectors and 116 distinct names, so a hand-written map would be a
 * larger artefact than the catalog and would fall behind it; the names are
 * machine-generated and follow strong conventions (`*_URL`, `*_TOKEN`,
 * `*_API_KEY`); and the classification only ever *reorders* a list the user is
 * already scanning, so a misclassification costs a little time and never a
 * wrong action.
 *
 * The rule is ordered most-specific first, and it is deliberately wrong in one
 * place: `*_ACCESS_TOKEN` and `*_PERSONAL_ACCESS_TOKEN` match on `ACCESS`
 * before `TOKEN`, which is harmless, but `AZURE_DEVOPS_PAT` matches on `PAT`
 * and is a token. Both land in the token bucket, which is the right bucket.
 */

/**
 * The four reasons a connector might be blocking you.
 *
 * `unverified` is not a missing value — it is a separate condition, flagged by
 * the catalog audit when a connector's upstream launcher package could not be
 * found. It is here because from the user's side it is one of the reasons a
 * connector does not work, and lumping it in with "no keys" would have been the
 * kind of tidy that hides a different problem behind a familiar label.
 */
export type Requirement = "endpoint" | "apikey" | "token" | "unverified";

/**
 * Name fragments, most specific first.
 *
 * Order is load-bearing: `URL` must be tested before `KEY` would matter, and
 * `CREDENTIALS` before either, because `BIGQUERY_CREDENTIALS` is a file path
 * the user has to supply rather than a secret.
 */
const RULES: ReadonlyArray<{ kind: Exclude<Requirement, "unverified">; matches: RegExp }> = [
  // An address the connector has to be pointed at.
  { kind: "endpoint", matches: /(URL|URI|DSN|CONNECTION_STRING|ENDPOINT|_HOST$|^KUBECONFIG)/ },
  // A credential the user has to mint or authorise. Anything OAuth-shaped is
  // a token even when the word token is absent.
  //
  // Deliberately *no* `KEY` here: a service API key is not a token, and the
  // first version of this rule matched `_KEY$` and filed all 13 of the
  // catalog's API-key connectors under tokens — which would have swapped the
  // two largest buckets in the breakdown and made the list actively misleading.
  {
    kind: "token",
    matches: /(TOKEN|OAUTH|SECRET|PASSWORD|CREDENTIALS|_PAT$|COOKIE|CERT)/,
  },
  // A service credential. The fallback for anything left over.
  { kind: "apikey", matches: /(KEY|APP_ID|CLIENT_ID|ACCOUNT_ID|ORG_ID|PROJECT_ID|_ID$)/ },
];

/**
 * Classify one environment key name.
 *
 * Unrecognised names fall through to `apikey`, which is the right default: it is
 * the bucket a user tries first, and a key that turns out to be something else
 * is corrected the moment they open the credential drawer and read the label.
 */
export function classifyRequirement(key: string): Exclude<Requirement, "unverified"> {
  const k = key.toUpperCase();
  for (const rule of RULES) {
    if (rule.matches.test(k)) return rule.kind;
  }
  return "apikey";
}

/** The minimum a connector needs before it can be assigned. */
export interface ConnectorNeeds {
  id: string;
  /** Every key name it is waiting on, in catalog order. */
  missing: string[];
  /**
   * The bucket to file it under.
   *
   * A connector can need several things at once — Elasticsearch wants a URL and
   * an API key, Home Assistant wants a token and a URL — and those are counted
   * against *each* bucket rather than one, because each bucket is a filter and
   * a filter may overlap. It is `primary` only so the card has something to
   * lead with.
   */
  requirements: Exclude<Requirement, "unverified">[];
  primary: Exclude<Requirement, "unverified">;
  unverified: boolean;
}

/** The shape this module needs; narrower than the component's own interface. */
export interface ConnectorLike {
  id: string;
  env_keys: string[];
  env_configured?: boolean;
  verified?: boolean;
}

/**
 * What one connector is waiting on, or `null` if nothing.
 *
 * `null` means it is ready: either it needs nothing at all (the 23 local
 * connectors — Filesystem, Git, Docker, Playwright — which are keyless by
 * design and are *not* "missing configuration"), or every key it declared is
 * already set.
 */
export function whatIsMissing(c: ConnectorLike): ConnectorNeeds | null {
  const missing = (c.env_keys ?? []).filter((k) => !c.env_configured);
  const unverified = c.verified === false;
  if (missing.length === 0 && !unverified) return null;
  const requirements = [...new Set(missing.map(classifyRequirement))];
  return {
    id: c.id,
    missing,
    requirements,
    // The endpoint is listed first when present because "point it somewhere" is
    // the first thing you try and the other two are not distinguishable without
    // opening it.
    primary: requirements.includes("endpoint") ? "endpoint" : requirements[0] ?? "token",
    unverified,
  };
}

/** Every blocker in the catalog, grouped by what it is waiting on. */
export function needsBreakdown(
  connectors: readonly ConnectorLike[],
): Record<Requirement, string[]> {
  const out: Record<Requirement, string[]> = {
    endpoint: [],
    apikey: [],
    token: [],
    unverified: [],
  };
  for (const c of connectors) {
    const n = whatIsMissing(c);
    if (!n) continue;
    for (const r of n.requirements) out[r].push(c.id);
    if (n.unverified) out.unverified.push(c.id);
  }
  return out;
}

/**
 * The shortest honest description of what a connector needs.
 *
 * Two names is the ceiling on purpose: the card is dense, a third wraps, and the
 * credential drawer has the full list one click away.
 */
export function missingLabel(needs: ConnectorNeeds, max = 2): string {
  return needs.missing.slice(0, max).join(", ") + (needs.missing.length > max ? "…" : "");
}