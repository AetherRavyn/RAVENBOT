// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import Settings from "$lib/components/Settings.svelte";
import { setLocale } from "$lib/i18n";
import { a11yViolations } from "$lib/test/a11y";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const provider = {
  id: "openai",
  name: "OpenAI",
  icon: "openai",
  description: "GPT models",
  key_placeholder: "sk-...",
  key_url: "https://platform.openai.com/api-keys",
  key_env: "OPENAI_API_KEY",
  supports_discovery: true,
  keyless: false,
  default_model: "gpt-4o-mini",
  fallback_models: [],
};

const serviceStatus = {
  supported: true,
  platform: "systemd (user)",
  installed: true,
  enabled: true,
  running: false,
  linger: true,
  path: "/home/tester/.config/systemd/user/ravenbot.service",
  detail: "inactive",
};

let currentAccountMock = {
  username: "owner",
  workspace: "owner",
  role: "owner",
  workspace_owner: true,
};

function mockSettings() {
  invokeMock.mockImplementation(async (command: string) => {
    switch (command) {
      case "get_model_catalog":
        return [provider];
      case "get_configured_providers":
        return ["openai"];
      case "fetch_all_provider_models":
        return [];
      case "list_custom_providers":
        return [];
      case "get_ollama_url":
        return "http://localhost:11434";
      case "get_autostart_enabled":
        return false;
      case "get_allow_private_tool_network":
        return false;
      case "get_service_status":
        return serviceStatus;
      case "get_remote_status":
        return {
          enabled: true,
          running: true,
          url: "http://127.0.0.1:8790",
          port: 8790,
          allow_lan: false,
          devices: [
            {
              id: "device-1",
              name: "Phone",
              role: "operator",
              created_at: "2026-11-18 10:00:00",
              last_seen_at: "2026-11-18 10:05:00",
              revoked_at: null,
            },
          ],
          audit: [
            {
              id: "audit-1",
              device_name: "Phone",
              role: "operator",
              method: "POST",
              path: "/api/threads/abc/messages",
              status: 202,
              created_at: "2026-11-18 10:05:00",
            },
          ],
        };
      case "get_telegram_status":
        return {
          enabled: true,
          token_present: true,
          default_bot: "bot-1",
          links: [
            {
              chat_id: "42",
              chat_title: "Phone chat",
              bot_id: "bot-1",
              thread_id: "thread-1",
              created_at: "2026-11-18 10:00:00",
              last_seen_at: "2026-11-18 10:05:00",
            },
          ],
        };
      case "get_slack_status":
        return {
          enabled: true,
          app_token_present: true,
          bot_token_present: true,
          default_bot: "bot-1",
          links: [
            {
              channel_id: "C123",
              chat_title: "Slack #support",
              bot_id: "bot-1",
              thread_id: "thread-2",
              created_at: "2026-11-18 10:00:00",
              last_seen_at: "2026-11-18 10:05:00",
            },
          ],
        };
      case "get_discord_status":
        return {
          enabled: true,
          token_present: true,
          default_bot: "bot-1",
          links: [
            {
              channel_id: "D123",
              guild_id: "G1",
              chat_title: "Discord #ops",
              bot_id: "bot-1",
              thread_id: "thread-3",
              created_at: "2026-11-18 10:00:00",
              last_seen_at: "2026-11-18 10:05:00",
            },
          ],
        };
      case "get_whatsapp_status":
        return {
          enabled: true,
          token_present: true,
          app_secret_present: true,
          verify_token_present: true,
          phone_number_id: "1234567890",
          default_bot: "bot-1",
          local_webhook_url: "http://127.0.0.1:8801/whatsapp",
          links: [
            {
              wa_id: "15551234567",
              display_name: "Ada",
              bot_id: "bot-1",
              thread_id: "thread-4",
              created_at: "2026-11-18 10:00:00",
              last_seen_at: "2026-11-18 10:05:00",
            },
          ],
        };
      case "get_current_account":
        return currentAccountMock;
      case "list_workspace_grants":
        return [["bob", "operator"]];
      case "get_teams_status":
        return {
          enabled: true,
          app_id: "11111111-1111-1111-1111-111111111111",
          app_password_present: true,
          tenants: ["22222222-2222-2222-2222-222222222222"],
          default_bot: "bot-1",
          local_webhook_url: "http://127.0.0.1:8801/teams",
          links: [
            {
              conversation_id: "conv-1",
              tenant_id: "22222222-2222-2222-2222-222222222222",
              conversation_type: "personal",
              service_url: "https://smba.example",
              chat_title: "Teams: Ada",
              bot_id: "bot-1",
              thread_id: "thread-5",
              created_at: "2026-11-18 10:00:00",
              last_seen_at: "2026-11-18 10:05:00",
            },
          ],
        };
      case "get_tool_prices":
        return {};
      case "get_default_model":
        return { provider: "ollama", model: "llama3.1" };
      case "get_default_agent":
        return null;
      default:
        return null;
    }
  });
}

