#!/usr/bin/env node
// preview-shot.mjs — headless PNG of the REAL <chat-widget>, backend-free.
//
// The faithful visual-iteration loop as one command: spin up the actual Vite dev
// server (so theme.css + TS + the element render byte-identical to production),
// point shot.mjs at /preview.html (a fixture mount, no socket), write a PNG, tear
// down. This is the tool whose ABSENCE let a themeless hand-rolled page masquerade
// as "the widget" — here the design tokens are always present because it IS the app
// pipeline. Iterate live instead with `npm run dev` → http://localhost:5173/preview.html.
//
// Usage:
//   node scripts/preview-shot.mjs                         # roster fixture → tmp PNG
//   node scripts/preview-shot.mjs out.png                 # roster fixture → out.png
//   node scripts/preview-shot.mjs out.png "fixture=empty" # a named fixture
//   node scripts/preview-shot.mjs out.png "fixture=roster" 1440,900   # + viewport
// Env: PREVIEW_PORT (default 5199), plus every SHOT_* var shot.mjs honors.

import { spawn } from 'node:child_process';
import { get } from 'node:http';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const webDir = join(here, '..', 'apps', 'web');
const OUT = process.argv[2] || join(tmpdir(), 'continuum-preview.png');
const QUERY = process.argv[3] || 'fixture=roster';
const SIZE = process.argv[4] || process.env.SHOT_SIZE || '1440,900';
const PORT = parseInt(process.env.PREVIEW_PORT || '5199', 10);
const URL = `http://localhost:${PORT}/preview.html?${QUERY}`;

function waitForServer(url, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const poll = () => {
      get(url, (res) => {
        res.resume();
        if (res.statusCode && res.statusCode < 500) resolve();
        else retry();
      }).on('error', retry);
    };
    const retry = () => (Date.now() > deadline ? reject(new Error('vite did not become ready')) : setTimeout(poll, 250));
    poll();
  });
}

const vite = spawn('npx', ['vite', '--port', String(PORT), '--strictPort', '--clearScreen', 'false'], {
  cwd: webDir,
  stdio: ['ignore', 'inherit', 'inherit'],
});

let done = false;
const cleanup = () => {
  if (!done) {
    done = true;
    vite.kill('SIGTERM');
  }
};
process.on('exit', cleanup);
process.on('SIGINT', () => { cleanup(); process.exit(130); });

try {
  await waitForServer(URL);
  await new Promise((resolve, reject) => {
    const shot = spawn('node', [join(here, 'shot.mjs'), URL, OUT], {
      stdio: 'inherit',
      env: { ...process.env, SHOT_SIZE: SIZE },
    });
    shot.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`shot.mjs exited ${code}`))));
    shot.on('error', reject);
  });
} catch (err) {
  console.error(`preview-shot: ${err.message}`);
  cleanup();
  process.exit(1);
}
cleanup();
process.exit(0);
