#!/usr/bin/env npx tsx
/**
 * Trigger Browser Messages Test
 * 
 * This test triggers the actual browser's JTAG demo buttons to send messages
 * through the browser's own WebSocket connection, so you can see the real
 * message flow in the browser's Network panel and console.
 */

import fetch from 'node-fetch';
import { jtag } from '../../index';

async function triggerBrowserJTAGMessages() {
  console.log('🚀 Triggering Browser JTAG Messages Test');
  console.log('=====================================');
  console.log('This test will use the browser\'s own JTAG WebSocket connection');
  console.log('🔍 Check browser Network panel for WebSocket traffic\n');

  jtag.test('BROWSER_TRIGGER_TEST', 'Starting browser JTAG message trigger test');

  try {
    // First, verify the demo server is running
    console.log('🌐 Checking demo server...');
    const serverResponse = await fetch('http://localhost:9002/api/server-info');
    const serverInfo = await serverResponse.json();
    
    if (!serverResponse.ok) {
      console.log('❌ Demo server not accessible');
      return;
    }

    console.log(`✅ Demo server running: ${serverInfo.uuid}`);
    console.log(`📡 JTAG Port: ${serverInfo.jtagPort}`);
    console.log(`🌐 Demo Port: ${serverInfo.demoPort}`);

    // Now trigger browser-side JTAG messages by simulating API calls
    // that the browser demo would make

    console.log('\n🔄 Triggering server-side JTAG logging...');
    
    // These will show up in the server logs and potentially send WebSocket messages
    // to any connected browser clients
    jtag.log('BROWSER_TRIGGER', 'Server-side message that should reach browser clients');
    jtag.critical('BROWSER_TRIGGER', 'Critical message that should reach browser clients');
    jtag.probe('BROWSER_TRIGGER', 'browser_connection_test', { 
      serverUUID: serverInfo.uuid,
      timestamp: Date.now(),
      testType: 'browser_trigger'
    });

    console.log('✅ Server-side messages sent');
    
    // Give some time for messages to propagate
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Try to trigger screenshot (this might send WebSocket messages to browser)
    console.log('\n📸 Triggering server screenshot...');
    const screenshotResult = await jtag.screenshot('browser-trigger-test');
    
    if (screenshotResult.success) {
      console.log(`✅ Screenshot triggered: ${screenshotResult.filepath}`);
    } else {
      console.log(`⚠️ Screenshot result: ${screenshotResult.error}`);
    }

    console.log('\n🎯 Real Browser Testing Instructions:');
    console.log('1. Open http://localhost:9002 in your browser');
    console.log('2. Open browser Developer Tools (F12)');
    console.log('3. Go to Network tab');
    console.log('4. Click the "Test Browser Logging" button on the demo page');
    console.log('5. Click the "Test Cross-Context Communication" button');
    console.log('6. You should see:');
    console.log('   - WebSocket connection to ws://localhost:9001');
    console.log('   - WebSocket messages in Network tab');
    console.log('   - Console logs showing "📨 JTAG WebSocket received:"');
    console.log('   - Messages appearing in the browser log panel');

    jtag.test('BROWSER_TRIGGER_COMPLETE', 'Browser trigger test completed', {
      serverUUID: serverInfo.uuid,
      jtagPort: serverInfo.jtagPort,
      demoPort: serverInfo.demoPort
    });

    console.log('\n✅ Browser JTAG message trigger test completed');
    console.log('🔍 Check browser for actual WebSocket messages now!');

  } catch (error: any) {
    console.log(`❌ Browser trigger test error: ${error.message}`);
    jtag.test('BROWSER_TRIGGER_ERROR', 'Browser trigger test error', { 
      error: error.message 
    });
  }
}

triggerBrowserJTAGMessages();