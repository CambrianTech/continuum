#!/usr/bin/env tsx
/**
 * Cross-Environment Events Integration Test
 * 
 * Validates that events emitted from server reach browser environment.
 */

import { jtag } from '../../server-index';

async function testCrossEnvironmentEvents() {
  console.log('🧪 INTEGRATION TEST: Cross-Environment Events...');
  
  let client: any = null;
  try {
    client = await jtag.connect({ targetEnvironment: 'server' });

    // Setup DOM event listener (correct widget API)
    const setupResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (function() {
            console.log('🔧 Setting up DOM event listener for cross-environment test');
            
            let eventCount = 0;
            
            const listener = (event) => {
              eventCount++;
              console.log('✅ BROWSER: Cross-environment DOM event received:', eventCount, event.detail);
              
              // Update global counter for test check
              window.crossEnvEventCount = eventCount;
              
              // Create visible proof element
              const proofElement = document.createElement('div');
              proofElement.id = 'cross-env-event-proof';
              proofElement.textContent = 'CHAT MESSAGE RECEIVED: ' + (event.detail?.message?.content || event.detail?.message || 'No message');
              proofElement.style.cssText = \`
                position: fixed;
                top: 50px;
                right: 10px;
                background: #00ff00;
                color: black;
                padding: 10px;
                border-radius: 4px;
                z-index: 10000;
                max-width: 300px;
              \`;
              
              // Remove existing proof
              const existing = document.getElementById('cross-env-event-proof');
              if (existing) existing.remove();
              
              document.body.appendChild(proofElement);
            };
            
            // Listen to DOM events (how widgets actually work)
            document.addEventListener('chat:message-received', listener);
            
            // Initialize counter
            window.crossEnvEventCount = eventCount;
            
            console.log('✅ DOM event listener registered for cross-environment test');
            
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
    throw error;
  } finally {
    // Clean up client connection to prevent hanging
    if (client?.disconnect) {
      try {
        console.log('🔌 Cleaning up client connection...');
        await client.disconnect();
        console.log('✅ Client disconnected');
      } catch (disconnectError) {
        console.warn('⚠️ Disconnect warning:', disconnectError);
      }
    }
    process.exit(0);
  }
}

testCrossEnvironmentEvents();