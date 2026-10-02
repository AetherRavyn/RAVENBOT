import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/**
 * The type scale, and the floor under it.
 *
 * OpenBot's scale is 12 / 13 / 14 / 16 / 18 / 24 at a 13px base. This app is
 * denser than that, so the floor here is **11px** rather than 12 — dense enough
 * that no panel needs to grow a scroll container it did not already have, and
 * still legible on a hidpi screen.
 *
 * ## Why this is a test and not a convention
 *
 * The app shipped 312 declarations of `text-[9px]` and `text-[10px]`. Nine is
 * below what any production desktop UI uses; ten is borderline. Individually
 * each one was a reasonable choice — a mono env-var hint, a count badge, a
 * caption under an icon — and together they meant the palette had *three*
 * adjacent sub-12px sizes (9, 10, 11) that nothing coordinated. A user cannot
 * tell that an 11px label is the "real" size and a 9px one is the fallback; they
 * can only see that the type is unsettled.
 *
 * So the sizes are gone rather than discouraged, and this is what keeps them
 * gone. A reviewer's eye on a diff will not catch a `text-[10px]` that someone
 * typed because the line above already had one; this will.
 */

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      walk(full, out);
    } else if (entry.endsWith(".svelte")) {
      out.push(full);
    }
  }
  return out;
}

/**
 * Every size the app is allowed to set text in.
 *
 * 20px is here because it is Tailwind's own named step (`text-xl`) and the app
 * uses it twice. Dropping it from the scale would mean forbidding a class the
 * framework provides and people reach for, which buys very little purity and
 * costs a rule everyone works around.
 */
const SCALE = new Set(["11px", "12px", "13px", "14px", "16px", "18px", "20px", "24px"]);

describe("type scale", () => {
  const files = walk("src");

  it("finds the source tree at all", () => {
    // A silent pass here would make every assertion below vacuous.
    expect(files.length, "no components found — wrong working directory?").toBeGreaterThan(30);
  });

  /**
   * The floor. Checked against the arbitrary Tailwind size syntax rather than
   * against a list of banned strings, so `text-[10px]` and `text-[9px]` are both
   * caught, and so is `text-[10.5px]`.
   */
  it("sets no text below the 11px floor", () => {
    const offenders: string[] = [];
    for (const file of files) {
      const src = readFileSync(file, "utf8");
      const re = /text-\[(\d+(?:\.\d+)?)px\]/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(src))) {
        if (parseFloat(m[1]) < 11) {
          offenders.push(`${file}: text-[${m[1]}px]`);
        }
      }
    }
    expect(
      offenders,
      `Type below the 11px floor — use one of ${[...SCALE].join(", ")}:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  /** And the floor is not the only rule: sizes must come off the scale. */
  it("sets text only at sizes on the scale", () => {
    const offenders: string[] = [];
    for (const file of files) {
      const src = readFileSync(file, "utf8");
      const re = /text-\[(\d+(?:\.\d+)?)px\]/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(src))) {
        if (!SCALE.has(`${parseFloat(m[1])}px`)) {
          offenders.push(`${file}: text-[${m[1]}px]`);
        }
      }
    }
    expect(
      offenders,
      `Off-scale type — the scale is ${[...SCALE].join(", ")}:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  /**
   * The Tailwind *named* sizes are the same scale by another route, and this is
   * what stops `text-xl` (20px) being available while sitting between two
   * declared steps. Every named size the app can use must resolve onto the
   * scale above.
   */
  it("keeps Tailwind's named sizes on the same scale", () => {
    const named: Record<string, string> = {
      "text-xs": "12px",
      "text-sm": "14px",
      "text-base": "16px",
      "text-lg": "18px",
      "text-xl": "20px",
      "text-2xl": "24px",
    };
    const offScale = Object.entries(named)
      .filter(([, size]) => !SCALE.has(size))
      .map(([cls, size]) => `${cls} = ${size}`);
    expect(offScale, `add these to the scale: ${offScale.join(", ")}`).toEqual([]);
  });
});