import { chromium } from 'playwright';
const url = process.argv[2] ?? 'http://localhost:5177/';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const logs = [];
page.on('console', (m) => logs.push(`[${m.type()}] ${m.text()}`));
await page.goto(url, { waitUntil: 'networkidle' });
await page.waitForTimeout(5000);
await page.screenshot({ path: process.argv[3] ?? '/tmp/web.png' });
// Introspect the widget's nav state
const nav = await page.evaluate(() => {
  const w = document.querySelector('chat-widget');
  if (!w) return { error: 'no chat-widget' };
  const tabs = w.shadowRoot?.querySelectorAll('.tab');
  const cells = w.shadowRoot?.querySelectorAll('.cell');
  return {
    hasNav: !!w.nav,
    openTabs: w.nav?.open_tabs?.map(t => ({ id: t.id.slice(0,8), title: t.title, kind: t.kind, unread: t.unread })) ?? null,
    currentTab: w.nav?.current_tab?.slice(0,8) ?? null,
    tabBarTabCount: tabs?.length ?? 0,
    railCellCount: cells?.length ?? 0,
    hasState: !!w.state,
    roomName: w.state?.room_name,
  };
});
console.log(JSON.stringify(nav, null, 2));
console.log('--- console tail ---');
console.log(logs.slice(-10).join('\n'));
await browser.close();
