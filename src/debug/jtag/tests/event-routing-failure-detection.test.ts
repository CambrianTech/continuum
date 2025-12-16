#!/usr/bin/env tsx
/**
 * Event Routing Failure Detection Test
 *
 * SIMPLE TEST: Detect the browser event routing issue
 * ASSERTION: Browser DOM should NOT update without manual refresh
 * CAUSALITY: Browser → Server ✅ | Server → Browser ❌
 */

async function detectEventRoutingFailure(): Promise<void> {
  console.log(`🔥 EVENT ROUTING FAILURE DETECTION TEST`);
  console.log(`🎯 ASSERTION: Browser DOM will NOT update automatically`);

  const testMessage = `DETECT-FAILURE-${Date.now()}`;
  console.log(`📝 Test message: "${testMessage}"`);
  console.log('');

  // Step 1: Capture baseline browser HTML
  console.log(`📸 STEP 1: Capture baseline browser HTML`);
  const beforeHTML = await runJTAGCommand('debug/html-inspector', { selector: 'chat-widget' });

  if (!beforeHTML?.success) {
    console.log(`❌ FAILED: Cannot capture baseline HTML`);
    return;
  }
  console.log(`✅ Baseline captured (${beforeHTML.html?.length || 0} chars)`);

  // Step 2: Send message via browser UI
  console.log(`\n🚀 STEP 2: Send message via browser UI`);
  const uiSend = await runJTAGCommand('exec', {
    code: `
      const widget = document.querySelector('continuum-widget')?.shadowRoot?.querySelector('main-widget')?.shadowRoot?.querySelector('chat-widget');
      const input = widget?.shadowRoot?.querySelector('.message-input');
      if (input && widget.sendMessage) {
        input.value = '${testMessage}';
        widget.sendMessage();
        'BROWSER_SEND_SUCCESS';
      } else {
        'BROWSER_SEND_FAILED';
      }
    `
  });

  if (!uiSend?.success || !uiSend.result?.includes('BROWSER_SEND_SUCCESS')) {
    console.log(`❌ FAILED: Browser UI send failed`);
    return;
  }
  console.log(`✅ Browser UI send successful`);

  // Step 3: Verify server has the message
  console.log(`\n💾 STEP 3: Verify server storage`);
  await sleep(1000); // Give server time to store

  const serverCheck = await runJTAGCommand('collaboration/chat/get-messages', { roomId: 'general', limit: 3 });

  if (!serverCheck?.success || !serverCheck.messages?.length) {
    console.log(`❌ FAILED: Server storage check failed`);
    return;
  }

  const messageFound = serverCheck.messages.some((msg: any) =>
    msg.content?.text?.includes(testMessage)
  );

  if (!messageFound) {
    console.log(`❌ FAILED: Message not found on server`);
    return;
  }
  console.log(`✅ Server has the message - storage works`);

  // Step 4: THE CRITICAL TEST - Check if browser HTML updated automatically
  console.log(`\n🔍 STEP 4: Check browser HTML for automatic update`);
  console.log(`⏰ Waiting 3 seconds for real-time event...`);
  await sleep(3000);

  const afterHTML = await runJTAGCommand('debug/html-inspector', { selector: 'chat-widget' });

  if (!afterHTML?.success) {
    console.log(`❌ FAILED: Cannot capture after HTML`);
    return;
  }

  const messageInHTML = afterHTML.html?.includes(testMessage.substring(0, 15)) || false;

  // ASSERTION: This SHOULD fail if events are broken
  console.log(`\n🚨 CRITICAL ASSERTION:`);
  if (messageInHTML) {
    console.log(`✅ UNEXPECTED: Message found in browser HTML!`);
    console.log(`🎉 BREAKTHROUGH: Real-time events are actually working!`);
  } else {
    console.log(`❌ EXPECTED FAILURE: Message NOT found in browser HTML`);
    console.log(`💡 CONFIRMED: Real-time event routing is broken`);
    console.log(`📋 Issue: Server→Browser events don't trigger DOM updates`);
  }

  console.log(`\n📊 DETECTION SUMMARY:`);
  console.log(`   Browser Send: ✅ Working`);
  console.log(`   Server Storage: ✅ Working`);
  console.log(`   Real-time Events: ${messageInHTML ? '✅ Working' : '❌ Broken'}`);
  console.log(`   DOM Updates: ${messageInHTML ? '✅ Working' : '❌ Broken'}`);

  if (!messageInHTML) {
    console.log(`\n🔥 ROOT CAUSE: EventBridge routing server→browser is broken`);
  }
}

/**
 * Simple JTAG command runner
 */
async function runJTAGCommand(command: string, params: any = {}): Promise<any> {
  const { exec } = await import('child_process');
  const { promisify } = await import('util');
  const execAsync = promisify(exec);

  try {
    const paramsStr = Object.keys(params).map(key =>
      `--${key}="${JSON.stringify(params[key]).replace(/"/g, '\\"')}"`
    ).join(' ');

    const result = await execAsync(`./jtag ${command} ${paramsStr}`);

    // Try to parse JSON from the command output
    const jsonMatch = result.stdout.match(/COMMAND RESULT:\s*(\{[\s\S]*?\})\s*====/);
    if (jsonMatch) {
      return JSON.parse(jsonMatch[1]);
    }

    return { success: true, result: result.stdout };
  } catch (error) {
    return { success: false, error: error instanceof Error ? error.message : String(error) };
  }
}

/**
 * Sleep utility
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Execute if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  detectEventRoutingFailure().catch(console.error);
}

export { detectEventRoutingFailure };