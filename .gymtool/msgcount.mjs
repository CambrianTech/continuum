import { chromium } from 'playwright';
const url = process.argv[2];
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
await page.goto(url, { waitUntil: 'networkidle' });
await page.waitForTimeout(4000);
const a = await page.evaluate(() => {
  const w = document.querySelector('chat-widget');
  const msgs = w?.state?.messages ?? [];
  const ids = msgs.map(m => m.id);
  const contentKeys = msgs.map(m => `${m.sender_id}|${m.content?.slice(0,60)}`);
  return {
    count: msgs.length,
    revision: w?.state ? 'has-state' : 'none',
    dupIds: ids.length - new Set(ids).size,
    dupContent: contentKeys.length - new Set(contentKeys).size,
    first: msgs[0] ? { t: msgs[0].timestamp, c: msgs[0].content?.slice(0,50) } : null,
    last: msgs.at(-1) ? { t: msgs.at(-1).timestamp, c: msgs.at(-1).content?.slice(0,50) } : null,
  };
});
console.log(JSON.stringify(a, null, 2));
await page.waitForTimeout(6000);
const b = await page.evaluate(() => {
  const w = document.querySelector('chat-widget');
  return { countAfter6s: w?.state?.messages?.length ?? 0 };
});
console.log(JSON.stringify(b));
await browser.close();
