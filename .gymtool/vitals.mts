/**
 * Hot-path vitals probe: dump the EXACT faculty/meter numbers reaching the DOM
 * for every roster row — the ground truth for "is the diamond wired or just dim".
 * Usage: npx tsx .gymtool/vitals.ts <url> [--watch-seconds N]
 */
import { chromium } from 'playwright';

interface RowVitals {
  readonly name: string;
  readonly meters: string;
  readonly faculties: readonly string[];
  readonly genomeTitle: string;
}

const url = process.argv[2] ?? 'http://localhost:5177/';
const watchSeconds = Number(process.argv[4] ?? 0);

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
await page.goto(url, { waitUntil: 'networkidle' });
await page.waitForTimeout(5000);

async function sample(label: string): Promise<void> {
  const rows = await page.evaluate((): RowVitals[] => {
    const w = document.querySelector('chat-widget');
    const root = w?.shadowRoot;
    if (!root) return [];
    return Array.from(root.querySelectorAll('.member-row, .listing-row, li')).flatMap((li) => {
      const diamond = li.querySelector('.cog-diamond');
      if (!diamond) return [];
      const name = li.querySelector('.member-name, .name, strong, b')?.textContent?.trim() ?? '?';
      const faculties = Array.from(diamond.querySelectorAll('polygon title')).map(
        (t) => t.textContent?.trim() ?? '',
      );
      const genomeTitle = li.querySelector('.genome-panel')?.getAttribute('title') ?? '';
      const meters =
        li.querySelector('.meters, .vitals')?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
      return [{ name, meters, faculties, genomeTitle }];
    });
  });
  console.log(`--- ${label} ---`);
  for (const r of rows) {
    console.log(`${r.name}: [${r.faculties.join(', ')}] genome="${r.genomeTitle}" meters="${r.meters}"`);
  }
}

await sample('t=0');
if (watchSeconds > 0) {
  await page.waitForTimeout(watchSeconds * 1000);
  await sample(`t=${watchSeconds}s`);
}
await browser.close();