describe("Settings accessibility", () => {
  beforeEach(() => {
    cleanup();
    setLocale("en");
    invokeMock.mockReset();
    currentAccountMock = {
      username: "owner",
      workspace: "owner",
      role: "owner",
      workspace_owner: true,
    };
    mockSettings();
  });

  afterEach(cleanup);

  it("renders the provider key section without axe violations", async () => {
    render(Settings, {
      props: {
        open: true,
        onClose: vi.fn(),
        bots: [],
        initialSection: "providers",
      },
    });

    await waitFor(() => expect(document.body.textContent).toContain("OpenAI"));
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("installs the service and offers the handoff", async () => {
    let status = {
      ...serviceStatus,
      installed: false,
      enabled: false,
      running: false,
      detail: "not installed",
    };
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_model_catalog") return [provider];
      if (command === "get_configured_providers") return ["openai"];
      if (command === "get_service_status") return status;
      if (command === "install_background_service") {
        status = { ...serviceStatus, installed: true, enabled: true, running: false };
        return status;
      }
      if (command === "get_ollama_url") return "http://localhost:11434";
      if (command === "get_autostart_enabled") return true;
      if (command === "get_allow_private_tool_network") return false;
      if (command === "get_tool_prices") return {};
      if (command === "get_default_model") return { provider: "ollama", model: "llama3.1" };
      if (command === "get_default_agent" || command === "list_custom_providers") return null;
      return null;
    });

    render(Settings, {
      props: { open: true, onClose: vi.fn(), bots: [], initialSection: "local" },
    });

    await waitFor(() =>
      expect(document.body.textContent).toContain("Install service"),
    );
    const install = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("Install service"),
    );
    await fireEvent.click(install!);
    await waitFor(() =>
      expect(document.body.textContent).toContain("Restart in background"),
    );
    expect(invokeMock).toHaveBeenCalledWith("install_background_service");
  });

  it("generates a one-time pairing code from the remote section", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_model_catalog") return [provider];
      if (command === "get_configured_providers") return ["openai"];
      if (command === "get_remote_status") {
        return {
          enabled: true,
          running: true,
          url: "http://127.0.0.1:8790",
          port: 8790,
          allow_lan: false,
          devices: [],
          audit: [],
        };
      }
      if (command === "create_remote_pairing") {
        return { code: "123456", role: "viewer", expires_at: "2026-11-18T10:10:00Z" };
      }
      if (command === "get_ollama_url") return "http://localhost:11434";
      if (command === "get_autostart_enabled") return false;
      if (command === "get_service_status") return serviceStatus;
      if (command === "get_allow_private_tool_network") return false;
      if (command === "get_tool_prices") return {};
      if (command === "get_default_model") return { provider: "ollama", model: "llama3.1" };
      if (command === "get_default_agent" || command === "list_custom_providers") return null;
      return null;
    });

    render(Settings, {
      props: { open: true, onClose: vi.fn(), bots: [], initialSection: "remote" },
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Generate code"),
    );
    const generate = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("Generate code"),
    );
    await fireEvent.click(generate!);
    await waitFor(() => expect(document.body.textContent).toContain("123456"));
    expect(invokeMock).toHaveBeenCalledWith("create_remote_pairing", {
      role: "viewer",
    });
  });

  it("shows the self-signed certificate fingerprint without axe violations", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_model_catalog") return [provider];
      if (command === "get_configured_providers") return ["openai"];
      if (command === "get_remote_status") {
        return {
          enabled: true,
          running: true,
          url: "https://127.0.0.1:8790",
          port: 8790,
          allow_lan: false,
          tls: true,
          fingerprint: "a".repeat(64),
          devices: [],
          audit: [],
        };
      }
      if (command === "get_ollama_url") return "http://localhost:11434";
      if (command === "get_autostart_enabled") return false;
      if (command === "get_service_status") return serviceStatus;
      if (command === "get_allow_private_tool_network") return false;
      if (command === "get_tool_prices") return {};
      if (command === "get_default_model") return { provider: "ollama", model: "llama3.1" };
      if (command === "get_default_agent" || command === "list_custom_providers") return null;
      return null;
    });

    render(Settings, {
      props: { open: true, onClose: vi.fn(), bots: [], initialSection: "remote" },
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Certificate fingerprint"),
    );
    expect(document.body.textContent).toContain("a".repeat(64));
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("renders the native Telegram connector without axe violations", async () => {
    render(Settings, {
      props: {
        open: true,
        onClose: vi.fn(),
        bots: [{ id: "bot-1", name: "Billing Agent" }],
        initialSection: "chatapps",
      },
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Phone chat"),
    );
    await waitFor(() =>
      expect(document.body.textContent).toContain("Slack #support"),
    );
    await waitFor(() =>
      expect(document.body.textContent).toContain("Discord #ops"),
    );
    await waitFor(() => expect(document.body.textContent).toContain("Ada"));
    await waitFor(() =>
      expect(document.body.textContent).toContain("Teams: Ada"),
    );
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("renders the signed-in account without axe violations", async () => {
    render(Settings, {
      props: {
        open: true,
        onClose: vi.fn(),
        bots: [{ id: "bot-1", name: "Billing Agent" }],
        initialSection: "profile",
      },
    });
    await waitFor(() => expect(document.body.textContent).toContain("owner"));
    expect(document.body.textContent).toContain("Log out");
    await waitFor(() =>
      expect(document.body.textContent).toContain("Workspace access"),
    );
    expect(document.body.textContent).toContain("bob");
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("limits a viewer to profile and account controls", async () => {
    currentAccountMock = {
      username: "ada",
      workspace: "owner",
      role: "viewer",
      workspace_owner: true,
    };
    render(Settings, {
      props: {
        open: true,
        onClose: vi.fn(),
        bots: [{ id: "bot-1", name: "Billing Agent" }],
        initialSection: "profile",
      },
    });
    await waitFor(() =>
      expect(document.body.textContent).toContain("Read-only viewer"),
    );
    expect(document.body.textContent).not.toContain("Model Manager");
    expect(document.body.textContent).not.toContain("Providers");
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("renders remote access without axe violations", async () => {
    render(Settings, {
      props: {
        open: true,
        onClose: vi.fn(),
        bots: [],
        initialSection: "remote",
      },
    });

    await waitFor(() =>
      expect(document.body.textContent).toContain("Paired devices"),
    );
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });

  it("renders the local and background-service section without axe violations", async () => {
    render(Settings, {
      props: {
        open: true,
        onClose: vi.fn(),
        bots: [],
        initialSection: "local",
      },
    });

    await waitFor(() =>
      expect(document.body.textContent).toContain("Background service"),
    );
    const violations = await a11yViolations(document.body);
    expect(violations, JSON.stringify(violations)).toEqual([]);
  });
});
