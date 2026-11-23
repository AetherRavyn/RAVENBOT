// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { COOKIE_REJECT_SCRIPT } from "./browserCookies";

function clickable(name: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.textContent = name;
  return button;
}

function banner(html: string): HTMLDivElement {
  const dialog = document.createElement("div");
  dialog.setAttribute("role", "dialog");
  dialog.innerHTML = html;
  document.body.appendChild(dialog);
  return dialog;
}

function runScript(): { ok: boolean; clicked: boolean; label?: string } {
  return window.eval(COOKIE_REJECT_SCRIPT);
}

beforeEach(() => {
  document.body.innerHTML = "";
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 200,
    height: 40,
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: 200,
    bottom: 40,
    toJSON: () => ({}),
  } as DOMRect);
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("COOKIE_REJECT_SCRIPT", () => {
  it("clicks an exact reject option inside a cookie dialog", () => {
    let clicked = false;
    const dialog = banner(
      "<p>We use cookies to improve this site.</p><button id='reject'>Reject all</button>",
    );
    dialog.querySelector<HTMLButtonElement>("#reject")!.onclick = () => {
      clicked = true;
    };

    const result = runScript();
    expect(result.clicked).toBe(true);
    expect(result.label).toBe("reject all");
    expect(clicked).toBe(true);
  });

  it("never clicks accept buttons", () => {
    let clicked = false;
    const dialog = banner(
      "<p>We use cookies.</p><button id='accept'>Accept all</button>",
    );
    dialog.querySelector<HTMLButtonElement>("#accept")!.onclick = () => {
      clicked = true;
    };

    expect(runScript().clicked).toBe(false);
    expect(clicked).toBe(false);
  });

  it("ignores a bare reject button outside a consent context", () => {
    let clicked = false;
    const form = document.createElement("form");
    form.innerHTML = "<p>Submit your application.</p><button id='reject'>Reject</button>";
    document.body.appendChild(form);
    form.querySelector<HTMLButtonElement>("#reject")!.onclick = () => {
      clicked = true;
    };

    expect(runScript().clicked).toBe(false);
    expect(clicked).toBe(false);
  });

  it("ignores hidden reject buttons", () => {
    let clicked = false;
    const dialog = banner(
      "<p>We use cookies.</p><button id='reject' style='display: none'>Reject</button>",
    );
    dialog.querySelector<HTMLButtonElement>("#reject")!.onclick = () => {
      clicked = true;
    };

    expect(runScript().clicked).toBe(false);
    expect(clicked).toBe(false);
  });

  it("handles localized consent banners", () => {
    let clicked = false;
    const dialog = banner(
      "<p>Wir verwenden Cookies.</p><button id='reject'>Alle ablehnen</button>",
    );
    dialog.querySelector<HTMLButtonElement>("#reject")!.onclick = () => {
      clicked = true;
    };

    expect(runScript().clicked).toBe(true);
    expect(clicked).toBe(true);
  });

  it("reports no banner when there is nothing to click", () => {
    document.body.innerHTML = "<main><button>Save</button></main>";
    const result = runScript();
    expect(result.clicked).toBe(false);
    expect(result.ok).toBe(true);
  });
});
