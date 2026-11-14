// The user gate, step 1 (docs/planning/PRODUCT-FOR-USERS.md): the release, installed as a user gets
// it, shows a working desktop.
//
//   npx tsx scripts/ci/user-gate.ts <extracted package dir> [out dir]
//
// Starts the core the way a user does (`continuum start`), opens the desktop, waits on the page's own
// `<html data-feed-status>` through perception's settle() (never a clock), and writes the frame and
// the status. Pass = the desktop reached `live` or `cached`; anything else fails naming what it showed.
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { chromium } from 'playwright';
import { findChromium, settle } from '../../packages/perception';

const [pkg, out = 'user-gate'] = process.argv.slice(2);
if (pkg === undefined) throw new Error('usage: user-gate.ts <extracted package dir> [out dir]');
mkdirSync(out, { recursive: true });

const exe = process.platform === 'win32' ? '.exe' : '';
const start = spawnSync(join(pkg, `continuum${exe}`), ['start'], { encoding: 'utf8' });
writeFileSync(join(out, 'start.log'), `${start.stdout}\n${start.stderr}`);
if (start.status !== 0) {
  console.error(`continuum start exited ${start.status}: ${out}/start.log`);
  process.exit(1);
}

const executablePath = findChromium();
const browser = await chromium.launch(executablePath === undefined ? {} : { executablePath });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await page.goto('http://127.0.0.1:8975/', { waitUntil: 'networkidle' });
  await settle(page);
  await page.screenshot({ path: join(out, 'desktop.png') });
  const status = await page.evaluate(() => document.documentElement.dataset.feedStatus ?? 'undeclared');
  writeFileSync(join(out, 'status.txt'), `${status}\n`);
  if (status !== 'live' && status !== 'cached') {
    console.error(`the desktop settled at '${status}', not live: ${out}/desktop.png`);
    process.exit(1);
  }
  console.log(`desktop ${status}`);
} finally {
  await browser.close();
}
