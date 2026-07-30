import { chromium } from 'playwright';
const browser = await chromium.launch();
const page = await browser.newPage();
await page.goto(process.argv[2], { waitUntil: 'networkidle' });
await page.waitForTimeout(6000);
const r = await page.evaluate(() => {
  const w = document.querySelector('chat-widget');
  const rows = w?.shadowRoot?.querySelectorAll('.messages .msg');
  const first = rows?.[0]?.textContent?.trim().slice(0, 60);
  return { renderedRows: rows?.length ?? 0, wireMessages: w?.state?.messages?.length ?? 0, first };
});
console.log(JSON.stringify(r, null, 2));
await browser.close();
