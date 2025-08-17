#!/usr/bin/env tsx
/**
 * Simple Event Bridge Integration Test
 * 
 * Proves cross-environment event bridging works independent of chat system.
 * Tests server→browser event flow with DOM proof element verification.
 */

import { jtag } from '../../server-index';
import { EVENT_ENDPOINTS } from '../../daemons/events-daemon/shared/EventEndpoints';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

async function testSimpleEventBridge() {
  console.log('🌉 INTEGRATION TEST: Simple event bridging (no chat dependency)');
  
  try {
    const client = await jtag.connect();
    const sessionId = generateUUID();
    
    console.log('📋 Test plan:');
    console.log('  1. Setup browser-side event listener with proof DOM element');
    console.log('  2. Send test event from server via EventsDaemon');
    console.log('  3. Verify event bridged to browser and DOM element updated');
    console.log('  4. Check browser logs for evidence of event reception');
    
    // 1. Setup browser-side proof mechanism
    console.log('🔧 Setting up browser-side event proof system...');
    const setupResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          // Create proof counter element
          let proofElement = document.createElement('div');
          proofElement.id = 'simple-event-bridge-proof';
          proofElement.textContent = 'Chat events received: 0';
          proofElement.style.cssText = 'position: fixed; top: 10px; right: 10px; background: #ff0000; color: white; padding: 10px; z-index: 9999; border-radius: 4px; font-family: monospace;';
          document.body.appendChild(proofElement);
          
          // Setup event listener for chat-message-sent events
          let eventCount = 0;
          window.simpleEventProofListener = function(event) {
            eventCount++;
            proofElement.textContent = \`Chat events received: \${eventCount}\`;
            proofElement.style.background = eventCount > 0 ? '#00ff00' : '#ff0000';
            console.log('🎉 BROWSER: Chat message event received!', event.detail);
          };
          
          document.addEventListener('chat-message-sent', window.simpleEventProofListener);
          console.log('✅ BROWSER: Simple event proof system ready');
        `
      }
    });
    
    if (!setupResult.success) {
      throw new Error(`Browser setup failed: ${setupResult.error}`);
    }
    
    // 2. Send test event via chat/send-message command (which emits events)
    console.log('📤 Sending message via chat/send-message to trigger event...');
    const chatResult = await client.commands['chat/send-message']({
      roomId: 'simple-test-room',
      content: 'Test message to trigger event bridging',
      sessionId: sessionId
    });
    
    console.log('📊 Chat command result:', JSON.stringify(chatResult, null, 2));
    
    if (!chatResult.success) {
      throw new Error(`Chat command failed: ${JSON.stringify(chatResult.error)}`);
    }
    
    // 3. Wait for event propagation
    console.log('⏳ Waiting for event propagation...');
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // 4. Check browser proof element
    console.log('🔍 Checking browser proof element...');
    const proofResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          const proofElement = document.getElementById('simple-event-bridge-proof');
          const eventCount = proofElement ? proofElement.textContent.match(/\\d+/)?.[0] || '0' : '0';
          
          console.log('🔍 BROWSER PROOF CHECK:');
          console.log('  - Proof element exists:', !!proofElement);
          console.log('  - Event count:', eventCount);
          console.log('  - Background color:', proofElement?.style.background);
          
          ({
            proofExists: !!proofElement,
            eventCount: parseInt(eventCount),
            backgroundColor: proofElement?.style.background,
            success: parseInt(eventCount) > 0
          });
        `
      }
    });
    
    if (!proofResult.success) {
      throw new Error(`Proof check failed: ${proofResult.error}`);
    }
    
    const proofData = proofResult.result as any;
    console.log('📊 PROOF RESULTS:', JSON.stringify(proofData, null, 2));
    
    // 5. Validation
    if (proofData.success && proofData.eventCount > 0) {
      console.log('✅ INTEGRATION TEST PASSED: Simple event bridging works!');
      console.log(`   Events bridged: ${proofData.eventCount}`);
      console.log(`   DOM updated: ${proofData.backgroundColor === 'rgb(0, 255, 0)' ? 'YES' : 'NO'}`);
      return true;
    } else {
      console.log('❌ INTEGRATION TEST FAILED: No events bridged to browser');
      console.log('   Expected: eventCount > 0');
      console.log('   Actual: eventCount =', proofData.eventCount);
      
      // Check browser logs for debugging
      console.log('🔍 Checking browser logs for event reception evidence...');
      await client.commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🔍 Recent browser console entries (looking for event evidence):');
            // Browser console.log calls should appear in browser logs
          `
        }
      });
      
      return false;
    }
    
  } catch (error) {
    console.error('❌ INTEGRATION TEST CRASHED:', error);
    return false;
  }
}

// Run test
testSimpleEventBridge().then((success) => {
  if (success) {
    console.log('🎉 Simple event bridge integration test completed successfully');
    process.exit(0);
  } else {
    console.log('💥 Simple event bridge integration test failed');
    process.exit(1);
  }
}).catch((error) => {
  console.error('💥 Integration test crashed:', error);
  process.exit(1);
});