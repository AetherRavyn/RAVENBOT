// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import PluginsStore from "$lib/components/PluginsStore.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const stripeUrl =
  "https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json";
const gmailUrl = "https://api.apis.guru/v2/specs/gmail.test/1.0/openapi.json";

const stripeEntry = {
  id: "stripe",
  name: "Stripe",
  description: "Payments, invoices, and billing.",
  spec_url: stripeUrl,
  category: "finance",
  categories: ["finance", "payment"],
  provider: "stripe.com",
  logo: "",
  version: "latest",
  openapi_version: "3.0",
  requires_auth: true,
  featured: true,
  source: "curated",
};

const gmailEntry = {
  id: "apisguru:gmail.test",
  name: "Gmail API",
  description: "Read and send mail.",
  spec_url: gmailUrl,
  category: "email",
  categories: ["email"],
  provider: "gmail.test",
  logo: "",
  version: "1.0",
  openapi_version: "3.0.0",
  requires_auth: null,
  featured: true,
  source: "apisguru",
};

const catalogPage = {
  entries: [stripeEntry, gmailEntry],
  total: 2,
  catalog_total: 2530,
  categories: [
    { id: "email", label: "Email", count: 1 },
    { id: "finance", label: "Finance", count: 1 },
  ],
  source: "live",
  fetched_at: "2026-11-05T01:00:00Z",
  stale: false,
  warning: null,
  has_more: false,
};

const installedStripe = {
  id: "stripe",
  name: "Stripe",
  description: "Payments, invoices, and billing.",
  logo: "",
  enabled_global: false,
  operations: 12,
  has_credential: false,
  requires_auth: true,
  source_url: stripeUrl,
  base_url_override: null,
  credential_header_hint: "Authorization",
};

const importedGmail = {
  summary: {
    id: "gmail-api",
    name: "Gmail API",
    description: "Read and send mail.",
    logo: "",
    enabled_global: false,
    operations: 5,
    has_credential: false,
    requires_auth: false,
    source_url: gmailUrl,
    base_url_override: null,
    credential_header_hint: null,
  },
  operations: [],
  credential_header: null,
};

function installMock() {
  invokeMock.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_plugins":
        return [installedStripe];
      case "list_global_plugins":
        return [];
      case "list_bot_plugins":
        return [];
      case "search_plugin_catalog":
        return catalogPage;
      case "install_catalog_plugin":
        return importedGmail;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
}

function cardFor(name: string): Element | undefined {
  return Array.from(document.querySelectorAll(".plugin-catalog__card")).find((card) =>
    card.textContent?.includes(name),
  );
}

describe("PluginsStore catalog", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    installMock();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders the live directory and marks installed apps", async () => {
    render(PluginsStore, {
      props: { bot: { id: "bot-1", name: "Billing" }, open: true, onClose: () => {} },
    });

    await waitFor(() => expect(cardFor("Gmail API")).toBeTruthy());
    expect(cardFor("Stripe")).toBeTruthy();
    expect(document.body.textContent).toContain("OpenAPI directory");
    expect(document.body.textContent).toContain("2,530");
    expect(invokeMock).toHaveBeenCalledWith("search_plugin_catalog", {
      query: null,
      category: null,
      limit: 48,
      offset: 0,
      refresh: false,
    });

    const stripeButton = cardFor("Stripe")?.querySelector(
      ".plugin-catalog__install",
    ) as HTMLButtonElement;
    expect(stripeButton.disabled).toBe(true);
    expect(stripeButton.textContent).toContain("Installed");

    const gmailButton = cardFor("Gmail API")?.querySelector(
      ".plugin-catalog__install",
    ) as HTMLButtonElement;
    expect(gmailButton.disabled).toBe(false);
  });

  it("searches and filters the directory", async () => {
    render(PluginsStore, {
      props: { bot: { id: "bot-1", name: "Billing" }, open: true, onClose: () => {} },
    });
    await waitFor(() => expect(cardFor("Gmail API")).toBeTruthy());

    const search = document.querySelector(
      'input[aria-label^="Search 2,500"]',
    ) as HTMLInputElement;
    await fireEvent.input(search, { target: { value: "gmail" } });
    await waitFor(
      () =>
        expect(invokeMock).toHaveBeenCalledWith(
          "search_plugin_catalog",
          expect.objectContaining({ query: "gmail" }),
        ),
      { timeout: 2_000 },
    );

    await waitFor(() =>
      expect(document.querySelectorAll(".plugin-catalog__select option").length).toBe(3),
    );
    const category = document.querySelector(
      ".plugin-catalog__select select",
    ) as HTMLSelectElement;
    await fireEvent.change(category, { target: { value: "email" } });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_plugin_catalog",
        expect.objectContaining({ query: "gmail", category: "email" }),
      ),
    );
  });

  it("installs a directory entry by catalog id and spec URL", async () => {
    render(PluginsStore, {
      props: { bot: { id: "bot-1", name: "Billing" }, open: true, onClose: () => {} },
    });
    await waitFor(() => expect(cardFor("Gmail API")).toBeTruthy());

    const install = cardFor("Gmail API")?.querySelector(
      ".plugin-catalog__install",
    ) as HTMLButtonElement;
    await fireEvent.click(install);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("install_catalog_plugin", {
        catalogId: "apisguru:gmail.test",
        specUrl: gmailUrl,
      }),
    );
    await waitFor(() =>
      expect(document.body.textContent).toContain("Imported Gmail API"),
    );
  });

  it("refreshes the directory with an explicit refresh flag", async () => {
    render(PluginsStore, {
      props: { bot: { id: "bot-1", name: "Billing" }, open: true, onClose: () => {} },
    });
    await waitFor(() => expect(cardFor("Gmail API")).toBeTruthy());

    await fireEvent.click(document.querySelector(".plugin-catalog__refresh")!);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_plugin_catalog",
        expect.objectContaining({ refresh: true }),
      ),
    );
  });
});
