// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * The browser recorder and replay helper are plain JavaScript assets shipped by
 * Rust with `include_str!`. These tests execute the same bytes in jsdom, so a
 * fix here is a fix in the page rather than a second implementation.
 */

const recorderSource = readFileSync(
  join(process.cwd(), "crates/skills/assets/demo-recorder.js"),
  "utf8",
);
const replaySource = readFileSync(
  join(process.cwd(), "crates/skills/assets/demo-replay.js"),
  "utf8",
);

type Recorder = {
  start: (options?: Record<string, unknown>) => any;
  status: (after?: number) => any;
  stop: (options?: Record<string, unknown>) => any;
};

function loadRecorder(): Recorder {
  delete (window as unknown as Record<string, unknown>).__ravenbotDemo;
  new Function(recorderSource)();
  return (window as unknown as { __ravenbotDemo: Recorder }).__ravenbotDemo;
}

function loadReplay(): {
  run: (action: Record<string, unknown>) => any;
} {
  delete (window as unknown as Record<string, unknown>).__ravenbotReplay;
  new Function(replaySource)();
  return (
    window as unknown as {
      __ravenbotReplay: { run: (action: Record<string, unknown>) => any };
    }
  ).__ravenbotReplay;
}

function click(element: Element) {
  element.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
}

function input(element: Element, value: string) {
  (element as HTMLInputElement).value = value;
  element.dispatchEvent(new Event("input", { bubbles: true }));
}

function change(element: Element, value: string) {
  (element as HTMLSelectElement).value = value;
  element.dispatchEvent(new Event("change", { bubbles: true }));
}

describe("demo-recorder.js", () => {
  let recorder: Recorder;

  beforeEach(() => {
    document.body.innerHTML = "";
    recorder = loadRecorder();
  });

  afterEach(() => {
    try {
      recorder.stop({ clear: true });
    } catch {
      /* no active recorder */
    }
    document.body.innerHTML = "";
  });

  it("records a click, its selector fallbacks, and the start navigation", () => {
    document.body.innerHTML =
      '<button data-testid="search" id="go" name="search">Search</button>';
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    click(document.querySelector("[data-testid=search]")!);
    const status = recorder.status(0);
    const kinds = status.events.map((event: any) => event.kind);
    expect(kinds).toEqual(["navigate", "click"]);
    expect(status.events[0].url).toBe(window.location.href);
    expect(status.events[1].selector).toBe('[data-testid="search"]');
    expect(status.events[1].fallbacks).toContain("#go");
    expect(status.events[1].label).toBe("Search");
  });

  it("coalesces a typing burst into one fill with the final value", () => {
    document.body.innerHTML = '<input id="query" name="q" aria-label="Query" />';
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    const field = document.querySelector("#query")!;
    input(field, "sh");
    input(field, "shoe");
    input(field, "shoes");
    field.dispatchEvent(new Event("change", { bubbles: true }));
    const fills = recorder
      .status(0)
      .events.filter((event: any) => event.kind === "fill");
    expect(fills).toHaveLength(1);
    expect(fills[0].text).toBe("shoes");
    expect(fills[0].selector).toBe("#query");
  });

  it("records a select value as a fill", () => {
    document.body.innerHTML =
      '<select id="size" aria-label="Size"><option value="s">Small</option><option value="l">Large</option></select>';
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    change(document.querySelector("#size")!, "l");
    const fill = recorder
      .status(0)
      .events.find((event: any) => event.kind === "fill");
    expect(fill.text).toBe("l");
    expect(fill.label).toBe("Size");
  });

  it("records only Enter, Escape, and Tab as presses", () => {
    document.body.innerHTML = '<input id="q" name="q" />';
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    const field = document.querySelector("#q")!;
    for (const key of ["Enter", "a", "Escape", "Tab"]) {
      field.dispatchEvent(
        new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
      );
    }
    const presses = recorder
      .status(0)
      .events.filter((event: any) => event.kind === "press")
      .map((event: any) => event.key);
    expect(presses).toEqual(["Enter", "Escape", "Tab"]);
  });

  it("omits password, OTP, and card values while keeping a manual step", () => {
    document.body.innerHTML = `
      <input id="password" name="password" type="password" />
      <input id="otp" name="code" autocomplete="one-time-code" />
      <input id="cc" name="card-number" autocomplete="cc-number" />
      <input id="file" type="file" />
    `;
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    input(document.querySelector("#password")!, "hunter2-super-secret");
    input(document.querySelector("#otp")!, "123456");
    input(document.querySelector("#cc")!, "4111111111111111");
    document.querySelector("#file")!.dispatchEvent(new Event("change", { bubbles: true }));
    const status = recorder.status(0);
    const fills = status.events.filter((event: any) => event.kind === "fill");
    expect(fills).toHaveLength(3);
    for (const fill of fills) {
      expect(fill.sensitive).toBe(true);
      expect(fill.text).toBeNull();
    }
    const serialized = JSON.stringify(status);
    for (const secret of [
      "hunter2-super-secret",
      "123456",
      "4111111111111111",
      "/tmp/secret.pdf",
    ]) {
      expect(serialized).not.toContain(secret);
    }
    expect(status.sensitive_omitted).toBeGreaterThanOrEqual(3);
    expect(status.warnings.join(" ")).toContain("sensitive");
  });

  it("treats a contenteditable inside a password-manager card as sensitive", () => {
    document.body.innerHTML =
      '<div class="1password-card"><div id="note" contenteditable="true"></div></div>';
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    const note = document.querySelector("#note")!;
    note.textContent = "the secret phrase";
    note.dispatchEvent(new Event("input", { bubbles: true }));
    click(note);
    const status = recorder.status(0);
    const fill = status.events.find((event: any) => event.kind === "fill");
    const clickEvent = status.events.find((event: any) => event.kind === "click");
    expect(fill.sensitive).toBe(true);
    expect(fill.text).toBeNull();
    expect(clickEvent.label).toBeNull();
    expect(JSON.stringify(status)).not.toContain("the secret phrase");
  });

  it("records checkbox state without double-counting a label click", () => {
    document.body.innerHTML =
      '<label for="remember">Remember me</label><input id="remember" type="checkbox" />';
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    const label = document.querySelector("label")!;
    const box = document.querySelector("#remember")!;
    click(label);
    click(box);
    const clicks = recorder
      .status(0)
      .events.filter((event: any) => event.kind === "click");
    expect(clicks).toHaveLength(1);
    expect(clicks[0].checked).toBe(true);
  });

  it("returns stop JSON with a clear count and clears on cancel", () => {
    document.body.innerHTML = '<button id="go">Go</button>';
    recorder.start({
      fresh: true,
      start_url: window.location.href,
      started_at: Date.now(),
    });
    click(document.querySelector("#go")!);
    const stopped = recorder.stop({ clear: false });
    expect(stopped.ok).toBe(true);
    expect(stopped.recording).toBe(false);
    expect(stopped.count).toBe(2);
    expect(stopped.events).toHaveLength(2);
    expect(stopped.events[1].kind).toBe("click");

    const cleared = recorder.stop({ clear: true });
    expect(cleared.count).toBe(0);
    expect(cleared.events).toEqual([]);
  });
});

