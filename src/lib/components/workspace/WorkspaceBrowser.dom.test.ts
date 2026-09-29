// @vitest-environment jsdom
/**
 * The workspace browser, in a DOM.
 *
 * This panel exists because the confinement was invisible: a user set a folder,
 * watched agents run, and had no way to tell whether they had produced anything.
 * So the states it has to tell apart are the product, not decoration — and the
 * one that most needs to be right is the distinction between a workspace that
 * does not exist yet and one that is empty. A brand new office is the first, and
 * it is not a fault; showing it as an error would send people looking for a
 * problem that does not exist.
 *
 * The listing is bounded and says so when a bound bites, which is also a
 * product decision: a short tree that does not announce itself reads as "this is
 * everything", and that is the one thing a file browser must never imply.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/svelte";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("$lib/toast", () => ({ notify: vi.fn() }));

import WorkspaceBrowser from "./WorkspaceBrowser.svelte";

const entry = (over: Record<string, unknown> = {}) => ({
  path: "OFFICE.md",
  name: "OFFICE.md",
  isDir: false,
  isLink: false,
  sizeBytes: 120,
  preview: "# Office\n",
  isBinary: false,
  ...over,
});

const tree = (over: Record<string, unknown> = {}) => ({
  root: "/w",
  entries: [],
  missing: false,
  truncated: false,
  note: null,
  fileCount: 0,
  dirCount: 0,
  totalBytes: 0,
  ...over,
});

/** Answer `browse_workspace` with `t`, and nothing else. */
function serve(t: unknown) {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "browse_workspace") return t;
    return null;
  });
}

beforeEach(() => {
  invoke.mockReset();
});

afterEach(cleanup);

describe("WorkspaceBrowser", () => {
  it("asks the backend for the folder it was given", async () => {
    serve(tree());
    render(WorkspaceBrowser, { props: { paths: ["/srv/work"] } });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("browse_workspace", {
        path: "/srv/work",
        showHidden: false,
      }),
    );
  });

  /**
   * The distinction the whole panel turns on. A folder that is not created yet
   * is the normal state of a new office.
   */
  it("distinguishes a workspace that does not exist from an empty one", async () => {
    serve(tree({ missing: true, note: "It is created the first time an agent works here." }));
    const { container, getByText } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(getByText(/not created yet/i)).toBeTruthy());
    expect(container.textContent).toContain("first time an agent works here");
    // An empty workspace is a different sentence, and must not borrow this one.
    expect(container.textContent).not.toMatch(/nothing here yet/i);
  });

  it("says an empty workspace is empty", async () => {
    serve(tree());
    const { getByText } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(getByText(/nothing here yet/i)).toBeTruthy());
  });

  it("offers the hidden-files switch when a workspace looks empty", async () => {
    serve(tree());
    const { getByText } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() =>
      // The hint matters: an agent says it wrote `.env` and the user cannot see it.
      expect(getByText(/hidden files are off/i)).toBeTruthy(),
    );
  });

  it("lists files and folders", async () => {
    serve(
      tree({
        entries: [
          entry({ path: "deliverables", name: "deliverables", isDir: true, preview: null }),
          entry(),
        ],
        fileCount: 1,
        dirCount: 1,
      }),
    );
    const { container, getByText } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(getByText("deliverables")).toBeTruthy());
    expect(getByText("OFFICE.md")).toBeTruthy();
    expect(container.textContent).toContain("1");
  });

  it("shows a file's text when it is selected", async () => {
    serve(tree({ entries: [entry()] }));
    const { container, getByText } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(getByText("OFFICE.md")).toBeTruthy());
    await fireEvent.click(getByText("OFFICE.md"));
    // Rendered through the markdown renderer, so the `#` is syntax and the
    // text is the heading.
    await waitFor(() => expect(container.querySelector("h1")?.textContent).toBe("Office"));
  });

  /**
   * A link is listed but never read, because `fs::read` follows it and a link
   * out of the workspace would hand the UI whatever it points at.
   */
  it("refuses to read a link", async () => {
    serve(
      tree({
        entries: [entry({ path: "link.txt", name: "link.txt", isLink: true, preview: null })],
      }),
    );
    const { container, getByText } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(getByText("link.txt")).toBeTruthy());
    await fireEvent.click(getByText("link.txt"));
    // The link is listed, and its target is never read.
    await waitFor(() => expect(container.textContent).toMatch(/link/i));
    expect(container.querySelector(".raven-wsb__text")).toBeNull();
    expect(container.textContent).not.toContain("secret");
  });

  it("says a truncated listing was truncated", async () => {
    serve(
      tree({
        entries: [entry()],
        truncated: true,
        note: "Showing the first 2000 entries, up to 3 levels deep.",
      }),
    );
    const { container } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() =>
      expect(container.textContent).toContain("Showing the first 2000 entries"),
    );
  });

  it("says so when no folder is set at all", async () => {
    const { container, getByText } = render(WorkspaceBrowser, { props: { paths: [] } });
    await waitFor(() => expect(getByText(/no folder is set/i)).toBeTruthy());
    expect(invoke).not.toHaveBeenCalled();
  });

  it("hands the folder to the OS rather than writing to it", async () => {
    serve(tree({ entries: [entry()] }));
    const { getByText } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(getByText("OFFICE.md")).toBeTruthy());
    await fireEvent.click(getByText("Open"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("open_workspace_in_file_manager", { path: "/w" }),
    );
  });

  it("re-reads the folder when hidden files are switched on", async () => {
    serve(tree());
    const { container } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(container.textContent).toBeTruthy());
    invoke.mockClear();
    await fireEvent.click(container.querySelector('[title*="hidden"]')!);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("browse_workspace", {
        path: "/w",
        showHidden: true,
      }),
    );
  });

  /** A failure to read is worth saying; a silent empty panel is indistinguishable. */
  it("surfaces a failure to read the workspace", async () => {
    invoke.mockImplementation(async () => {
      throw new Error("permission denied");
    });
    const { container } = render(WorkspaceBrowser, { props: { paths: ["/w"] } });
    await waitFor(() => expect(container).toBeTruthy());
    // The panel does not claim the workspace is empty when the read failed.
    expect(container.textContent).not.toMatch(/nothing here yet/i);
  });
});
