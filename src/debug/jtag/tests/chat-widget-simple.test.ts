#!/usr/bin/env npx tsx
/**
 * Simple Chat Widget Test
 * 
 * Clean, careful test that:
 * 1. Takes initial screenshot
 * 2. Sends ONE message
 * 3. Takes final screenshot  
 * 4. Exits cleanly
 */

import { jtag } from '../server-index';

async function testChatWidgetCarefully(): Promise<void> {
  console.log('🧪 CAREFUL CHAT WIDGET TEST');
  console.log('============================');

  let client = null;
  
  try {
    // Connect once
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected');
    
    // Take initial screenshot
    console.log('📸 1. Taking initial screenshot...');
    await client.commands.screenshot({
      filename: 'widget-test-before.png',
      querySelector: 'chat-widget'
    });
    console.log('✅ Initial screenshot saved');
    
    // Send ONE message
    console.log('💬 2. Sending ONE test message...');
    await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          const chatWidget = document.querySelector('chat-widget');
          const input = chatWidget?.shadowRoot?.getElementById('messageInput');
          const button = chatWidget?.shadowRoot?.getElementById('sendButton');
          
          if (input && button) {
            input.value = 'Single test message';
            button.click();
            console.log('✅ ONE message sent');
          }
          'Done - sent one message';
        `
      }
    });
    
    // Wait for UI to update
    console.log('⏳ 3. Waiting for UI update...');
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Take final screenshot
    console.log('📸 4. Taking final screenshot...');
    await client.commands.screenshot({
      filename: 'widget-test-after.png', 
      querySelector: 'chat-widget'
    });
    console.log('✅ Final screenshot saved');
    
    console.log('');
    console.log('🎉 TEST COMPLETE - NO LOOPS');
    console.log('📸 Check: widget-test-before.png vs widget-test-after.png');
    
  } catch (error) {
    console.error('❌ Test failed:', error);
    console.error('❌ Error details:', {
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined
    });
    throw error;
  } finally {
    // Always disconnect
    if (client) {
      try {
        console.log('🔌 Disconnecting...');
        await client.disconnect();
        console.log('✅ Disconnected cleanly');
      } catch (disconnectError) {
        console.error('Disconnect error:', disconnectError);
      }
    }
  }
}

// Run once and exit
testChatWidgetCarefully().then(() => {
  console.log('✅ Test completed successfully');
  process.exit(0);
}).catch(error => {
  console.error('🚨 Test failed:', error);
  process.exit(1);
});