describe("demo-replay.js", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    // jsdom does not implement scrollIntoView; the helper guards it.
    Element.prototype.scrollIntoView = function scrollIntoView() {};
  });

  it("falls back to a label when the selector is gone", () => {
    document.body.innerHTML = '<button aria-label="Search">Go</button>';
    const replay = loadReplay();
    const result = replay.run({
      kind: "click",
      selector: "#missing",
      fallbacks: [],
      label: "Search",
    });
    expect(result.ok).toBe(true);
    expect(result.matched_by).toBe("aria-label");
    expect(result.attempted).toContain("#missing");
  });

  it("uses a recorded fallback selector and reports which one matched", () => {
    document.body.innerHTML = '<button class="primary">Go</button>';
    const replay = loadReplay();
    const result = replay.run({
      kind: "click",
      selector: "#missing",
      fallbacks: [".primary"],
      label: "Go",
    });
    expect(result.ok).toBe(true);
    expect(result.matched_by).toBe("fallback");
    expect(result.selector).toBe(".primary");
  });

  it("returns a failure instead of pretending when nothing matches", () => {
    document.body.innerHTML = "<div>nothing here</div>";
    const replay = loadReplay();
    const result = replay.run({
      kind: "click",
      selector: "#missing",
      fallbacks: [".also-missing"],
      label: "Search",
    });
    expect(result.ok).toBe(false);
    expect(result.error).toContain("No element matched");
    expect(result.attempted).toEqual([
      "#missing",
      ".also-missing",
      "label:Search",
    ]);
  });

  it("refuses to fill a sensitive field", () => {
    document.body.innerHTML = '<input id="password" type="password" />';
    const replay = loadReplay();
    const result = replay.run({
      kind: "fill",
      selector: "#password",
      sensitive: true,
      text: null,
    });
    expect(result.ok).toBe(false);
    expect(result.needs_input).toBe(true);
    expect(result.error).toContain("needs your input");
    expect((document.querySelector("#password") as HTMLInputElement).value).toBe("");
  });

  it("fills an input and selects a select option", () => {
    document.body.innerHTML = `
      <input id="query" />
      <select id="size"><option value="s">Small</option><option value="l">Large</option></select>
    `;
    const replay = loadReplay();
    const filled = replay.run({
      kind: "fill",
      selector: "#query",
      text: "shoes",
    });
    expect(filled.ok).toBe(true);
    expect((document.querySelector("#query") as HTMLInputElement).value).toBe("shoes");

    const selected = replay.run({
      kind: "fill",
      selector: "#size",
      text: "l",
    });
    expect(selected.ok).toBe(true);
    expect((document.querySelector("#size") as HTMLSelectElement).value).toBe("l");

    const missing = replay.run({
      kind: "fill",
      selector: "#size",
      text: "x-large",
    });
    expect(missing.ok).toBe(false);
    expect(missing.error).toContain("no longer offers");
  });

  it("dispatches a press and reports that it is synthetic", () => {
    document.body.innerHTML = '<input id="q" />';
    const replay = loadReplay();
    const result = replay.run({
      kind: "press",
      selector: "#q",
      key: "Enter",
    });
    expect(result.ok).toBe(true);
    expect(result.synthetic).toBe(true);
    expect(result.submitted_form).toBe(false);
  });
});
