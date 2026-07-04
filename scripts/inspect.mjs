#!/usr/bin/env node
// inspect.mjs — layout-truth probe. shot.mjs gives pixels; this gives the DOM box
// model + computed styles for a selector and its ancestor chain, so a clip/overflow
// is diagnosable instead of guessable ([[never-blind-feedback-driven-iteration]]).
// Fix the tool weakness the moment you hit it — this closes shot.mjs's blind spot.
//
// Reports, for the matched element up to :host, each ancestor's clientWidth vs
// scrollWidth (scrollWidth > clientWidth = THIS is where content overflows), plus
// min-width / width / overflow-x / white-space. Pierces shadow DOM (the app is nested
// shadow roots). Cross-platform (Node + CDP), same OS-detected Chrome as shot.mjs.
//
// Usage:
//   node scripts/inspect.mjs "<url>" "<css-selector>"
//   SHOT_SIZE=390,844 node scripts/inspect.mjs "http://localhost:5173/?..." ".content"

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { tmpdir, platform } from 'node:os';
import { join } from 'node:path';

const URL = process.argv[2];
const SELECTOR = process.argv[3];
if (!URL || !SELECTOR) {
  console.error('usage: node scripts/inspect.mjs "<url>" "<css-selector>"');
  process.exit(1);
}
const [W, H] = (process.env.SHOT_SIZE || '1600,1000').split(',');
const PORT = parseInt(process.env.INSPECT_PORT || '9333', 10);
const SETTLE = parseInt(process.env.SHOT_BUDGET_MS || '6000', 10);

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const c = {
    darwin: ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'],
    win32: ['C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
            'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
            join(process.env.LOCALAPPDATA || '', 'Google\\Chrome\\Application\\chrome.exe')],
    linux: ['/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium'],
  }[platform()] || [];
  return c.find((p) => p && existsSync(p));
}
const chrome = findChrome();
if (!chrome) { console.error(`inspect: no Chrome for ${platform()} — set CHROME=<path>.`); process.exit(1); }

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The diagnostic runs IN the page: pierce shadow DOM to the selector, then walk the
// ancestor chain (crossing shadow boundaries) reporting box metrics + overflow flags.
const PROBE = `(() => {
  const SEL = ${JSON.stringify(SELECTOR)};
  function deepFind(root) {
    const hit = root.querySelector(SEL); if (hit) return hit;
    for (const el of root.querySelectorAll('*')) {
      if (el.shadowRoot) { const f = deepFind(el.shadowRoot); if (f) return f; }
    }
    return null;
  }
  const el = deepFind(document);
  if (!el) return { error: 'selector not found: ' + SEL };
  const chain = []; let node = el;
  for (let i = 0; i < 10 && node && node.tagName; i++) {
    const cs = getComputedStyle(node);
    chain.push({
      tag: node.tagName.toLowerCase(),
      cls: String(node.className || '').slice(0, 36),
      clientW: node.clientWidth, scrollW: node.scrollWidth,
      overflows: node.scrollWidth > node.clientWidth + 1,
      minWidth: cs.minWidth, width: cs.width,
      overflowX: cs.overflowX, whiteSpace: cs.whiteSpace, wordBreak: cs.wordBreak,
    });
    node = node.parentElement || (node.getRootNode() instanceof ShadowRoot ? node.getRootNode().host : null);
  }
  return { viewport: window.innerWidth, chain };
})()`;

async function cdp() {
  const profile = join(tmpdir(), 'continuum-inspect-' + process.pid);
  const child = spawn(chrome, [
    '--headless=new', '--disable-gpu', '--no-first-run',
    `--user-data-dir=${profile}`, `--window-size=${W},${H}`,
    `--remote-debugging-port=${PORT}`, URL,
  ], { stdio: 'ignore' });
  const cleanup = () => { try { child.kill(); } catch {} };
  try {
    // wait for the devtools endpoint + a page target
    let ws;
    for (let i = 0; i < 40; i++) {
      await sleep(250);
      try {
        const res = await fetch(`http://127.0.0.1:${PORT}/json`);
        const targets = await res.json();
        const page = targets.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
        if (page) { ws = page.webSocketDebuggerUrl; break; }
      } catch { /* not up yet */ }
    }
    if (!ws) throw new Error('CDP endpoint never came up');
    const sock = new WebSocket(ws);
    await new Promise((res, rej) => { sock.onopen = res; sock.onerror = () => rej(new Error('ws open failed')); });
    // Request/response over CDP by id — so we can force the viewport BEFORE probing.
    let nextId = 1; const pending = new Map();
    sock.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
    const send = (method, params) => new Promise((res) => { const id = nextId++; pending.set(id, res); sock.send(JSON.stringify({ id, method, params })); });
    // --window-size doesn't set the viewport over CDP headless — Emulation does. This
    // is the tool weakness the probe found in ITSELF: now it honors the requested width.
    await send('Emulation.setDeviceMetricsOverride',
      { width: parseInt(W, 10), height: parseInt(H, 10), deviceScaleFactor: 1, mobile: true });
    await sleep(SETTLE); // let the SPA reflow at the forced width
    const result = await send('Runtime.evaluate', { expression: PROBE, returnByValue: true });
    sock.close();
    const val = result.result?.result?.value;
    if (!val) { console.error('inspect: no result', JSON.stringify(result).slice(0, 400)); process.exit(1); }
    if (val.error) { console.error('inspect:', val.error); process.exit(1); }
    console.log(`viewport innerWidth = ${val.viewport}px · chain (leaf → :host), OVERFLOW where scrollW > clientW:\n`);
    for (const n of val.chain) {
      const flag = n.overflows ? ' ⟵ OVERFLOWS' : '';
      console.log(`  ${n.tag}.${n.cls.replace(/\\s+/g, '.')}  client=${n.clientW} scroll=${n.scrollW}${flag}`);
      console.log(`      min-width:${n.minWidth}  width:${n.width}  overflow-x:${n.overflowX}  white-space:${n.whiteSpace}  word-break:${n.wordBreak}`);
    }
  } finally { cleanup(); }
}

cdp().catch((e) => { console.error('inspect: ' + e.message); process.exit(1); });
