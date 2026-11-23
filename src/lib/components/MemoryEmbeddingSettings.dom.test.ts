// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import MemoryEmbeddingSettings from "./MemoryEmbeddingSettings.svelte";
import { setLocale } from "$lib/i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

function localConfig() {
  return {
    provider: "local",
    model: "",
    base_url: "",
    has_api_key: false,
    agent_memories: 2,
    office_memories: 3,
  };
}

beforeEach(() => {
  cleanup();
  setLocale("en");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string, args: any) => {
    if (command === "get_memory_embedding_config") return localConfig();
    if (command === "set_memory_embedding_config") {
      return {
        provider: args.provider,
        model:
          args.model ??
          (args.provider === "ollama"
            ? "nomic-embed-text"
            : "text-embedding-3-small"),
        base_url:
          args.baseUrl ??
          (args.provider === "ollama"
            ? "http://127.0.0.1:11434"
            : "https://api.openai.com/v1"),
        has_api_key: args.provider === "openai",
        agent_memories: 2,
        office_memories: 3,
      };
    }
    if (command === "reindex_memory_embeddings") {
      return { agent: 2, office: 3 };
    }
    if (command === "evaluate_memory_retrieval") {
      return {
        k: 5,
        dimension: 384,
        recall_at_k: 0.8,
        mean_reciprocal_rank: 0.72,
        queries: [
          {
            query: "When does the release train leave?",
            expected: "f01",
            rank: 1,
            top: ["f01"],
          },
          {
            query: "Which cryptographic key signs invoices?",
            expected: "f02",
            rank: null,
            top: ["f09"],
          },
        ],
      };
    }
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(cleanup);

describe("MemoryEmbeddingSettings", () => {
  it("saves an Ollama backend and reindexes on demand", async () => {
    const { getByLabelText, getByText } = render(MemoryEmbeddingSettings);
    await waitFor(() =>
      expect(getByText("2 agent facts · 3 office memories")).toBeTruthy(),
    );
    expect(invokeMock).toHaveBeenCalledWith("get_memory_embedding_config");

    await fireEvent.click(getByText("Ollama"));
    await fireEvent.input(getByLabelText("Embedding model"), {
      target: { value: "nomic-embed-text" },
    });
    await fireEvent.click(getByText("Save and reindex"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_memory_embedding_config", {
        provider: "ollama",
        model: "nomic-embed-text",
        baseUrl: null,
        apiKey: null,
      }),
    );
    await waitFor(() =>
      expect(
        getByText("Embeddings updated and 5 memories reindexed."),
      ).toBeTruthy(),
    );
    expect(
      (getByLabelText("Endpoint URL") as HTMLInputElement).value,
    ).toBe("http://127.0.0.1:11434");

    await fireEvent.click(getByText("Reindex now"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("reindex_memory_embeddings"),
    );
    await waitFor(() =>
      expect(getByText("Reindexed 5 memories.")).toBeTruthy(),
    );
  });

  it("runs the fixed retrieval benchmark and reports recall and misses", async () => {
    const { getByText } = render(MemoryEmbeddingSettings);
    await waitFor(() =>
      expect(getByText("2 agent facts · 3 office memories")).toBeTruthy(),
    );

    await fireEvent.click(getByText("Evaluate retrieval"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("evaluate_memory_retrieval"),
    );
    await waitFor(() => expect(getByText("Recall@5")).toBeTruthy());
    expect(getByText("80%")).toBeTruthy();
    expect(getByText("0.72")).toBeTruthy();
    expect(getByText("384-dimension vectors")).toBeTruthy();
    expect(getByText("rank 1")).toBeTruthy();
    expect(getByText("missed")).toBeTruthy();
    expect(
      getByText(
        "Runs the fixed 50-fact, 10-query benchmark. Only the benchmark text is sent to the configured endpoint; your memories are not.",
      ),
    ).toBeTruthy();
  });

  it("collects a write-only key for an OpenAI-compatible endpoint", async () => {
    const { getByLabelText, getByText } = render(MemoryEmbeddingSettings);
    await waitFor(() =>
      expect(getByText("2 agent facts · 3 office memories")).toBeTruthy(),
    );

    await fireEvent.click(getByText("OpenAI-compatible"));
    await fireEvent.input(getByLabelText("Embedding model"), {
      target: { value: "text-embedding-3-small" },
    });
    await fireEvent.input(getByLabelText("API key"), {
      target: { value: "sk-test" },
    });
    await fireEvent.click(getByText("Save and reindex"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_memory_embedding_config", {
        provider: "openai",
        model: "text-embedding-3-small",
        baseUrl: null,
        apiKey: "sk-test",
      }),
    );
    await waitFor(() =>
      expect(getByText("A key is saved.")).toBeTruthy(),
    );
    expect(
      getByText("Memory text will be sent to https://api.openai.com/v1 for embedding."),
    ).toBeTruthy();
  });
});
