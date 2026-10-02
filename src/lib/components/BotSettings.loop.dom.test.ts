// @vitest-environment jsdom
/**
 * The agent builder must not lock the event loop.
 *
 * ## The bug
 *
 * "Model selection is not working" — reported with no further detail, because
 * from inside the dialog there is nothing to report. The dialog was not broken;
 * **the process was**. Two `$effect`s read state that they also caused to be
 * written, so each one re-triggered itself, and both loops ran entirely in
 * microtasks:
 *
 *   1. `if (open && allSkills.length === 0) void loadSkills()` — and
 *      `loadSkills` sets `allSkills = rows ?? []`. Any install where
 *      `list_all_skills` returns an empty array (fresh install, no skills
 *      enabled, or the catch path when the IPC call fails) left the guard still
 *      true, forever.
 *
 *   2. The re-hydration effect reset `defaultWorkspace = ""` and then called
 *      `resolveDefaultWorkspace()`, which *reads* `defaultWorkspace`. Setting the
 *      path re-ran the effect, which cleared the path again — and
 *      `get_sandbox_report` / `default_workspace_for` fired twice per lap.
 *
 * A loop made only of microtasks never yields to a timer, so layout, input and
 * paint all stop. From outside there is no crash and no error dialog: the
 * dialog is simply inert, and picking a model is the first thing anyone tries
 * to do in an agent builder.
 *
 * ## The proof
 *
 * Measured in a real browser against this component, before and after:
 *
 *     a plain `setTimeout(fn, 250)`
 *       HEAD          never fired — still pending after 30 seconds
 *       with fix      fired at 250ms
 *
 * ## Why this test is shaped the way it is
 *
 * The loop starves the event loop, which means a naive test cannot simply wait
 * for it to end — the runner's own timeout is a timer, and timers are exactly
 * what never fire. A first attempt at this file capped the repeat count and
 * threw, and it still hung: the loop does not care that a call failed, it just
 * asks again.
 *
 * So once a repeat-counted command has been called absurdly many times, the mock
 * returns a promise that **never settles**. The component's `.then` therefore
 * never runs, the write that re-triggers the effect never happens, and the loop
 * stops on its own — leaving a call count that can be asserted on normally. On
 * the unfixed component this suite *fails*, in about a second. It never hangs,
 * which is the whole reason for the shape.
 */
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup } from "@testing-library/svelte";

/** How many calls of one command we will tolerate before declaring a loop. */
const LOOP_CAP = 25;

const counts: Record<string, number> = {};
/** Set once a repeat-counted command has been called far too many times. */
let tripped = false;

/** A promise that never settles, which is how a runaway loop is stopped. */
const never = () => new Promise<never>(() => {});

/**
 * How many times the runaway loop managed to go round, for the failure text.
 *
 * Reads the busiest of the loop-prone commands rather than all of them, because
 * on the unfixed component they interleave unevenly and the largest count is the
 * one that reads like a number rather than a smudge.
 */
function laps(): number {
  const watched = ["list_all_skills", "get_sandbox_report", "default_workspace_for"]
    .map((k) => counts[k] ?? 0);
  return watched.length ? Math.max(...watched) : 0;
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    counts[cmd] = (counts[cmd] ?? 0) + 1;

    // Stop the loop rather than letting it starve the runner. Rejecting is not
    // enough — the effect catches, writes, and asks again — so the response has
    // to simply never arrive.
    if (
      (cmd === "list_all_skills" || cmd === "get_sandbox_report" || cmd === "default_workspace_for") &&
      counts[cmd] > LOOP_CAP
    ) {
      tripped = true;
      return never();
    }

    switch (cmd) {
      case "get_model_catalog":
        return [
          {
            id: "ollama",
            name: "Ollama",
            icon: "🦙",
            description: "Local",
            keyless: true,
            default_model: "llama3.1:8b",
            supports_tools: true,
            fallback_models: [{ id: "llama3.1:8b", name: "Llama 3.1 8B" }],
          },
        ];
      // The case that started it: an empty skills list.
      case "list_all_skills":
        return [];
      case "fetch_provider_models":
        return [];
      case "get_default_model":
        return { provider: "ollama", model: "llama3.1:8b" };
      default:
        return null;
    }
  }),
}));

