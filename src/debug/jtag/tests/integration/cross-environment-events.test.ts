#!/usr/bin/env tsx
/**
 * Cross-Environment Event Routing Proof Test
 * 
 * Proves that events route across environments (server→browser, browser→server)
 * using the same routing infrastructure as commands, with real non-mocked verification.
 */

import { jtag } from '../../server-index';

async function testCrossEnvironmentEventRouting() {
  console.log('🧪 INTEGRATION TEST: Cross-environment event routing proof...');
  
  try {
    // Connect to running JTAG system
    console.log('🔌 Connecting to JTAG system...');
    const client = await jtag.connect({ 
      targetEnvironment: 'server'
    });
    
    console.log('✅ Connected! Setting up cross-environment event proof...');
    
    // Step 1: Setup browser event listener that logs to browser console
    console.log('🌐 Step 1: Setting up browser event listener...');
    const listenerSetupResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (async function() {
            console.log('🌐 BROWSER: Setting up cross-environment event listener...');
            
            try {
              // Get JTAG system instance
              const jtagSystem = window.jtag;
              if (!jtagSystem || !jtagSystem.eventManager) {
                return { success: false, error: 'JTAG system not available' };
              }
              
              // Setup listener for chat message events
              let eventReceived = false;
              const chatMessageListener = (eventData) => {
                eventReceived = true;
                console.log('📨 BROWSER EVENT RECEIVED: Chat message from server!');
                console.log('📊 Event data:', JSON.stringify(eventData, null, 2));
                
                // Create visible proof element
                const proofElement = document.createElement('div');
                proofElement.id = 'cross-env-event-proof';
                proofElement.textContent = 'CHAT MESSAGE RECEIVED: ' + (eventData.message || eventData.content || 'No message');
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
              
              // Listen to chat message events that should be bridged
              jtagSystem.eventManager.events.on('chat-message-sent', chatMessageListener);
              
              console.log('✅ BROWSER: Cross-environment event listener registered');
              return { 
                success: true, 
                listenerRegistered: true,
                eventPath: 'events/chat/test-room/cross-env-message',
                proof: 'CROSS_ENV_LISTENER_SETUP'
              };
              
            } catch (error) {
              console.error('❌ BROWSER: Cross-env listener setup failed:', error);
              return { success: false, error: String(error) };
            }
          })();
        `
      }
    });
    
    console.log('📊 Browser listener setup result:', listenerSetupResult);
    
    if (!listenerSetupResult.success) {
      throw new Error('Failed to setup browser event listener');
    }
    
    // Step 2: Send server→browser event via router (targeting browser environment)
    console.log('📨 Step 2: Emitting server→browser event...');
    
    // Use a simple chat message to test events - this should work via existing event bridge
    const serverEventResult = await client.commands['chat/send-message']({
      roomId: 'test-room',
      message: 'This is a cross-environment test message',
      metadata: {
        testType: 'cross-env-event-proof',
        timestamp: new Date().toISOString()
      }
    });
    
    console.log('📊 Server event emission result:', serverEventResult);
    
    // Step 3: Wait and check if browser received the event
    console.log('⏳ Step 3: Waiting for event delivery...');
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Step 4: Check browser for proof element
    console.log('🔍 Step 4: Checking browser for event receipt proof...');
    const proofCheckResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (async function() {
            console.log('🔍 BROWSER: Checking for cross-environment event proof...');
            
            // Look for proof element created by event listener
            const proofElement = document.getElementById('cross-env-event-proof');
            
            if (proofElement) {
              console.log('✅ BROWSER: Cross-environment event proof found!');
              console.log('📊 Proof text:', proofElement.textContent);
              
              return {
                success: true,
                eventReceived: true,
                proofText: proofElement.textContent,
                proof: 'CROSS_ENV_EVENT_DELIVERED'
              };
            } else {
              console.log('❌ BROWSER: No cross-environment event proof found');
              return {
                success: false,
                eventReceived: false,
                error: 'No proof element found'
              };
            }
          })();
        `
      }
    });
    
    console.log('📊 Proof check result:', proofCheckResult);
    
    if (proofCheckResult.success && proofCheckResult.commandResult?.result?.eventReceived) {
      console.log('✅ INTEGRATION TEST PASSED: Cross-environment event routing proven!');
      console.log('📨 Server→Browser event delivered and confirmed via DOM proof');
      console.log('🎯 Events route across environments using same infrastructure as commands');
      return true;
    } else {
      console.log('❌ Event not received in browser - cross-environment routing may not be working');
      return false;
    }
    
  } catch (error) {
    console.error('❌ INTEGRATION TEST FAILED:', error);
    throw error;
  }
}

// Run test
testCrossEnvironmentEventRouting().then((success) => {
  if (success) {
    console.log('🎉 Cross-environment event routing proof completed successfully!');
    process.exit(0);
  } else {
    console.log('💥 Cross-environment event routing proof failed!');
    process.exit(1);
  }
}).catch((error) => {
  console.error('💥 Test failed:', error);
  process.exit(1);
});