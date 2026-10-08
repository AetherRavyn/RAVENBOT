import { launch, HELPERS } from "./cdp.mjs";

const AUDIT = `
const findings = [];
const add = (kind, severity, el, detail) => {
  const r = el.getBoundingClientRect();
  findings.push({ kind, severity, sel: __raven.sel(el),
    label: (el.getAttribute('aria-label') || el.textContent || '').trim().replace(/\\s+/g,' ').slice(0,56),
    at: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)], detail });
};
const visible = [...document.querySelectorAll('*')].filter(__raven.visible);

const textNodes = visible.filter(el => {
  const s = getComputedStyle(el);
  if (!el.textContent || !el.textContent.trim()) return false;
  if (s.webkitTextFillColor === 'transparent') return false;
  return [...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim());
});

for (const el of textNodes) {
  const s = getComputedStyle(el);
  const fg = __raven.rgb(s.color); if (!fg) continue;
  const bg = __raven.bgOf(el);
  const size = parseFloat(s.fontSize), weight = parseInt(s.fontWeight,10)||400;
  const need = (size >= 24 || (size >= 18.66 && weight >= 700)) ? 3 : 4.5;
  const ratio = Math.round(__raven.contrast(fg, bg) * 100) / 100;
  if (ratio < need) add('contrast', ratio < need-1 ? 'high':'medium', el,
    { ratio, need, size: s.fontSize, fg: s.color, bg:'rgb('+bg.r+','+bg.g+','+bg.b+')' });
}

for (const el of document.querySelectorAll(__raven.interactive)) {
  if (!__raven.visible(el)) continue;
  if (el.disabled === true || el.getAttribute('aria-disabled') === 'true') continue;
  const r = el.getBoundingClientRect();
  const inlineText = el.matches('a') && (el.textContent||'').trim().length > 1;
  if (!inlineText && (r.width < 24 || r.height < 24))
    add('hit-target', r.width < 18 || r.height < 18 ? 'high':'medium', el,
      { w: Math.round(r.width), h: Math.round(r.height) });
}

for (const el of visible) {
  const r = el.getBoundingClientRect();
  if (r.width && r.right > document.documentElement.clientWidth + 1.5)
    add('overflow-x','high', el, { over: Math.round(r.right - document.documentElement.clientWidth) });
  const s = getComputedStyle(el);
  if ((s.overflow==='hidden'||s.overflowX==='hidden') && el.scrollWidth > el.clientWidth+2
      && el.textContent.trim() && s.textOverflow!=='ellipsis' && s.overflowX!=='auto' && s.overflowX!=='scroll')
    add('clipped-text','medium', el, { scrollW: el.scrollWidth, clientW: el.clientWidth, text: el.textContent.trim().slice(0,44) });
}

for (const el of document.querySelectorAll('button, a[href], [role="button"]')) {
  if (!__raven.visible(el)) continue;
  if (!(el.getAttribute('aria-label')||el.textContent||'').trim())
    add('no-name','high', el, { tag: el.tagName.toLowerCase(), cls: String(el.className||'').slice(0,90), html: el.innerHTML.replace(/\\s+/g,' ').slice(0,60) });
}

const sizes = {};
for (const el of textNodes) { const s = getComputedStyle(el); sizes[s.fontSize] = (sizes[s.fontSize]||0)+1; }
return { findings, sizes };
`;

const b = await launch({ port: 9336 });
await b.goto("http://localhost:1420/", 6000);
await b.evaluate(HELPERS);

async function closeOverlays() {
  await b.key("Escape", { settle: 600 });
  await b.evaluate(`
    // Anything still open on top of the shell is a panel that will be measured
    // instead of the surface underneath it.
    const modal = document.querySelector('[role="dialog"]');
    if (modal) {
      const close = [...modal.querySelectorAll('button')].find(b => /close|dismiss/i.test(b.getAttribute('aria-label')||''));
      if (close) close.click();
    }
    return true;`);
  await new Promise((r) => setTimeout(r, 900));
  await b.key("Escape", { settle: 600 });
}

async function goRail(label) {
  await closeOverlays();
  const ok = await b.evaluate(`
    const t = [...document.querySelectorAll('nav button')]
      .find(x => (x.getAttribute('aria-label')||'') === ${JSON.stringify(label)});
    if (!t) return false; t.click(); return true;`);
  await new Promise((r) => setTimeout(r, 3000));
  return ok;
}

for (const target of ["MCPs & Connectors", "Settings", "Home"]) {
  const ok = await goRail(target);
  const heading = await b.evaluate(`
    const dlg = document.querySelector('[role="dialog"]');
    const main = document.querySelector('main');
    const src = dlg || main;
    return {
      surface: dlg ? 'dialog' : 'main',
      heading: (src.querySelector('h1,h2,h3')?.textContent || '').trim().slice(0,60),
      buttons: src.querySelectorAll('button').length,
      hasFocusTrap: !!document.querySelector('[role="dialog"] [tabindex], [data-focus-trap]'),
    };`);
  console.log(`\n${"#".repeat(70)}\n# ${target}  -> ${heading.surface}: "${heading.heading}" (${heading.buttons} buttons) navigated=${ok}\n${"#".repeat(70)}`);

  const res = await b.evaluate(AUDIT);
  const byKind = {};
  for (const f of res.findings) (byKind[f.kind] ||= []).push(f);
  const order = { high: 0, medium: 1, low: 2 };
  for (const [kind, list] of Object.entries(byKind)) {
    list.sort((a, b) => order[a.severity] - order[b.severity]);
    console.log(`\n-- ${kind}: ${list.length}`);
    const seen = new Set();
    let shown = 0;
    for (const f of list) {
      // Collapse repeats of the same component signature.
      const sig = f.sel.replace(/\\[\\d+\\]/g, "");
      const key = kind + "|" + sig;
      if (seen.has(key)) continue;
      seen.add(key); shown++;
      console.log(`   [${f.severity}] ${f.sel}`);
      console.log(`      "${f.label}" @${f.at.join(",")} ${JSON.stringify(f.detail)}`);
      if (shown >= 10) break;
    }
    if (seen.size < list.length) console.log(`   … ${list.length} findings across ${seen.size} distinct components`);
  }
  if (!Object.keys(byKind).length) console.log("   clean");
  console.log("\n   sizes:", JSON.stringify(res.sizes));
}

b.close();