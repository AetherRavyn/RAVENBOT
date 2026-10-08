// Minimal Chrome DevTools Protocol client. No dependencies — Node 24 ships a
// global WebSocket, which is the only thing this needs beyond fetch.

import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

export async function launch({ port = 9333, width = 1440, height = 900 } = {}) {
  const proc = spawn(
    "/sbin/chromium",
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--hide-scrollbars",
      "--remote-debugging-port=" + port,
      "--window-size=" + width + "," + height,
      "about:blank",
    ],
    { stdio: "ignore" },
  );

  let info = null;
  for (let i = 0; i < 60; i += 1) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/version`);
      info = await r.json();
      break;
    } catch {
      await sleep(250);
    }
  }
  if (!info) throw new Error("chromium did not come up");

  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = targets.find((t) => t.type === "page");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = rej;
  });

  let id = 0;
  const pending = new Map();
  const consoleErrors = [];
  const pageErrors = [];

  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
      return;
    }
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      consoleErrors.push(msg.params.args.map((a) => a.value ?? a.description ?? "").join(" "));
    }
    if (msg.method === "Runtime.exceptionThrown") {
      pageErrors.push(
        msg.params.exceptionDetails.exception?.description ??
          msg.params.exceptionDetails.text,
      );
    }
  };

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const mid = ++id;
      pending.set(mid, { resolve, reject });
      ws.send(JSON.stringify({ id: mid, method, params }));
      setTimeout(() => {
        if (pending.has(mid)) {
          pending.delete(mid);
          reject(new Error("CDP timeout: " + method));
        }
      }, 30000);
    });

  await send("Runtime.enable");
  await send("Page.enable");

  /** Evaluate an expression in the page and return its JSON value. */
  async function evaluate(expression) {
    const r = await send("Runtime.evaluate", {
      expression: `(() => { ${expression} })()`,
      returnByValue: true,
      awaitPromise: true,
    });
    if (r.exceptionDetails) {
      throw new Error(
        r.exceptionDetails.exception?.description ?? r.exceptionDetails.text,
      );
    }
    return r.result.value;
  }

  async function goto(url, settle = 2500) {
    await send("Page.navigate", { url });
    await sleep(settle);
  }

  async function click(selector, settle = 900) {
    const ok = await evaluate(
      `const el = document.querySelector(${JSON.stringify(selector)});
       if (!el) return false;
       el.scrollIntoView({block:'center'});
       const r = el.getBoundingClientRect();
       const t = document.elementFromPoint(r.x + r.width/2, r.y + r.height/2);
       (t && t !== el && el.contains(t) ? t : el).click();
       return true;`,
    );
    await sleep(settle);
    return ok;
  }

  async function key(key, { code, ctrl = false, shift = false, settle = 700 } = {}) {
    const base = { key, code: code ?? key, windowsVirtualKeyCode: keyCode(key) };
    await send("Input.dispatchKeyEvent", {
      type: "keyDown",
      modifiers: (ctrl ? 2 : 0) | (shift ? 8 : 0),
      ...base,
    });
    await send("Input.dispatchKeyEvent", {
      type: "keyUp",
      modifiers: (ctrl ? 2 : 0) | (shift ? 8 : 0),
      ...base,
    });
    await sleep(settle);
  }

  function close() {
    try {
      ws.close();
    } catch {}
    proc.kill("SIGKILL");
  }

  return { send, evaluate, goto, click, key, close, consoleErrors, pageErrors };
}

function keyCode(k) {
  const map = {
    Escape: 27,
    Enter: 13,
    Tab: 9,
    ",": 188,
    "/": 191,
    " ": 32,
    Backspace: 8,
    ArrowDown: 40,
    ArrowUp: 38,
  };
  return map[k] ?? k.toUpperCase().charCodeAt(0);
}

/* ── In-page measurement helpers, injected once per session ─────────────── */

export const HELPERS = `
window.__raven = {
  px(v) { return Math.round(v * 100) / 100; },

  rgb(s) {
    const m = String(s).match(/rgba?\\(([^)]+)\\)/);
    if (!m) return null;
    const p = m[1].split(',').map(x => parseFloat(x));
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 };
  },

  lum(c) {
    const f = (v) => { v /= 255; return v <= 0.03928 ? v/12.92 : Math.pow((v+0.055)/1.055, 2.4); };
    return 0.2126*f(c.r) + 0.7152*f(c.g) + 0.0722*f(c.b);
  },

  contrast(a, b) {
    const l1 = this.lum(a), l2 = this.lum(b);
    return (Math.max(l1,l2)+0.05) / (Math.min(l1,l2)+0.05);
  },

  // Nearest ancestor with a non-transparent background, so a text colour is
  // measured against what is actually behind it and not against the page.
  bgOf(el) {
    let n = el;
    while (n && n !== document.documentElement) {
      const c = this.rgb(getComputedStyle(n).backgroundColor);
      if (c && c.a > 0.5) return c;
      n = n.parentElement;
    }
    return this.rgb(getComputedStyle(document.body).backgroundColor) ?? {r:0,g:0,b:0,a:1};
  },

  visible(el) {
    const s = getComputedStyle(el);
    if (s.display === 'none' || s.visibility === 'hidden' || parseFloat(s.opacity) < 0.05) return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  },

  // The user's real target: everything they can click.
  interactive: 'a[href], button, input, textarea, select, [role="button"], [role="tab"], [role="menuitem"], [role="switch"], [role="option"], [tabindex]:not([tabindex="-1"]), [contenteditable="true"]',

  describe(el) {
    const r = el.getBoundingClientRect();
    const s = getComputedStyle(el);
    return {
      tag: el.tagName.toLowerCase(),
      cls: (el.className && el.className.baseVal !== undefined ? el.className.baseVal : el.className || '').toString().slice(0, 90),
      id: el.id || undefined,
      label: (el.getAttribute('aria-label') || el.textContent || '').trim().replace(/\\s+/g,' ').slice(0, 48),
      x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height),
      font: s.fontSize, weight: s.fontWeight, lh: s.lineHeight,
      color: s.color, bg: s.backgroundColor,
      radius: s.borderRadius, pad: s.padding,
      disabled: el.disabled === true || el.getAttribute('aria-disabled') === 'true',
    };
  },

  sel(el) {
    if (el.id) return '#' + el.id;
    const t = el.tagName.toLowerCase();
    if (el.className && typeof el.className === 'string' && el.className.trim()) {
      return t + '.' + el.className.trim().split(/\\s+/).slice(0,3).join('.');
    }
    return t;
  },
};
return true;
`;