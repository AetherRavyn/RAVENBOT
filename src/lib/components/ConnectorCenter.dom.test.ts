// @vitest-environment jsdom
/**
 * The connector centre, with the backend faked.
 *
 * ## Why this exists
 *
 * The whole connector centre only exists once `list_mcp_servers` returns. Run
 * in a browser there is no Tauri IPC, so the grid renders zero cards and every
 * one of its states — the blocker breakdown, the missing-value line on a card,
 * the "All agents" scope toggle, the card menu's names — is invisible to both a
 * screenshot and a casual look.
 *
 * So the catalog is faked here, from the *real* one: the same ids, the same
 * environment key names, the same mix of keyless local connectors, unverified
 * launchers and multi-requirement ones. A fixture of three invented connectors
 * would have missed the two cases that matter most (a connector wanting a URL
 * *and* a key, and one needing nothing at all).
 */
import { describe, it, expect, afterEach, vi, beforeEach } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";

/* The catalog, keyed by the real ids so the unverified list means something. */
const CATALOG = [
  { id: "redis", name: "Redis MCP", category: "Databases", description: "Vector store and cache.", icon: "🗄", command: "uvx", args: ["redis"], env_keys: ["REDIS_URL"], verified: true, enabled: false, assigned_bot_ids: [] },
  { id: "github", name: "GitHub MCP", category: "Dev & Code", description: "Issues, PRs and repos.", icon: "", command: "npx", args: ["gh-mcp"], env_keys: ["GITHUB_PERSONAL_ACCESS_TOKEN"], verified: true, enabled: false, assigned_bot_ids: [] },
  { id: "tavily", name: "Tavily MCP", category: "Web & Search", description: "Web search API.", icon: "🔍", command: "npx", args: ["tavily"], env_keys: ["TAVILY_API_KEY"], verified: true, enabled: false, assigned_bot_ids: [] },
  // Both an endpoint and a key — the overlap case.
  { id: "elasticsearch", name: "Elasticsearch MCP", category: "Databases", description: "Search cluster.", icon: "", command: "npx", args: ["es"], env_keys: ["ELASTICSEARCH_URL", "ELASTICSEARCH_API_KEY"], verified: true, enabled: false, assigned_bot_ids: [] },
  // Keyless and local: must never appear as a blocker.
  { id: "filesystem", name: "Filesystem MCP", category: "Local System", description: "Local files.", icon: "", command: "npx", args: ["fs"], env_keys: [], verified: true, enabled: false, assigned_bot_ids: [] },
  // Unverified launcher, with nothing missing.
  { id: "obscure", name: "Obscure MCP", category: "Business", description: "Unaudited package.", icon: "", command: "npx", args: ["obscure"], env_keys: [], verified: false, enabled: false, assigned_bot_ids: [] },
];

const calls: string[] = [];

/**
 * A connection test the test controls the timing of.
 *
 * `test_mcp_server` has to be able to hang: the point of the "testing" state is
 * that it exists *while* the request is in flight, and a mock that resolves in
 * the next microtask can never be observed mid-flight. So it returns a promise
 * the test settles, and defaults to a failure — which is the state worth
 * checking that the card keeps.
 */
let pendingTest: {
  resolve: (v: unknown) => void;
  reject: (v: unknown) => void;
} | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string, args?: Record<string, unknown>) => {
    calls.push(cmd);
    switch (cmd) {
      case "list_mcp_servers":
        return Promise.resolve(CATALOG.map((c) => ({ ...c, env_configured: false, is_custom: false })));
      case "list_bot_mcp_servers":
        return Promise.resolve([]);
      case "list_all_skills":
        return Promise.resolve([]);
      case "test_mcp_server":
        return new Promise((resolve, reject) => {
          pendingTest = { resolve, reject };
        });
      default:
        return Promise.resolve(null);
    }
  }),
}));

const { default: ConnectorCenter } = await import("./ConnectorCenter.svelte");

afterEach(cleanup);
beforeEach(() => {
  calls.length = 0;
});

