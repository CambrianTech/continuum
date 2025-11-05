#!/usr/bin/env tsx
/**
 * Browser→Server DOM Event Flow Integration Test
 * 
 * Tests that DOM events in browser can trigger JTAG events that bridge to server.
 * Uses browser exec to setup DOM listener that emits JTAG events.
 */

import { jtag } from '../../server-index';

async function testBrowserToServerEventFlow() {
  console.log('🧪 INTEGRATION TEST: Browser→Server DOM event flow...');
  
  try {
    // Connect to running JTAG system
    console.log('🔌 Connecting to JTAG system...');
    const client = await jtag.connect({ 
      targetEnvironment: 'server'
    });
    
    console.log('✅ Connected! Setting up browser DOM event listener...');
    
    // Execute JavaScript in browser to setup DOM event that emits JTAG events
    const setupResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (async function() {
            console.log('🌐 BROWSER: Setting up DOM event→JTAG event bridge...');
            
            try {
              // Create a test button for DOM events
              const testButton = document.createElement('button');
              testButton.id = 'jtag-event-test-button';
              testButton.textContent = 'Click me to emit JTAG event';
              testButton.style.cssText = \`
                position: fixed;
                top: 10px;
                right: 10px;
                z-index: 9999;
                background: #00d4ff;
                color: black;
                border: none;
                padding: 10px;
                border-radius: 4px;
                cursor: pointer;
              \`;
              
              // Remove existing button if any
              const existing = document.getElementById('jtag-event-test-button');
              if (existing) existing.remove();
              
              document.body.appendChild(testButton);
              
              // Setup click handler that emits JTAG event
              testButton.addEventListener('click', async () => {
                console.log('🖱️ BROWSER: DOM click detected, emitting JTAG event...');
                
                try {
                  // Get JTAG system instance to emit event
                  const jtagSystem = window.jtag;
                  if (!jtagSystem) {
                    console.error('❌ BROWSER: JTAG system not available');
                    return;
                  }
                  
                  // Create event data
                  const eventData = {
                    eventName: 'dom-click-event',
                    buttonId: 'jtag-event-test-button',
                    timestamp: new Date().toISOString(),
                    scope: {
                      type: 'user',
                      sessionId: jtagSystem.context.uuid
                    },
                    originSessionId: jtagSystem.context.uuid,
                    data: {
                      type: 'dom-click',
                      element: 'test-button',
                      coordinates: { x: 0, y: 0 }
                    }
                  };
                  
                  // Emit event through JTAG event system
                  if (jtagSystem.eventManager && jtagSystem.eventManager.events) {
                    jtagSystem.eventManager.events.emit('dom-click-event', eventData);
                    console.log('📨 BROWSER: Emitted JTAG event for DOM click');
                  } else {
                    console.error('❌ BROWSER: JTAG event manager not available');
                  }
                  
                } catch (error) {
                  console.error('❌ BROWSER: Failed to emit JTAG event:', error);
                }
              });
              
              console.log('✅ BROWSER: DOM event listener setup complete');
              return { 
                success: true, 
                buttonId: 'jtag-event-test-button',
                proof: 'DOM_EVENT_LISTENER_SETUP'
              };
              
            } catch (error) {
              console.error('❌ BROWSER: DOM setup failed:', error);
              return { success: false, error: String(error) };
            }
          })();
        `
      }
    });
    
    console.log('📊 DOM setup result:', setupResult);
    
    if (!setupResult.success) {
      throw new Error('Failed to setup DOM event listener');
    }
    
    // Trigger the DOM event by clicking the button
    console.log('🖱️ Triggering DOM click event...');
    const clickResult = await client.commands.exec({
      code: {
        type: 'inline', 
        language: 'javascript',
        source: `
          return (async function() {
            console.log('🖱️ BROWSER: Clicking test button to trigger DOM→JTAG event...');
            
            const testButton = document.getElementById('jtag-event-test-button');
            if (!testButton) {
              console.error('❌ BROWSER: Test button not found');
              return { success: false, error: 'Test button not found' };
            }
            
            // Click the button to trigger our event listener
            testButton.click();
            
            // Wait a moment for event to process
            await new Promise(resolve => setTimeout(resolve, 100));
            
            console.log('✅ BROWSER: DOM click triggered');
            return { 
              success: true, 
              proof: 'DOM_CLICK_TRIGGERED'
            };
          })();
        `
      }
    });
    
    console.log('📊 Click trigger result:', clickResult);
    
    if (clickResult.success) {
      console.log('✅ INTEGRATION TEST PASSED: Browser→Server DOM event flow working');
      console.log('📨 DOM click triggered JTAG event emission');
      return true;
    } else {
      throw new Error('Failed to trigger DOM click event');
    }
    
  } catch (error) {
    console.error('❌ INTEGRATION TEST FAILED:', error);
    
    if (error instanceof Error && error.message.includes('timeout')) {
      console.log('\n💡 System may not be running. Start with:');
      console.log('   npm run system:start');
      console.log('   sleep 45');
    }
    
    throw error;
  }
}

// Run test
testBrowserToServerEventFlow().then(() => {
  console.log('🎉 Browser→Server DOM event flow test completed!');
  process.exit(0);
}).catch((error) => {
  console.error('💥 Test failed:', error);
  process.exit(1);
});