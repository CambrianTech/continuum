/**
 * Integrated Chat Widget Test
 * 
 * Tests the complete chat widget flow:
 * 1. Take before screenshot
 * 2. Send message via exec command (no infinite loops)
 * 3. Take after screenshot showing message
 * 4. Verify both exec commands work and actual message appears
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../../system/core/client/shared/JTAGClient';

async function runIntegratedChatTest(): Promise<void> {
  console.log('💬 INTEGRATED CHAT WIDGET TEST - Before/after with actual message sending');
  
  try {
    // Connect to JTAG system
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server',
      transportType: 'websocket', 
      serverUrl: 'ws://localhost:9001',
      enableFallback: false
    };
    
    console.log('🔗 Connecting to JTAG system for chat widget test...');
    const { client } = await JTAGClientServer.connect(clientOptions);
    console.log('✅ JTAG Client connected for chat widget integration test');
    
    // Step 1: Take BEFORE screenshot
    console.log('📸 Step 1: Taking BEFORE screenshot of chat widget...');
    const beforeResult = await (client as any).commands.screenshot({
      filename: 'chat-widget-before-integrated.png'
    });
    console.log('✅ Step 1: BEFORE screenshot captured');
    
    // Step 2: Send message to chat widget via exec command (no infinite loops)
    console.log('💬 Step 2: Sending message to chat widget via exec command...');
    const chatResult = await (client as any).commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          (async () => {
            console.log('💬 INTEGRATED TEST: Sending message to chat widget');
            
            try {
              // Find chat widget in DOM
              const chatWidget = document.querySelector('chat-widget');
              if (!chatWidget) {
                console.log('❌ INTEGRATED TEST: Chat widget not found in DOM');
                return { success: false, error: 'Chat widget not found' };
              }
              
              // Find input and button inside shadow DOM
              const shadowRoot = chatWidget.shadowRoot;
              if (!shadowRoot) {
                console.log('❌ INTEGRATED TEST: Shadow root not found');
                return { success: false, error: 'Shadow root not found' };
              }
              
              const input = shadowRoot.querySelector('input[type="text"]');
              const button = shadowRoot.querySelector('button');
              
              if (!input || !button) {
                console.log('❌ INTEGRATED TEST: Input or button not found in chat widget');
                return { success: false, error: 'Chat widget controls not found' };
              }
              
              // Send message
              console.log('💬 INTEGRATED TEST: Setting input value and clicking send...');
              input.value = 'INTEGRATED TEST MESSAGE: Exec commands fixed - no more infinite loops!';
              button.click();
              
              // Wait a moment for message to appear
              await new Promise(resolve => setTimeout(resolve, 100));
              
              console.log('✅ INTEGRATED TEST: Message sent to chat widget successfully');
              return { 
                success: true, 
                message: 'INTEGRATED TEST MESSAGE: Exec commands fixed - no more infinite loops!',
                proof: 'CHAT_MESSAGE_SENT_VIA_EXEC'
              };
            } catch (error) {
              console.log('❌ INTEGRATED TEST: Chat message sending failed:', error);
              return { success: false, error: error.message || String(error) };
            }
          })();
        `
      }
    });
    
    console.log('📊 Step 2: Chat message result:', chatResult.success ? 'SUCCESS' : 'FAILED');
    
    // Step 3: Take AFTER screenshot
    console.log('📸 Step 3: Taking AFTER screenshot to show message...');
    const afterResult = await (client as any).commands.screenshot({
      filename: 'chat-widget-after-integrated.png'
    });
    console.log('✅ Step 3: AFTER screenshot captured');
    
    // Step 4: Verify both screenshots exist and have content
    console.log('🔍 Step 4: Verifying test results...');
    
    const verificationResult = await (client as any).commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          (async () => {
            // Check if chat widget has messages
            const chatWidget = document.querySelector('chat-widget');
            if (!chatWidget || !chatWidget.shadowRoot) {
              return { success: false, error: 'Chat widget or shadow root not found' };
            }
            
            const messages = chatWidget.shadowRoot.querySelectorAll('.message, .chat-message, div[class*="message"]');
            console.log('💬 VERIFICATION: Found ' + messages.length + ' messages in chat widget');
            
            // Look for our test message
            let foundTestMessage = false;
            messages.forEach((msg, index) => {
              console.log('💬 Message ' + (index + 1) + ':', msg.textContent);
              if (msg.textContent && msg.textContent.includes('INTEGRATED TEST MESSAGE')) {
                foundTestMessage = true;
              }
            });
            
            return {
              success: foundTestMessage,
              messageCount: messages.length,
              foundTestMessage: foundTestMessage,
              proof: foundTestMessage ? 'TEST_MESSAGE_VISIBLE_IN_WIDGET' : 'TEST_MESSAGE_NOT_FOUND'
            };
          })();
        `
      }
    });
    
    // Graceful disconnect
    try {
      if (client && typeof (client as any).disconnect === 'function') {
        await (client as any).disconnect();
      }
    } catch (disconnectError) {
      // Ignore disconnect errors
    }
    
    // Final Results
    console.log('');
    console.log('🎯 ============= INTEGRATED CHAT WIDGET TEST RESULTS =============');
    console.log('📸 Before screenshot:', beforeResult.success ? '✅ SUCCESS' : '❌ FAILED');
    console.log('💬 Chat message send:', chatResult.success ? '✅ SUCCESS' : '❌ FAILED'); 
    console.log('📸 After screenshot:', afterResult.success ? '✅ SUCCESS' : '❌ FAILED');
    console.log('🔍 Message verification:', verificationResult.success ? '✅ SUCCESS' : '❌ FAILED');
    console.log('');
    
    if (beforeResult.success && chatResult.success && afterResult.success && verificationResult.success) {
      console.log('🎉 INTEGRATED CHAT WIDGET TEST: COMPLETE SUCCESS!');
      console.log('✅ Exec commands work without infinite loops');
      console.log('✅ Chat widget receives and displays messages');  
      console.log('✅ Screenshots capture before/after states');
      console.log('📁 Check screenshots: examples/test-bench/.continuum/jtag/currentUser/screenshots/');
      process.exit(0);
    } else {
      console.log('❌ INTEGRATED CHAT WIDGET TEST: Some steps failed');
      console.log('🔍 Check individual step results above');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('💥 INTEGRATED CHAT TEST ERROR:', error);
    process.exit(1);
  }
}

// Run the integrated chat test
runIntegratedChatTest().catch(error => {
  console.error('💥 Integrated chat test runner error:', error);
  process.exit(1);
});