async function renderCenter() {
  const r = render(ConnectorCenter, { props: { bots: [] } });
  await waitFor(() => expect(r.container.textContent).toContain("Filesystem MCP"));
  return r;
}

const cards = (c: HTMLElement) => [...c.querySelectorAll("h4")].map((h) => h.textContent);

describe("ConnectorCenter — filter hierarchy", () => {
  it("separates status filters from category filters with a label each", async () => {
    const { container } = await renderCenter();
    const text = container.textContent ?? "";
    // Two different questions, and they read as two groups rather than as one
    // list of fourteen chips.
    expect(text).toContain("Status");
    expect(text).toContain("Category");
    // The taxonomy group is still there and still complete — the reviewer was
    // explicit that it must not be removed.
    for (const cat of ["Dev & Code", "Databases", "Web & Search"]) {
      expect(text, `category ${cat} missing`).toContain(cat);
    }
  });
});

describe("ConnectorCenter — the blocker breakdown", () => {
  /**
   * The main opportunity: "112 need something" became "here is what, in four
   * groups you can actually work through".
   */
  it("breaks the blockers down by what is missing", async () => {
    const { container, getByText } = await renderCenter();
    await fireEvent.click(getByText(/Needs configuration/));

    const text = container.textContent ?? "";
    expect(text).toContain("Missing endpoint");
    expect(text).toContain("Missing API key");
    expect(text).toContain("Missing token or OAuth");
    expect(text).toContain("Unverified launcher");

    // Redis and Elasticsearch both need an endpoint.
    const endpointRow = getByText("Missing endpoint").closest("button")!;
    expect(endpointRow.textContent).toContain("2");
    // Tavily *and* Elasticsearch — which is the overlap, and the reason the
    // panel carries the note about counts not summing.
    expect(getByText("Missing API key").closest("button")!.textContent).toContain("2");
    // GitHub only.
    expect(getByText("Missing token or OAuth").closest("button")!.textContent).toContain("1");
    // Obscure only.
    expect(getByText("Unverified launcher").closest("button")!.textContent).toContain("1");
  });

  /**
   * The overlap has to be stated. Elasticsearch appears under both "endpoint"
   * and "API key", so the four counts do not sum to the tab total, and a user
   * who adds them up should not conclude something is broken.
   */
  it("says the counts overlap rather than implying a total", async () => {
    const { container, getByText } = await renderCenter();
    await fireEvent.click(getByText(/Needs configuration/));
    expect(container.textContent).toContain("more than one");
  });

  it("narrows the grid to the selected blocker", async () => {
    const { container, getByText } = await renderCenter();
    await fireEvent.click(getByText(/Needs configuration/));
    expect(cards(container)).toEqual(
      expect.arrayContaining(["Redis MCP", "GitHub MCP", "Tavily MCP"]),
    );

    await fireEvent.click(getByText("Missing endpoint"));
    expect(cards(container).sort()).toEqual(["Elasticsearch MCP", "Redis MCP"]);

    await fireEvent.click(getByText("Missing endpoint"));
    expect(cards(container).length).toBe(4);
  });

  /** The 23 local connectors are keyless by design, not "missing configuration". */
  it("never lists a keyless local connector as a blocker", async () => {
    const { container, getByText } = await renderCenter();
    await fireEvent.click(getByText(/Needs configuration/));
    await fireEvent.click(getByText("Missing endpoint"));
    expect(cards(container)).not.toContain("Filesystem MCP");
    await fireEvent.click(getByText("Missing endpoint"));
    expect(cards(container)).not.toContain("Filesystem MCP");
  });

  /**
   * An unverified launcher with nothing missing has no `env_keys`, so a filter
   * written as "env_keys.length > 0" would make it unreachable — the one
   * connector in this catalog whose status the user most needs to see.
   */
  it("can reach an unverified connector that needs no credentials", async () => {
    const { container, getByText } = await renderCenter();
    await fireEvent.click(getByText(/Needs configuration/));
    await fireEvent.click(getByText("Unverified launcher"));
    expect(cards(container)).toEqual(["Obscure MCP"]);
  });
});

