import axe from "axe-core";

export interface A11yViolation {
  id: string;
  impact: string | null;
  help: string;
  nodes: string[];
}

/**
 * Run axe in jsdom. Colour contrast is disabled because jsdom has no layout or
 * computed colours; contrast is covered by the theme tokens and manual review.
 */
export async function a11yViolations(container: HTMLElement): Promise<A11yViolation[]> {
  const results = await axe.run(container, {
    rules: {
      "color-contrast": { enabled: false },
    },
  });
  return results.violations.map((violation) => ({
    id: violation.id,
    impact: violation.impact ?? null,
    help: violation.help,
    nodes: violation.nodes.map((node) => node.target.join(" ")),
  }));
}
