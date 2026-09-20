#!/usr/bin/env node
// preview-record.mjs — headless VIDEO capture of the REAL <chat-widget> preview.
//
// The feedback loop's motion dimension ([[feedback-is-a-first-class-cross-modality
// -dimension]]): screenshots prove layout; THIS proves animation — speaking-border
// pulses, streaming caption carets, video frames painting, presenter motion. Spins
// the actual Vite pipeline (same as preview-shot.mjs), drives Chrome over raw CDP
// `Page.startScreencast`, and writes:
//   out-dir/frame-NNN.jpg          — the captured frames (real cadence)
//   out-dir/contact-sheet.png      — a labeled grid of sampled frames (chat-ready
//                                    animation proof; distinct cells = motion)
//   out-dir/ffmpeg-hint.txt        — one command to make an mp4 where ffmpeg exists
//
// Usage:
//   node scripts/preview-record.mjs out-dir "fixture=live" [seconds] [WxH]
// Env: PREVIEW_PORT (default 5199).

import { spawn } from 'node:child_process';
import { get } from 'node:http';
import { existsSync, mkdirSync, writeFileSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const webDir = join(here, '..', 'apps', 'web');
const OUT_DIR = resolve(process.argv[2] || join(tmpdir(), 'continuum-preview-rec'));
const QUERY = process.argv[3] || 'fixture=live';
const SECONDS = parseFloat(process.argv[4] || '4');
const SIZE = (process.argv[5] || '900,700').replace('x', ',');
const PORT = parseInt(process.env.PREVIEW_PORT || '5199', 10);
const URL_ = `http://localhost:${PORT}/preview.html?${QUERY}`;

function findChrome() {
  const candidates =
    process.platform === 'darwin'
      ? ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome']
      : ['/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium'];
  const hit = candidates.find((c) => existsSync(c));
  if (!hit) throw new Error('Chrome not found');
  return hit;
}

const waitHttp = (url, tries = 60) =>
  new Promise((res, rej) => {
    const tick = (n) =>
      get(url, () => res()).on('error', () =>
        n <= 0 ? rej(new Error(`vite never came up at ${url}`)) : setTimeout(() => tick(n - 1), 250),
      );
    tick(tries);
  });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  mkdirSync(OUT_DIR, { recursive: true });
  // 1. Vite (reuses a running server on PORT when present).
  let vite;
  let started = false;
  try {
    await waitHttp(`http://localhost:${PORT}/preview.html`, 2);
  } catch {
    vite = spawn('npx', ['vite', '--port', String(PORT), '--strictPort'], {
      cwd: webDir,
      stdio: 'ignore',
    });
    started = true;
    await waitHttp(`http://localhost:${PORT}/preview.html`);
  }

  const [W, H] = SIZE.split(',').map((n) => parseInt(n, 10));
  const DBG = 9300 + (process.pid % 600);
  const chrome = spawn(
    findChrome(),
    [
      '--headless=new',
      '--disable-gpu',
      '--hide-scrollbars',
      '--no-first-run',
      `--user-data-dir=${join(tmpdir(), 'continuum-rec-' + process.pid)}`,
      `--window-size=${W},${H}`,
      `--remote-debugging-port=${DBG}`,
      URL_,
    ],
    { stdio: 'ignore' },
  );
  const cleanup = () => {
    try {
      chrome.kill();
    } catch {}
    if (started)
      try {
        vite?.kill();
      } catch {}
  };
  try {
    let wsUrl;
    for (let i = 0; i < 40 && !wsUrl; i++) {
      await sleep(250);
      try {
        const targets = await (await fetch(`http://127.0.0.1:${DBG}/json`)).json();
        wsUrl = targets.find((t) => t.type === 'page' && t.webSocketDebuggerUrl)?.webSocketDebuggerUrl;
      } catch {}
    }
    if (!wsUrl) throw new Error('CDP endpoint never came up');
    const sock = new WebSocket(wsUrl);
    await new Promise((res, rej) => {
      sock.onopen = res;
      sock.onerror = () => rej(new Error('ws open failed'));
    });
    let nextId = 1;
    const pending = new Map();
    const frames = [];
    sock.onmessage = (m) => {
      const d = JSON.parse(m.data);
      if (d.id && pending.has(d.id)) {
        pending.get(d.id)(d);
        pending.delete(d.id);
      } else if (d.method === 'Page.screencastFrame') {
        frames.push({ data: d.params.data, ts: d.params.metadata?.timestamp ?? 0 });
        sock.send(
          JSON.stringify({
            id: nextId++,
            method: 'Page.screencastFrameAck',
            params: { sessionId: d.params.sessionId },
          }),
        );
      }
    };
    const send = (method, params) =>
      new Promise((res) => {
        const id = nextId++;
        pending.set(id, res);
        sock.send(JSON.stringify({ id, method, params }));
      });

    await send('Page.enable', {});
    await sleep(1800); // let the widget paint
    await send('Page.startScreencast', {
      format: 'jpeg',
      quality: 75,
      maxWidth: W,
      maxHeight: H,
      everyNthFrame: 1,
    });
    await sleep(SECONDS * 1000);
    await send('Page.stopScreencast', {});
    if (frames.length === 0) throw new Error('screencast produced no frames');
    frames.forEach((f, i) => {
      writeFileSync(join(OUT_DIR, `frame-${String(i).padStart(3, '0')}.jpg`), Buffer.from(f.data, 'base64'));
    });

    // 2. Contact sheet — sample up to 8 frames into a labeled grid, rendered by
    //    the SAME chrome (a data: URL montage page), screenshot as PNG.
    const n = Math.min(8, frames.length);
    const picks = Array.from({ length: n }, (_, i) => Math.floor((i * (frames.length - 1)) / Math.max(1, n - 1)));
    const t0 = frames[0].ts;
    const cells = picks
      .map((idx) => {
        const f = frames[idx];
        const ms = t0 ? Math.round((f.ts - t0) * 1000) : idx;
        return `<figure><img src="data:image/jpeg;base64,${f.data}"><figcaption>t+${ms}ms · #${idx}</figcaption></figure>`;
      })
      .join('');
    const montage = `<!doctype html><style>body{margin:0;background:#0b0e14;font:11px monospace;color:#9fb2c8}
      main{display:grid;grid-template-columns:repeat(4,1fr);gap:6px;padding:8px}
      figure{margin:0}img{width:100%;display:block;border:1px solid #223}figcaption{padding:2px 0}</style>
      <main>${cells}</main>`;
    await send('Page.navigate', { url: 'data:text/html;base64,' + Buffer.from(montage).toString('base64') });
    await sleep(1200);
    await send('Emulation.setDeviceMetricsOverride', {
      width: 1400,
      height: Math.ceil(n / 4) * Math.round((H / W) * 340 + 30) + 20,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await sleep(300);
    const shot = await send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(join(OUT_DIR, 'contact-sheet.png'), Buffer.from(shot.result.data, 'base64'));
    writeFileSync(
      join(OUT_DIR, 'ffmpeg-hint.txt'),
      `ffmpeg -framerate ${Math.max(1, Math.round(frames.length / SECONDS))} -i frame-%03d.jpg -pix_fmt yuv420p recording.mp4\n`,
    );
    sock.close();
    console.log(
      `rec: ${URL_} → ${frames.length} frames over ${SECONDS}s → ${OUT_DIR} (contact-sheet.png + frames)`,
    );
  } finally {
    cleanup();
  }
}

main().catch((e) => {
  console.error('preview-record failed:', e.message);
  process.exit(1);
});
