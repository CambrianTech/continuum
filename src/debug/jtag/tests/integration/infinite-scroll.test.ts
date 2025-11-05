#!/usr/bin/env npx tsx

/**
 * Infinite Scroll Test - SIMPLE VERSION
 *
 * 1. Count messages in DOM
 * 2. Scroll to top
 * 3. Wait 3 seconds
 * 4. Count messages again
 * 5. FAIL if no new messages loaded
 */

import { execSync } from 'child_process';

function run(cmd: string): string {
  console.log(`> ${cmd}`);
  return execSync(cmd, { encoding: 'utf8', timeout: 15000 });
}

function countMessages(): number {
  // Use widget-state with countOnly to get entity count without truncation
  const output = run(`./jtag debug/widget-state --widgetSelector="chat-widget" --countOnly=true`);

  // Save to temp file to avoid JSON parsing issues with large output
  const fs = require('fs');
  const tmpFile = '/tmp/widget-count.json';
  fs.writeFileSync(tmpFile, output);
  const result = JSON.parse(fs.readFileSync(tmpFile, 'utf8'));

  if (!result.success || !result.widgetFound) {
    throw new Error('Could not find chat widget');
  }

  return result.state.entityCount || 0;
}

function triggerInfiniteScroll(): void {
  // Directly call loadMore() on the chat widget's scroller
  // This is what should happen when user scrolls to top
  const code = `
    const cw = document.querySelector('continuum-widget')?.shadowRoot
      ?.querySelector('main-widget')?.shadowRoot
      ?.querySelector('chat-widget');
    if (cw && cw.scroller) {
      console.log('📜 TEST: Manually triggering scroller.loadMore()');
      cw.scroller.loadMore();
    }
  `;

  run(`./jtag exec --code="${code.replace(/\n/g, ' ').replace(/"/g, '\\"')}"`);
  console.log(`✅ Triggered infinite scroll via scroller.loadMore()`);
}

async function test() {
  console.log('🧪 Infinite Scroll Test\n');

  // Step 1: Wait for messages to load, then count
  console.log('📊 Step 1: Wait for messages to load...');
  let before = 0;
  for (let i = 0; i < 10; i++) {
    before = countMessages();
    if (before > 0) {
      console.log(`   Found ${before} messages after ${i} seconds\n`);
      break;
    }
    console.log(`   Waiting for messages... (${i + 1}/10)`);
    await new Promise(resolve => setTimeout(resolve, 1000));
  }

  if (before === 0) {
    throw new Error('No messages loaded after 10 seconds - chat widget may not be initialized');
  }

  // Step 2: Trigger infinite scroll
  console.log('⬆️  Step 2: Trigger infinite scroll...');
  triggerInfiniteScroll();
  console.log('');

  // Step 5: Wait for infinite scroll
  console.log('⏳ Step 5: Wait 3 seconds for infinite scroll...');
  await new Promise(resolve => setTimeout(resolve, 3000));
  console.log('');

  // Step 6: Count again
  console.log('📊 Step 6: Count messages again...');
  const after = countMessages();
  console.log(`   Found ${after} messages\n`);

  // Step 5: Check if more loaded
  const newMessages = after - before;

  if (newMessages > 0) {
    console.log(`✅ SUCCESS: ${newMessages} new messages loaded!`);
    console.log(`   Before: ${before}`);
    console.log(`   After: ${after}`);
  } else {
    console.log(`❌ FAIL: No new messages loaded`);
    console.log(`   Before: ${before}`);
    console.log(`   After: ${after}`);
    process.exit(1);
  }
}

test().catch(err => {
  console.error('❌ Test failed:', err.message);
  process.exit(1);
});