describe("ConnectorCenter — card status", () => {
  it("names the value that is missing, and offers the action", async () => {
    const { container } = await renderCenter();
    const text = container.textContent ?? "";
    expect(text).toContain("Missing REDIS_URL");
    expect(text).toContain("Missing GITHUB_PERSONAL_ACCESS_TOKEN");
    expect(text).toContain("Configure");
  });

  /**
   * "Unverified" on its own is a dead end — the user cannot tell whether the
   * connector is broken or merely unaudited. Making the line offer the test is
   * what turns it from a warning into something they can resolve.
   */
  it("offers a way to resolve an unverified connector", async () => {
    const { container } = await renderCenter();
    const status = [...container.querySelectorAll("button")].find((b) =>
      (b.textContent ?? "").includes("Unverified connector"),
    )!;
    expect(status).toBeTruthy();
    expect(status.getAttribute("title")).toContain("catalog audit");
    expect(status.textContent).toContain("Test to find out");
  });

  it("does not describe a keyless connector as missing anything", async () => {
    const { container } = await renderCenter();
    expect(container.textContent).toContain("Ready to assign");
  });
});

describe("ConnectorCenter — card chrome", () => {
  /**
   * "Global" was ambiguous — enabled, visible, configured, scope. "All agents"
   * says which.
   */
  it("labels the scope toggle for what it actually does", async () => {
    const { container } = await renderCenter();
    const text = container.textContent ?? "";
    expect(text).toContain("All agents");
    expect(text).not.toMatch(/>\s*Global\s*</);
  });

  it("gives every card menu a name that says which card it belongs to", async () => {
    const { container } = await renderCenter();
    const menus = [...container.querySelectorAll('[aria-haspopup="true"]')];
    expect(menus.length).toBe(CATALOG.length);
    for (const m of menus) {
      expect(m.getAttribute("aria-label")).toMatch(/^More actions for .+/);
    }
    expect(menus.some((m) => (m.getAttribute("aria-label") ?? "").includes("Redis MCP"))).toBe(true);
  });
});

describe("ConnectorCenter — empty state", () => {
  it("still renders the shell with no connectors at all", async () => {
    const { container } = render(ConnectorCenter, { props: { bots: [] } });
    await waitFor(() => expect(calls).toContain("list_mcp_servers"));
    expect(container.textContent).toContain("Connectors & Tools");
  });
});
describe("ConnectorCenter — state design", () => {
  /**
   * The card used to forget that a connector had failed.
   *
   * The test result lived only in the modal, so a connector shown as failed went
   * back to reading "Ready to assign" the moment you dismissed it. A card that
   * forgets the most expensive thing it ever learned is the one bug in this list
   * that would have shipped silently, because nothing throws.
   */
  async function openTestOnFirstCard() {
    const { container, getByText } = await renderCenter();
    const menu = container.querySelector('[aria-haspopup="true"]') as HTMLElement;
    await fireEvent.click(menu);
    fireEvent.click(getByText("Test connection"));
    return { container, getByText };
  }

  it("keeps showing a failed connection after the modal is dismissed", async () => {
    const { container } = await openTestOnFirstCard();
    // While it is in flight the card says so, rather than sitting there
    // looking ready — which is what a spinner-less await looks like.
    await waitFor(() => expect(container.textContent).toContain("Testing connection"));

    pendingTest?.resolve({ success: false, message: "connection refused" });
    await waitFor(() => expect(container.textContent).toContain("Connection failed"));
    // Dismiss the modal. The card must not forget.
    await fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(container.textContent).toContain("Connection failed"),
    );
  });

  it("reports a successful test as no longer failing", async () => {
    const { container } = await openTestOnFirstCard();
    await waitFor(() => expect(container.textContent).toContain("Testing connection"));
    pendingTest?.resolve({ success: true, message: "ok", latency_ms: 12, tools: [] });
    await waitFor(() =>
      expect(container.textContent ?? "").not.toContain("Connection failed"),
    );
  });
});
