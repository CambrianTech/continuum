#!/usr/bin/env npx tsx
/**
 * Browser Ping Test - Minimal Precommit Validation
 *
 * Verifies that:
 * 1. Server is running
 * 2. Browser can connect
 * 3. Browser can execute ping command
 * 4. Ping returns success
 *
 * This is the MINIMUM viable test for precommit validation.
 */

import { jtag } from '../../server-index';

async function testBrowserPing(): Promise<void> {
  console.log('🏓 BROWSER PING TEST');
  console.log('=================================');

  let client: any;

  try {
    // 1. Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected\n');

    // 2. Execute ping from server context
    console.log('🏓 Testing server ping...');
    const serverPingResult = await client.commands.ping({});

    if (!serverPingResult.success) {
      throw new Error('Server ping failed: ' + JSON.stringify(serverPingResult));
    }
    console.log('✅ Server ping successful\n');

    // 3. Execute basic command to verify system functionality
    console.log('📋 Testing command system (list available commands)...');
    const listResult = await client.commands.list({});

    if (!listResult.success || !listResult.commands || listResult.commands.length === 0) {
      throw new Error('List commands failed: ' + JSON.stringify(listResult));
    }
    console.log(`✅ Command system working (${listResult.commands.length} commands available)\n`);

    // 4. Execute ping from browser context (via screenshot command which uses browser)
    console.log('🏓 Testing browser connectivity...');
    const browserTestResult = await client.commands['interface/screenshot']({
      querySelector: 'body',
      filename: 'precommit-ping-test.png'
    });

    if (!browserTestResult.success) {
      throw new Error('Browser connectivity failed: ' + JSON.stringify(browserTestResult));
    }
    console.log('✅ Browser connected and responsive\n');

    console.log('🎉 BROWSER PING TEST: PASSED');
    console.log('=================================\n');
    process.exit(0);

  } catch (error) {
    console.error('\n❌ Browser ping test failed:', error);
    console.error('❌ Error details:', {
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined
    });
    console.error('🚨 Browser ping test failed:', error instanceof Error ? error.message : error);
    console.log('=================================\n');
    process.exit(1);
  } finally {
    if (client?.disconnect) {
      await client.disconnect();
    }
  }
}

testBrowserPing();
