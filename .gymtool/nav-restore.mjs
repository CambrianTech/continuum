import { chromium } from 'playwright';
const [url, target] = [process.argv[2], process.argv[3]];
const browser = await chromium.launch();
const page = await browser.newPage();
await page.goto(url, { waitUntil: 'networkidle' });
await page.waitForTimeout(2500);
const r = await page.evaluate(async (t) => {
  const w = document.querySelector('chat-widget');
  if (!w?.selectRoomHandler) return 'no handler';
  await w.selectRoomHandler(t, 'room');
  return 'restored to ' + t.slice(0, 8);
}, target);
console.log(r);
await browser.close();
