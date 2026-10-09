import { afterAll } from "vitest";

/**
 * Drain timers that would otherwise outlive the environment.
 *
 * bits-ui's body-scroll-lock restores `document.body` on a 24 ms timer
 * (`actualDelay = delay === null ? 24 : delay`, with no component overriding
 * the default) once a dialog unmounts. `afterEach(cleanup)` unmounts it, so a
 * file's last test routinely leaves that timer pending. vitest then disposes
 * the jsdom environment, the timer fires into a world with no `document`, and
 * the resulting ReferenceError is reported as an *unhandled error* — which
 * fails the whole run even though every assertion in it passed. That is
 * exactly how CI went red on a suite reporting "403 passed":
 *
 *     ReferenceError: document is not defined
 *     ❯ Proxy.resetBodyStyle node_modules/bits-ui/dist/internal/
 *       body-scroll-lock.svelte.js:34:9
 *
 * `afterAll` is the only moment where both halves are still true: the file's
 * tests are finished, and the environment has not been torn down yet. Waiting
 * outlasts the 24 ms timer by enough margin that a starved event loop still
 * runs it first — it was scheduled before this one, with an earlier expiry.
 *
 * The `document` guard keeps the pure-logic files — the lexer, the URL guard,
 * the avatar geometry — which run in the node environment and have nothing to
 * drain, from paying for a wait they cannot spend.
 */
afterAll(async () => {
  if (typeof document === "undefined") return;
  await new Promise((resolve) => setTimeout(resolve, 60));
});
