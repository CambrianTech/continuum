import { chromium } from 'playwright';
const url = process.argv[2];
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
await page.goto(url, { waitUntil: 'networkidle' });
await page.waitForTimeout(4000);
const before = await page.evaluate(() => {
  const w = document.querySelector('chat-widget');
  return { tab: w?.nav?.current_tab?.slice(0, 8), tabs: w?.nav?.open_tabs?.length };
});
// Click the FIRST genome slots block in the roster (Benchy's tile).
const clicked = await page.evaluate(() => {
  const w = document.querySelector('chat-widget');
  const g = w?.shadowRoot?.querySelector('.member .genome-slots');
  if (!g) return 'no genome-slots found';
  g.dispatchEvent(new MouseEvent('click', { bubbles: true, composed: true }));
  return 'clicked';
});
await page.waitForTimeout(2500);
const after = await page.evaluate(() => {
  const w = document.querySelector('chat-widget');
  const home = w?.shadowRoot?.querySelector('.persona-home');
  const genomeCard = w?.shadowRoot?.querySelector('.persona-home #genome');
  const what = w?.shadowRoot?.querySelector('.what');
  return {
    tab: w?.nav?.current_tab?.slice(0, 8),
    tabs: w?.nav?.open_tabs?.map(t => `${t.kind}:${t.title}`),
    personaHomeRendered: !!home,
    genomeCardExists: !!genomeCard,
    scrollTop: what?.scrollTop,
    genomeCardTop: genomeCard ? genomeCard.getBoundingClientRect().top : null,
  };
});
console.log(JSON.stringify({ before, clicked, after }, null, 2));
await browser.close();
