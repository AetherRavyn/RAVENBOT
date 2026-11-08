// @vitest-environment jsdom
/**
 * The Projects viewer and editor, in a DOM.
 *
 * The properties worth pinning are the ones a plain `<pre>` silently lacked:
 * a Markdown file opens rendered rather than as raw text, a code file opens
 * with its line numbers, and editing is not a one-way door — Ctrl+S sends the
 * version the editor opened, a conflict is visible instead of overwriting an
 * agent's newer write, and leaving with unsaved edits asks first.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("$lib/toast", () => ({ notify: vi.fn() }));
vi.mock("$lib/workspace.svelte", () => ({ workspace: { bots: [] } }));

import ProjectsView from "./ProjectsView.svelte";

const ROOT = "/projects/acme";

const entry = (over: Record<string, unknown>) => ({
  path: String(over.path),
  name: String(over.path).split("/").pop(),
  isDir: false,
  isLink: false,
  sizeBytes: 24,
  preview: null,
  isBinary: false,
  ...over,
});

const tree = {
  root: ROOT,
  entries: [
    entry({ path: "README.md", sizeBytes: 24 }),
    entry({
      path: "main.rs",
      sizeBytes: 32,
      isBinary: false,
    }),
  ],
  missing: false,
  truncated: false,
  note: null,
  fileCount: 2,
  dirCount: 0,
  totalBytes: 56,
};

const files: Record<string, { text: string; sha256: string }> = {
  [`${ROOT}/README.md`]: { text: "# Acme\n\nHello **world**.\n", sha256: "hash-readme" },
  [`${ROOT}/main.rs`]: {
    text: 'fn main() {\n    println!("hi");\n}\n',
    sha256: "hash-main",
  },
};

function serve(overrides: Record<string, unknown> = {}) {
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd in overrides) {
      const value = overrides[cmd];
      return typeof value === "function" ? (value as (...a: unknown[]) => unknown)(args) : value;
    }
    switch (cmd) {
      case "list_projects":
        return [{ id: "p1", name: "Acme", kind: "office", paths: [ROOT] }];
      case "file_change_totals":
        return [];
      case "list_recent_file_changes":
        return [];
      case "browse_workspace":
        return tree;
      case "read_workspace_file": {
        const file = files[String(args.path)];
        if (!file) throw new Error(`no such file: ${args.path}`);
        return {
          text: file.text,
          sizeBytes: file.text.length,
          truncated: false,
          sha256: file.sha256,
          modifiedMs: 1_700_000_000_000,
        };
      }
      case "write_workspace_file":
        return {
          status: "saved",
          sha256: "hash-saved",
          sizeBytes: String(args.content).length,
          modifiedMs: 1_700_000_000_001,
        };
      default:
        return null;
    }
  });
}

beforeEach(() => {
  invoke.mockReset();
  serve();
});

afterEach(cleanup);

async function openProjects() {
  const rendered = render(ProjectsView);
  await waitFor(() => expect(rendered.getByText("README.md")).toBeTruthy());
  return rendered;
}

async function openFile(getByRole: ReturnType<typeof render>["getByRole"], name: string) {
  const row = getByRole("button", { name: new RegExp(name.replace(".", "\\.")) }) as HTMLElement;
  await fireEvent.click(row);
}

describe("ProjectsView viewer", () => {
  it("renders a Markdown file as prose, not source", async () => {
    const { container, getByRole } = await openProjects();
    await openFile(getByRole, "README.md");

    await waitFor(() => {
      expect(container.querySelector(".markdown-content h1")?.textContent).toBe("Acme");
    });
    // The source is still one click away.
    await fireEvent.click(getByRole("button", { name: /Source/ }));
    await waitFor(() => {
      expect(container.querySelector(".proj__pre")?.textContent).toContain("# Acme");
    });
  });

  it("shows code with a line-number gutter", async () => {
    const { container, getByRole } = await openProjects();
    await openFile(getByRole, "main.rs");

    await waitFor(() => {
      expect(container.querySelector(".proj__code")?.textContent).toContain('println!("hi")');
    });
    expect(container.querySelector(".proj__gutter")?.textContent).toBe("1\n2\n3\n4");
  });

  it("highlights the source it shows", async () => {
    const { container, getByRole } = await openProjects();
    await openFile(getByRole, "main.rs");
    await waitFor(() => expect(container.querySelector(".tok-keyword")).toBeTruthy());
    expect(container.querySelector(".tok-keyword")?.textContent).toBe("fn");
  });
});

describe("ProjectsView editor", () => {
  it("saves with the hash it opened, so an agent's newer write is not lost", async () => {
    let written: Record<string, unknown> | null = null;
    serve({
      write_workspace_file: (args: Record<string, unknown>) => {
        written = args;
        return {
          status: "saved",
          sha256: "hash-saved",
          sizeBytes: String(args.content).length,
          modifiedMs: 1_700_000_000_001,
        };
      },
    });

    const { getByRole, getByText } = await openProjects();
    await openFile(getByRole, "main.rs");
    await waitFor(() => expect(getByRole("button", { name: "Edit" })).toBeTruthy());
    await fireEvent.click(getByRole("button", { name: "Edit" }));

    const editor = await waitFor(() => getByRole("textbox", { name: "File editor" }) as HTMLTextAreaElement);
    await fireEvent.input(editor, { target: { value: "fn main() {}\n" } });
    await fireEvent.keyDown(editor, { key: "s", ctrlKey: true });

    await waitFor(() => expect(written).not.toBeNull());
    expect(written).toMatchObject({
      path: `${ROOT}/main.rs`,
      content: "fn main() {}\n",
      expectedSha256: "hash-main",
      force: false,
    });
    await waitFor(() => expect(getByText("Saved")).toBeTruthy());
  });

  it("shows a conflict and only overwrites after the explicit choice", async () => {
    let calls = 0;
    serve({
      write_workspace_file: (args: Record<string, unknown>) => {
        calls += 1;
        if (calls === 1) {
          return {
            status: "conflict",
            sha256: "hash-agent",
            text: "fn main() { agent(); }\n",
            sizeBytes: 25,
            message: "The file changed on disk since you opened it.",
          };
        }
        return { status: "saved", sha256: "hash-mine", sizeBytes: 12 };
      },
    });

    const { getByRole, getByText } = await openProjects();
    await openFile(getByRole, "main.rs");
    await fireEvent.click(await waitFor(() => getByRole("button", { name: "Edit" })));

    const editor = await waitFor(() => getByRole("textbox", { name: "File editor" }) as HTMLTextAreaElement);
    await fireEvent.input(editor, { target: { value: "fn main() { mine(); }\n" } });
    await fireEvent.keyDown(editor, { key: "s", ctrlKey: true });

    await waitFor(() => expect(getByText("The file changed on disk")).toBeTruthy());
    expect(calls).toBe(1);

    await fireEvent.click(getByRole("button", { name: /Overwrite with mine/ }));
    await waitFor(() => expect(calls).toBe(2));
    const overwrite = invoke.mock.calls.filter(([cmd]) => cmd === "write_workspace_file").at(-1)?.[1];
    expect(overwrite).toMatchObject({ force: true, content: "fn main() { mine(); }\n" });
  });

  it("asks before throwing away unsaved edits", async () => {
    const { container, getByRole, getByText } = await openProjects();
    await openFile(getByRole, "main.rs");
    await fireEvent.click(await waitFor(() => getByRole("button", { name: "Edit" })));
    const editor = await waitFor(() => getByRole("textbox", { name: "File editor" }) as HTMLTextAreaElement);
    await fireEvent.input(editor, { target: { value: "fn main() { changed(); }\n" } });

    await openFile(getByRole, "README.md");

    await waitFor(() => expect(getByText("Unsaved changes")).toBeTruthy());
    await fireEvent.click(getByRole("button", { name: "Discard changes" }));

    // The requested file opened, and the unsaved editor is gone.
    await waitFor(() => {
      expect(container.querySelector(".markdown-content h1")?.textContent).toBe("Acme");
    });
    expect(container.querySelector(".proj__editor")).toBeNull();
  });
});
