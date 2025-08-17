#!/usr/bin/env tsx
/**
 * Cross-Environment Events Integration Test
 * 
 * Validates that events emitted from server reach browser environment.
 */

import { jtag } from '../../server-index';

async function testCrossEnvironmentEvents() {
  console.log('🧪 INTEGRATION TEST: Cross-Environment Events...');
  
  try {
    const client = await jtag.connect({ targetEnvironment: 'server' });

    // Setup browser event listener
    const setupResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (function() {
            const jtagSystem = window.jtag;
            if (!jtagSystem?.eventManager?.events) {
              return { success: false, error: 'JTAG event system not available' };
            }
            
            let eventCount = 0;
            
            const listener = (data) => {
              eventCount++;
              console.log('✅ BROWSER: Cross-environment event received:', eventCount);
            };
            
            jtagSystem.eventManager.events.on('chat-message-sent', listener);
            
            // Store counter for later check
            window.crossEnvEventCount = eventCount;
            
            return { success: true, listenerReady: true };
          })();
        `
      }
    });

    if (!setupResult.commandResult?.result?.success) {
      throw new Error('Failed to setup browser event listener');
    }

    // Send chat message that should emit event
    const chatResult = await client.commands['chat/send-message']({
      context: { uuid: 'cross-env-test', environment: 'server' },
      sessionId: 'cross-env-test-session',
      roomId: 'cross-env-test-room',
      content: 'Cross-environment test message',
      category: 'chat'
    });

    if (!chatResult.success) {
      throw new Error('Failed to send chat message');
    }

    // Wait for event propagation
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Check if browser received event
    const checkResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return { eventCount: window.crossEnvEventCount || 0 };
        `
      }
    });

    const result = checkResult.commandResult?.result;
    
    if (!result?.eventCount || result.eventCount === 0) {
      console.error('❌ CROSS-ENVIRONMENT EVENTS NOT WORKING');
      console.error(`📊 Events received: ${result?.eventCount || 0}`);
      process.exit(1);
    }

    console.log('🎉 CROSS-ENVIRONMENT EVENTS WORKING!');
    console.log(`📊 Events received: ${result.eventCount}`);
    console.log('✅ Integration test passed');
    
  } catch (error) {
    console.error('❌ Integration test failed:', error);
    process.exit(1);
  }
}

testCrossEnvironmentEvents();