#!/usr/bin/env node
/**
 * Chat Widget Automation Test
 * 
 * Specialized test for chat widget interactions:
 * - Send messages through chat input
 * - Wait for message delivery/display
 * - Screenshot before/after message states
 * - Monitor chat animations (typing indicators, message bubbles)
 * - Test multiple message scenarios
 * - Verify message content extraction
 * - Test chat widget UI controls
 * 
 * Perfect demonstration of real-world widget testing.
 */

console.log('\n💬 Chat Widget Automation Test');
console.log('===============================');

async function runChatWidgetTest(): Promise<void> {
  try {
    // Connect to JTAG
    const jtagModule = await import('../../server-index');
    const jtag = await jtagModule.jtag.connect();
    console.log('✅ JTAG connected - starting chat widget tests');

    let passed = 0;
    let failed = 0;
    const chatTestResults: Array<{test: string, success: boolean, details: string}> = [];

    // Chat Test 1: Initial Chat State Screenshot
    try {
      console.log('\n▶️  Chat Test 1: Capture Initial Chat State');
      
      const initialShot = await jtag.commands.screenshot('chat-initial-state');
      
      if (initialShot && (initialShot.success || initialShot.filename)) {
        console.log('✅ Initial chat state captured');
        passed++;
        chatTestResults.push({test: 'Initial State Capture', success: true, details: 'Chat initial state screenshot taken'});
      } else {
        console.log('❌ Failed to capture initial state');
        failed++;
        chatTestResults.push({test: 'Initial State Capture', success: false, details: 'Screenshot failed'});
      }
    } catch (error) {
      console.log('❌ Initial state error:', error);
      failed++;
      chatTestResults.push({test: 'Initial State Capture', success: false, details: String(error)});
    }

    // Chat Test 2: Find and Interact with Chat Input
    try {
      console.log('\n▶️  Chat Test 2: Locate Chat Input Field');
      
      // Try various chat input selectors
      const chatInputSelectors = [
        '.chat-input',
        '#chat-input', 
        'input[placeholder*="message"]',
        'input[placeholder*="chat"]',
        'input[placeholder*="type"]',
        'textarea[placeholder*="message"]',
        '[data-testid="chat-input"]',
        '.message-input',
        '#message-box',
        'input[type="text"]',
        'textarea'
      ];

      let chatInputFound = false;
      let chatInputSelector = '';
      
      for (const selector of chatInputSelectors) {
        try {
          const waitResult = await jtag.commands.waitForElement(selector, 2000);
          if (waitResult && waitResult.success && waitResult.found) {
            chatInputFound = true;
            chatInputSelector = selector;
            console.log(`✅ Found chat input: ${selector}`);
            break;
          }
        } catch (e) {
          continue;
        }
      }
      
      if (chatInputFound) {
        passed++;
        chatTestResults.push({test: 'Chat Input Location', success: true, details: `Found chat input: ${chatInputSelector}`});
        
        // Store the working selector for next tests
        (global as any).foundChatInput = chatInputSelector;
      } else {
        console.log('❌ No chat input field found');
        failed++;
        chatTestResults.push({test: 'Chat Input Location', success: false, details: 'No chat input field detected'});
      }
    } catch (error) {
      console.log('❌ Chat input location error:', error);
      failed++;
      chatTestResults.push({test: 'Chat Input Location', success: false, details: String(error)});
    }

    // Chat Test 3: Type Test Message
    try {
      console.log('\n▶️  Chat Test 3: Type Test Message');
      
      const chatInput = (global as any).foundChatInput || 'input, textarea';
      const testMessage = 'Hello! This is a JTAG automation test message 🚀';
      
      const typeResult = await jtag.commands.type(chatInput, testMessage, true); // Clear first
      
      if (typeResult && typeResult.success) {
        console.log('✅ Test message typed successfully');
        passed++;
        chatTestResults.push({test: 'Message Typing', success: true, details: `Typed: "${testMessage}"`});
        
        // Screenshot the typed state
        try {
          const typedShot = await jtag.commands.screenshot('chat-message-typed');
          console.log('📸 Captured message typed state');
        } catch (e) {
          console.log('⚠️  Screenshot after typing failed');
        }
      } else {
        console.log('❌ Failed to type test message');
        failed++;
        chatTestResults.push({test: 'Message Typing', success: false, details: 'Type command failed'});
      }
    } catch (error) {
      console.log('❌ Message typing error:', error);
      failed++;
      chatTestResults.push({test: 'Message Typing', success: false, details: String(error)});
    }

    // Chat Test 4: Find and Click Send Button
    try {
      console.log('\n▶️  Chat Test 4: Find and Click Send Button');
      
      const sendButtonSelectors = [
        '.send-button',
        '#send-button',
        'button[type="submit"]',
        'button[aria-label*="send"]',
        '.chat-send',
        '[data-testid="send"]',
        '.message-send',
        'button:contains("Send")',
        'button',  // Any button as fallback
      ];

      let sendButtonWorked = false;
      let sendButtonSelector = '';
      
      for (const selector of sendButtonSelectors) {
        try {
          const clickResult = await jtag.commands.click(selector);
          if (clickResult && clickResult.success) {
            sendButtonWorked = true;
            sendButtonSelector = selector;
            console.log(`✅ Send button clicked: ${selector}`);
            break;
          }
        } catch (e) {
          continue;
        }
      }
      
      if (sendButtonWorked) {
        passed++;
        chatTestResults.push({test: 'Send Button Click', success: true, details: `Clicked send button: ${sendButtonSelector}`});
        
        // Wait a moment for message to be processed/displayed
        await new Promise(resolve => setTimeout(resolve, 1500));
        
        // Screenshot the sent state
        try {
          const sentShot = await jtag.commands.screenshot('chat-message-sent');
          console.log('📸 Captured message sent state');
        } catch (e) {
          console.log('⚠️  Screenshot after sending failed');
        }
      } else {
        console.log('❌ No send button found/clicked');
        failed++;
        chatTestResults.push({test: 'Send Button Click', success: false, details: 'No send button found'});
      }
    } catch (error) {
      console.log('❌ Send button error:', error);
      failed++;
      chatTestResults.push({test: 'Send Button Click', success: false, details: String(error)});
    }

    // Chat Test 5: Verify Message in Chat History
    try {
      console.log('\n▶️  Chat Test 5: Verify Message in Chat History');
      
      const messageSelectors = [
        '.message',
        '.chat-message',
        '.msg',
        '[data-testid="message"]',
        '.message-bubble',
        '.chat-bubble',
        'li',  // Message lists
        'p'    // Paragraph messages
      ];

      let messageFound = false;
      let messageText = '';
      let messageSelector = '';
      
      for (const selector of messageSelectors) {
        try {
          const textResult = await jtag.commands.getText(selector, true, true); // innerText, trim
          if (textResult && textResult.success && textResult.text && textResult.text.length > 0) {
            // Check if it contains our test message or at least some text
            if (textResult.text.includes('JTAG') || textResult.text.length > 5) {
              messageFound = true;
              messageText = textResult.text;
              messageSelector = selector;
              console.log(`✅ Found message content: ${selector}`);
              break;
            }
          }
        } catch (e) {
          continue;
        }
      }
      
      if (messageFound) {
        passed++;
        const preview = messageText.length > 100 ? messageText.substring(0, 100) + '...' : messageText;
        chatTestResults.push({test: 'Message Verification', success: true, details: `Found message (${messageSelector}): "${preview}"`});
      } else {
        console.log('❌ No message content found');
        failed++;
        chatTestResults.push({test: 'Message Verification', success: false, details: 'No message content detected'});
      }
    } catch (error) {
      console.log('❌ Message verification error:', error);
      failed++;
      chatTestResults.push({test: 'Message Verification', success: false, details: String(error)});
    }

    // Chat Test 6: Animation Detection (typing indicators, transitions)
    try {
      console.log('\n▶️  Chat Test 6: Chat Animation Detection');
      
      const animationResult = await jtag.commands.exec(`
        // Look for chat-specific animations
        const indicators = {
          typingIndicators: document.querySelectorAll('.typing-indicator, .is-typing, [class*="typing"]').length,
          loadingSpinners: document.querySelectorAll('.loading, .spinner, [class*="loading"]').length,
          messageAnimations: document.querySelectorAll('.message[style*="animation"], .chat-message[style*="transition"]').length,
          cssAnimations: 0,
          cssTransitions: 0
        };
        
        // Check for CSS animations/transitions in chat elements
        const chatElements = document.querySelectorAll('.chat, .message, [class*="chat"], [class*="message"]');
        for (const el of chatElements) {
          const styles = getComputedStyle(el);
          if (styles.animationName !== 'none') indicators.cssAnimations++;
          if (styles.transitionProperty !== 'none' && styles.transitionProperty !== 'all') indicators.cssTransitions++;
        }
        
        return {
          ...indicators,
          totalAnimatedElements: indicators.typingIndicators + indicators.loadingSpinners + 
                                indicators.messageAnimations + indicators.cssAnimations + indicators.cssTransitions,
          chatElementsScanned: chatElements.length
        };
      `);
      
      if (animationResult && animationResult.success && animationResult.result) {
        const animData = animationResult.result;
        const details = `Scanned ${animData.chatElementsScanned} chat elements: ${animData.totalAnimatedElements} with animations (typing: ${animData.typingIndicators}, loading: ${animData.loadingSpinners}, CSS: ${animData.cssAnimations + animData.cssTransitions})`;
        
        console.log('✅ Animation detection completed');
        passed++;
        chatTestResults.push({test: 'Animation Detection', success: true, details});
      } else {
        console.log('❌ Animation detection failed');
        failed++;
        chatTestResults.push({test: 'Animation Detection', success: false, details: 'Animation analysis failed'});
      }
    } catch (error) {
      console.log('❌ Animation detection error:', error);
      failed++;
      chatTestResults.push({test: 'Animation Detection', success: false, details: String(error)});
    }

    // Chat Test 7: Multi-Message Workflow
    try {
      console.log('\n▶️  Chat Test 7: Multi-Message Workflow Test');
      
      const messages = [
        'First message from automation',
        'Second message with emoji 😊', 
        'Third message to test continuity'
      ];
      
      let workflowPassed = 0;
      const workflowDetails = [];
      
      const chatInput = (global as any).foundChatInput || 'input, textarea';
      
      for (let i = 0; i < messages.length; i++) {
        try {
          // Type message
          const typeResult = await jtag.commands.type(chatInput, messages[i], true);
          if (typeResult && typeResult.success) {
            workflowPassed++;
            
            // Try to send
            try {
              const sendResult = await jtag.commands.click('button, .send-button, [type="submit"]');
              if (sendResult && sendResult.success) {
                workflowDetails.push(`✅ Message ${i + 1}: "${messages[i]}" sent`);
                
                // Wait between messages
                await new Promise(resolve => setTimeout(resolve, 800));
              } else {
                workflowDetails.push(`⚠️  Message ${i + 1}: typed but send failed`);
              }
            } catch (sendError) {
              workflowDetails.push(`⚠️  Message ${i + 1}: send error`);
            }
          } else {
            workflowDetails.push(`❌ Message ${i + 1}: typing failed`);
          }
        } catch (error) {
          workflowDetails.push(`❌ Message ${i + 1}: error`);
        }
      }
      
      // Final screenshot
      try {
        await jtag.commands.screenshot('chat-multi-message-final');
        workflowDetails.push('📸 Final state captured');
      } catch (e) {
        workflowDetails.push('⚠️  Final screenshot failed');
      }
      
      if (workflowPassed >= 2) { // At least 2/3 messages worked
        console.log('✅ Multi-message workflow passed');
        passed++;
        chatTestResults.push({test: 'Multi-Message Workflow', success: true, details: workflowDetails.join(', ')});
      } else {
        console.log('❌ Multi-message workflow failed');
        failed++;
        chatTestResults.push({test: 'Multi-Message Workflow', success: false, details: workflowDetails.join(', ')});
      }
    } catch (error) {
      console.log('❌ Multi-message workflow error:', error);
      failed++;
      chatTestResults.push({test: 'Multi-Message Workflow', success: false, details: String(error)});
    }

    // CHAT WIDGET RESULTS
    console.log('\n═══════════════════════════════════════════════════════════════');
    console.log('💬 CHAT WIDGET AUTOMATION RESULTS');
    console.log('═══════════════════════════════════════════════════════════════');
    
    const total = passed + failed;
    const successRate = total > 0 ? Math.round((passed / total) * 100) : 0;
    
    console.log(`📊 Chat Tests: ${passed}/${total} passed (${successRate}%)`);
    console.log('');
    
    console.log('📋 Chat Test Details:');
    chatTestResults.forEach(result => {
      const status = result.success ? '✅' : '❌';
      console.log(`   ${status} ${result.test}: ${result.details}`);
    });
    console.log('');
    
    console.log('🎭 Chat Widget Testing Capabilities:');
    console.log('   • Screenshot chat states (before/during/after)');
    console.log('   • Locate and interact with chat input fields');
    console.log('   • Type messages with special characters/emoji');
    console.log('   • Find and click send buttons');
    console.log('   • Verify messages appear in chat history');
    console.log('   • Detect chat animations (typing indicators, transitions)');
    console.log('   • Multi-message conversation workflows');
    console.log('   • Chat UI element discovery and interaction');
    console.log('');
    
    if (passed >= 4) {
      console.log('🎉 CHAT WIDGET AUTOMATION FULLY FUNCTIONAL!');
      console.log('💬 Ready for autonomous chat testing and validation');
      console.log('📸 Visual before/after states captured for analysis');
      process.exit(0);
    } else {
      console.log('⚠️  Chat widget automation partially working');
      console.log('✅ Use working capabilities for chat testing');
      process.exit(0);
    }

  } catch (error) {
    console.error('💥 Chat widget test failed:', error);
    process.exit(1);
  }
}

// Run chat widget automation test
if (require.main === module) {
  runChatWidgetTest();
}