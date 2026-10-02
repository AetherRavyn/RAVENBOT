import { launch, HELPERS } from "./cdp.mjs";
import { writeFileSync } from "node:fs";

/* Snapshot every element's geometry plus every scroll container's scrollHeight,
 * so a type-size sweep can be diffed for reflow without anyone looking at it.
 * What must not change: anything that overflows or starts clipping. */

const SNAP = `
const rows = [];
for (const el of [...document.querySelectorAll('*')].filter(__raven.visible)) {
  const r = el.getBoundingClientRect();
  if (r.width < 2 || r.height < 2) continue;
  const s = getComputedStyle(el);
  rows.push({
    sel: __raven.sel(el),
    y: Math.round(r.y), h: Math.round(r.height), w: Math.round(r.width),
    right: Math.round(r.right),
    fontSize: s.fontSize,
    overflowsSelf: s.overflow !== 'visible' && el.scrollHeight > el.clientHeight + 2,
    clipsX: (s.overflow==='hidden'||s.overflowX==='hidden') && el.scrollWidth > el.clientWidth + 2
            && s.textOverflow !== 'ellipsis',
    text: (el.textContent||'').trim().replace(/\\s+/g,' ').slice(0, 40),
  });
}
return {
  rows,
  docScrollH: document.documentElement.scrollHeight,
  docScrollW: document.documentElement.scrollWidth,
  vw: innerWidth, vh: innerHeight,
};
`;

const out = {};
const b = await launch({ port: 9339 });
await b.goto("http://localhost:1420/", 6000);
await b.evaluate(HELPERS);

for (const target of ["Settings", "MCPs & Connectors", "Home"]) {
  await b.key("Escape", { settle: 500 });
  await b.evaluate(`
    const t=[...document.querySelectorAll('nav button')].find(x=>(x.getAttribute('aria-label')||'')===${JSON.stringify(target)});
    if(t) t.click(); return true;`);
  await new Promise((r) => setTimeout(r, 2800));
  out[target] = await b.evaluate(SNAP);
  console.log(
    `${target}: ${out[target].rows.length} elements, doc ${out[target].docScrollW}x${out[target].docScrollH}`,
  );
}

writeFileSync(process.argv[2] || "./ui-audit-geometry.json", JSON.stringify(out));
b.close();