const { default: BotSettings } = await import("./BotSettings.svelte");

const BOT = {
  id: "b1",
  name: "Coder",
  config: { model_provider: "ollama", model_id: "llama3.1:8b", temperature: 0.7, max_tokens: 4096 },
};

/** Let the microtask queue drain a bounded number of times. */
const settle = () => new Promise((r) => setTimeout(r, 120));

describe("agent builder — the event loop survives opening the dialog", () => {
  afterEach(() => {
    cleanup();
    for (const k of Object.keys(counts)) delete counts[k];
    tripped = false;
  });

  /**
   * The core regression. `list_all_skills` returning `[]` used to spin forever.
   */
  it("asks for the skills list once, even when it comes back empty", async () => {
    render(BotSettings, {
      props: { bot: BOT, open: true, onClose: () => {}, onUpdated: () => {} },
    });
    await settle();

    expect(
      counts.list_all_skills,
      `list_all_skills was called ${counts.list_all_skills} times — the effect is re-triggering itself`,
    ).toBe(1);
    expect(tripped, "the loop cap was reached — the effect is re-triggering itself").toBe(false);
  });

  /**
   * The second loop. The re-hydration effect cleared the workspace path and then
   * asked for it again, forever, taking the sandbox report with it twice a lap.
   */
  it("resolves the default workspace and the sandbox report once each", async () => {
    render(BotSettings, {
      props: { bot: BOT, open: true, onClose: () => {}, onUpdated: () => {} },
    });
    await settle();

    expect(
      counts.default_workspace_for,
      `the workspace path ping-ponged between empty and resolved — the busiest loop command ran ${laps()} times`,
    ).toBe(1);
    expect(counts.get_sandbox_report, "the sandbox report was refetched in a loop").toBe(1);
    expect(tripped, "the loop cap was reached").toBe(false);
  });

  /**
   * The provider catalog is fetched at most once per open. It was guarded on
   * `catalog.length === 0`, which only terminated because `getCatalog` happens
   * to fall back to a built-in list — it terminated by luck, not by design.
   *
   * The bound is "<= 1" rather than "== 1" because `getCatalog` memoises in a
   * module-level cache: once one dialog has fetched it, a later one legitimately
   * makes no call at all. Asserting an exact count would be asserting the cache
   * rather than the fix.
   */
  it("loads the provider catalog at most once", async () => {
    render(BotSettings, {
      props: { bot: BOT, open: true, onClose: () => {}, onUpdated: () => {} },
    });
    await settle();
    expect(counts.get_model_catalog ?? 0).toBeLessThanOrEqual(1);
    expect(tripped, "the loop cap was reached").toBe(false);
  });

  /**
   * Reopening the dialog is the ordinary case, and it is where a "fetch once"
   * flag has to be either correct or obviously wrong. It is correct: once per
   * open, not once ever.
   */
  it("still refreshes when the dialog is reopened", async () => {
    const props = { bot: BOT, onClose: () => {}, onUpdated: () => {} };
    const first = render(BotSettings, { props: { ...props, open: true } });
    await settle();
    cleanup();

    const before = counts.list_all_skills ?? 0;
    render(BotSettings, { props: { ...props, open: true } });
    await settle();

    expect(counts.list_all_skills).toBe(before + 1);
    expect(tripped, "the loop cap was reached").toBe(false);
    first.unmount?.();
  });

  /**
   * Closed, it must not spin.
   *
   * It does still hydrate from `bot` on mount — the dialog is only ever mounted
   * while open in practice, so that is not worth changing and this does not
   * pretend otherwise. What matters is that nothing *repeats*, which is the
   * whole difference between this and the bug.
   */
  it("does not repeat anything while closed", async () => {
    render(BotSettings, {
      props: { bot: BOT, open: false, onClose: () => {}, onUpdated: () => {} },
    });
    await settle();
    expect(counts.list_all_skills ?? 0, "the skills fetch is gated on open").toBe(0);
    expect(counts.get_sandbox_report ?? 0).toBeLessThanOrEqual(1);
    expect(counts.default_workspace_for ?? 0).toBeLessThanOrEqual(1);
    expect(tripped, "something is looping while the dialog is closed").toBe(false);
  });
});