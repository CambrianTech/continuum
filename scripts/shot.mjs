#!/usr/bin/env node
// shot.mjs — headless screenshot of a running URL → PNG. The "never blind" feedback
// loop as one command, cross-platform (Windows / macOS / Linux).
//
// The factory must run everywhere the product does ([[solve-for-public-users]]). The
// original shot.sh was bash + a hardcoded macOS Chrome path — useless on Windows. This
// is the portable version: Node (already a repo dep), OS-detected Chrome, one CDP flow.
//
// EVENT-BASED, REAL-TIME CAPTURE — the 2026-08-14 lesson. The first desktop path ran
// Chrome with `--virtual-time-budget`, which fast-forwards PAGE time ahead of real I/O:
// the app's 5s WS-handshake timer fired before the real-world socket (real milliseconds)
// could open, the app killed its own socket, and every capture showed a "broken"
// desktop that was actually healthy (netlog: request CANCELLED, zero bytes sent). A
// screenshot tool must never bend the page's clock. This version runs in REAL time and
// captures on a READINESS EVENT, with the budget only as a cap:
//
//   readiness (polled via CDP Runtime.evaluate, override with SHOT_READY_JS):
//     - a continuum app stamps `<html data-feed-status="…">` (apps/web/src/index.ts) —
//       wait for "live", the E2E signal that a real State frame landed (health is
//       delivery, never socket existence).
//     - any other page: `document.readyState === "complete"`.
//
// Usage:
//   node scripts/shot.mjs                                  # http://localhost:5173/
//   node scripts/shot.mjs "http://localhost:5173/?me=<id>" out.png
// Env: CHROME (explicit binary), SHOT_SIZE (WxH, default 1600,1000),
//      SHOT_BUDGET_MS (readiness CAP, default 15000 — capture happens EARLIER on the
//      readiness event; on cap expiry the capture still lands, labeled not-ready),
//      SHOT_READY_JS (JS expression; truthy = capture now),
//      SHOT_SETTLE_MS (paint settle after readiness, default 400),
//      SHOT_MOBILE=1 (+SHOT_DSR) for a TRUE mobile viewport via CDP device-metrics.

import { spawn } from 'node:child_process';
import { existsSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir, platform } from 'node:os';
import { join } from 'node:path';

const URL = process.argv[2] || 'http://localhost:5173/';
const OUT = process.argv[3] || join(tmpdir(), 'continuum-shot.png');
const SIZE = process.env.SHOT_SIZE || '1600,1000';
const BUDGET = parseInt(process.env.SHOT_BUDGET_MS || '15000', 10);
const SETTLE = parseInt(process.env.SHOT_SETTLE_MS || '400', 10);
const MOBILE = process.env.SHOT_MOBILE === '1' || process.env.SHOT_MOBILE === 'true';
const DSR = parseInt(process.env.SHOT_DSR || '2', 10);
// Default readiness: a continuum app's E2E feed stamp wins; plain pages settle on load.
const READY_JS =
  process.env.SHOT_READY_JS ||
  `(() => {
    const fs = document.documentElement.dataset.feedStatus;
    if (fs !== undefined) return fs === 'live' || fs === 'cached';
    return document.readyState === 'complete';
  })()`;

// OS-detected Chrome — the whole point of the port. First hit wins; env override first.
function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const candidates = {
    darwin: [
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
    ],
    win32: [
      'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
      'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
      join(process.env.LOCALAPPDATA || '', 'Google\\Chrome\\Application\\chrome.exe'),
      'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe', // Edge is Chromium
    ],
    linux: [
      '/usr/bin/google-chrome',
      '/usr/bin/google-chrome-stable',
      '/usr/bin/chromium',
      '/usr/bin/chromium-browser',
    ],
  }[platform()] || [];
  return candidates.find((p) => p && existsSync(p));
}

const chrome = findChrome();
if (!chrome) {
  console.error(`shot: no Chrome/Chromium/Edge found for ${platform()} — set CHROME=<path>.`);
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function shot() {
  const [W, H] = SIZE.split(',').map((n) => parseInt(n, 10));
  const PORT = 9200 + (process.pid % 800);
  const child = spawn(chrome, [
    '--headless=new', '--disable-gpu', '--hide-scrollbars', '--no-first-run',
    `--user-data-dir=${join(tmpdir(), 'continuum-shot-' + process.pid)}`,
    `--window-size=${W},${H}`, `--remote-debugging-port=${PORT}`, URL,
  ], { stdio: 'ignore' });
  const cleanup = () => { try { child.kill(); } catch { /* already gone */ } };
  try {
    // CDP attach — the page target appears once Chrome is up.
    let wsUrl;
    for (let i = 0; i < 40; i++) {
      await sleep(250);
      try {
        const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
        const page = targets.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
        if (page) { wsUrl = page.webSocketDebuggerUrl; break; }
      } catch { /* not up yet */ }
    }
    if (!wsUrl) throw new Error('CDP endpoint never came up');
    const sock = new WebSocket(wsUrl);
    await new Promise((res, rej) => { sock.onopen = res; sock.onerror = () => rej(new Error('CDP ws open failed')); });
    let nextId = 1; const pending = new Map();
    sock.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
    const send = (method, params) => new Promise((res) => { const id = nextId++; pending.set(id, res); sock.send(JSON.stringify({ id, method, params })); });

    if (MOBILE) {
      await send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: DSR, mobile: true });
    }

    // Event-based readiness in REAL time: poll the page's own signal; the budget
    // only caps the wait. On expiry we still capture (a not-ready frame is itself
    // diagnostic) and SAY so — safe feedback, never a silent lie.
    const t0 = Date.now();
    let ready = false; let lastState = 'unknown';
    while (Date.now() - t0 < BUDGET) {
      const r = await send('Runtime.evaluate', { expression: READY_JS, returnByValue: true });
      ready = r.result?.result?.value === true;
      if (ready) break;
      const s = await send('Runtime.evaluate', {
        expression: `document.documentElement.dataset.feedStatus ?? document.readyState`,
        returnByValue: true,
      });
      lastState = String(s.result?.result?.value ?? 'unknown');
      await sleep(250);
    }
    if (ready && SETTLE > 0) await sleep(SETTLE); // let the ready frame paint

    const shotR = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
    sock.close();
    const b64 = shotR.result?.data;
    if (!b64) throw new Error('captureScreenshot returned no data');
    writeFileSync(OUT, Buffer.from(b64, 'base64'));
    const kb = Math.round(statSync(OUT).size / 1024);
    const how = ready
      ? `ready in ${Date.now() - t0 - (SETTLE > 0 ? SETTLE : 0)}ms`
      : `NOT ready after ${BUDGET}ms cap (last state: ${lastState}) — captured anyway`;
    const mode = MOBILE ? `mobile ${W}x${H} @${DSR}x` : `${W}x${H}`;
    console.log(`shot: ${URL} → ${OUT} (${kb}KB, ${mode}, ${how})`);
    if (!ready) process.exitCode = 2; // distinguishable: image exists, page wasn't ready
  } finally { cleanup(); }
}

shot().catch((e) => { console.error('shot: ' + e.message); process.exit(1); });
