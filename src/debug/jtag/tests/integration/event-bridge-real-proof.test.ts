#!/usr/bin/env tsx
/**
 * Real Event Bridge Cross-Environment Proof Test
 * 
 * Creates actual proof that events bridge from server to browser by:
 * 1. Setting up browser event listener via DOM manipulation
 * 2. Sending server event via chat command
 * 3. Verifying browser receives event via DOM changes
 */

import { jtag } from '../../server-index';

async function testRealEventBridgeProof() {
  console.log('🧪 INTEGRATION TEST: Real cross-environment event bridge proof...');
  
  try {
    // Connect to running JTAG system
    console.log('🔌 Connecting to JTAG system...');
    const client = await jtag.connect({ 
      targetEnvironment: 'server'
    });
    
    console.log('✅ Connected! Setting up real event bridge proof...');
    
    // Step 1: Setup browser-side proof mechanism using DOM events
    console.log('🌐 Step 1: Setting up browser DOM-based event proof...');
    const setupResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (async function() {
            console.log('🌐 BROWSER: Setting up DOM-based event bridge proof...');
            
            try {
              // Create proof counter element
              let proofElement = document.getElementById('event-bridge-counter');
              if (!proofElement) {
                proofElement = document.createElement('div');
                proofElement.id = 'event-bridge-counter';
                proofElement.textContent = 'Events received: 0';
                proofElement.style.cssText = \`
                  position: fixed;
                  top: 10px;
                  left: 10px;
                  background: #ff0000;
                  color: white;
                  padding: 10px;
                  border-radius: 4px;
                  z-index: 10000;
                  font-family: monospace;
                \`;
                document.body.appendChild(proofElement);
              }
              
              // Setup DOM event listener for custom events
              let eventCount = 0;
              window.eventBridgeProofListener = function(event) {
                eventCount++;
                console.log('📨 BROWSER: Received bridged event!', event.detail);
                proofElement.textContent = \`Events received: \${eventCount}\`;
                proofElement.style.background = '#00ff00'; // Turn green on success
              };
              
              // Listen for custom DOM events dispatched by EventsDaemon
              document.addEventListener('chat-message-sent', window.eventBridgeProofListener);
              
              console.log('✅ BROWSER: DOM event bridge proof listener ready');
              return { 
                success: true, 
                listenerSetup: true,
                proof: 'BROWSER_EVENT_LISTENER_READY'
              };
              
            } catch (error) {
              console.error('❌ BROWSER: Event bridge setup failed:', error);
              return { success: false, error: String(error) };
            }
          })();
        `
      }
    });
    
    console.log('📊 Browser setup result:', setupResult);
    
    if (!setupResult.success) {
      throw new Error('Failed to setup browser event listener');
    }
    
    // Step 2: Send chat message from server (this should emit event that bridges to browser)
    console.log('💬 Step 2: Sending server chat message (should bridge to browser)...');
    const chatResult = await client.commands['chat/send-message']({
      context: { uuid: 'bridge-proof', environment: 'server' },
      // Use shared session instead of hardcoded string
      roomId: 'bridge-proof-room',
      content: 'PROOF: This message should trigger cross-environment event',
      category: 'chat'
    });
    
    console.log('📊 Chat result:', chatResult);
    
    // Step 3: Wait for event delivery across environments
    console.log('⏳ Step 3: Waiting for cross-environment event delivery...');
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Step 4: Check browser proof element to see if event was received
    console.log('🔍 Step 4: Checking browser for cross-environment event proof...');
    const proofResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (async function() {
            console.log('🔍 BROWSER: Checking event bridge proof...');
            
            const proofElement = document.getElementById('event-bridge-counter');
            if (!proofElement) {
              return { success: false, error: 'Proof element not found' };
            }
            
            const proofText = proofElement.textContent;
            const isGreen = proofElement.style.background === 'rgb(0, 255, 0)' || proofElement.style.background === '#00ff00';
            
            console.log('📊 BROWSER: Proof element text:', proofText);
            console.log('📊 BROWSER: Proof element color:', proofElement.style.background);
            
            // Check if event was received (counter should be > 0 and element should be green)
            const eventReceived = proofText.includes('Events received:') && 
                                 !proofText.includes('Events received: 0') && 
                                 isGreen;
            
            return {
              success: eventReceived,
              proofText: proofText,
              colorChanged: isGreen,
              eventBridgeWorking: eventReceived
            };
          })();
        `
      }
    });
    
    console.log('📊 Browser proof check result:', proofResult);
    
    // Step 5: Take screenshot as visual proof
    console.log('📸 Step 5: Taking screenshot as visual proof...');
    const screenshotResult = await client.commands.screenshot({
      filename: 'cross-environment-event-proof.png'
    });
    
    // Evaluate proof
    if (proofResult.success && 
        proofResult.commandResult?.result?.eventBridgeWorking) {
      console.log('✅ INTEGRATION TEST PASSED: Cross-environment event bridge PROVEN!');
      console.log('📨 Server→Browser event delivery confirmed via DOM proof');
      console.log('🌉 EventsDaemon successfully bridges events across environments');
      return true;
    } else {
      console.log('❌ INTEGRATION TEST FAILED: No cross-environment event evidence found');
      console.log('🔍 Browser proof element status:', proofResult.commandResult?.result);
      return false;
    }
    
  } catch (error) {
    console.error('❌ INTEGRATION TEST FAILED:', error);
    throw error;
  }
}

// Run test
testRealEventBridgeProof().then((success) => {
  if (success) {
    console.log('🎉 Real cross-environment event bridge proof completed!');
    process.exit(0);
  } else {
    console.log('💥 Cross-environment event bridge not working!');
    process.exit(1);
  }
}).catch((error) => {
  console.error('💥 Test failed:', error);
  process.exit(1);
});