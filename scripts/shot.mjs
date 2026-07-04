#!/usr/bin/env node
// shot.mjs — headless screenshot of a running URL → PNG. The "never blind" feedback
// loop as one command, cross-platform (Windows / macOS / Linux).
//
// The factory must run everywhere the product does ([[solve-for-public-users]]). The
// original shot.sh was bash + a hardcoded macOS Chrome path — useless on Windows. This
// is the portable version: Node (already a repo dep), OS-detected Chrome, a wall-clock
// guard so a live-WebSocket page can't hang the capture (Chrome writes the PNG but never
// idles, so it won't exit on its own — found by dogfooding the live three-panel app).
//
// Usage:
//   node scripts/shot.mjs                                  # http://localhost:5173/
//   node scripts/shot.mjs "http://localhost:5173/?me=<id>" out.png
// Env: CHROME (explicit binary), SHOT_SIZE (WxH, default 1600,1000),
//      SHOT_BUDGET_MS (SPA settle budget, default 6000).

import { spawn } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { tmpdir, platform } from 'node:os';
import { join } from 'node:path';

const URL = process.argv[2] || 'http://localhost:5173/';
const OUT = process.argv[3] || join(tmpdir(), 'continuum-shot.png');
const SIZE = process.env.SHOT_SIZE || '1600,1000';
const BUDGET = parseInt(process.env.SHOT_BUDGET_MS || '6000', 10);

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

const args = [
  '--headless=new', '--disable-gpu', '--hide-scrollbars', '--no-first-run',
  `--user-data-dir=${join(tmpdir(), 'continuum-shot-profile-' + process.pid)}`,
  `--window-size=${SIZE}`,
  `--virtual-time-budget=${BUDGET}`,
  `--screenshot=${OUT}`,
  URL,
];

const child = spawn(chrome, args, { stdio: 'ignore' });

// Wall-clock guard: give it the budget + margin to write the frame, then reap. A live
// page keeps the socket busy so Chrome never exits on its own; the capture still lands.
const deadlineMs = BUDGET + 8000;
const killer = setTimeout(() => { try { child.kill(); } catch {} }, deadlineMs);

function done() {
  clearTimeout(killer);
  try { child.kill(); } catch {}
  if (existsSync(OUT) && statSync(OUT).size > 0) {
    const kb = Math.round(statSync(OUT).size / 1024);
    console.log(`shot: ${URL} → ${OUT} (${kb}KB, size ${SIZE})`);
    process.exit(0);
  }
  console.error(`shot: FAILED — no image written. Is '${URL}' reachable/serving?`);
  process.exit(1);
}

child.on('exit', done);
child.on('error', (e) => { clearTimeout(killer); console.error(`shot: spawn failed — ${e.message}`); process.exit(1); });
// Backstop: if 'exit' never fires (it should, via the killer), finalize anyway.
setTimeout(done, deadlineMs + 2000);
