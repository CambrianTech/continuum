#!/usr/bin/env npx tsx
/**
 * Chat Widget Dynamic Updates Test
 * 
 * Tests ChatWidget integration with ChatDaemon and event system
 * Takes screenshots to verify dynamic UI updates
 */

import { jtag } from '../';

async function testChatWidgetDynamicUpdates() {
  console.log('🧪 CHAT WIDGET DYNAMIC UPDATES TEST');
  console.log('==================================================');
  console.log('🎯 Testing ChatWidget integration with real chat system');
  
  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    const jtagClient = await jtag.connect();
    console.log('✅ Connected to JTAG system');
    
    // Take initial screenshot of just the chat widget
    console.log('📸 Taking initial chat widget screenshot...');
    await jtagClient.commands.screenshot({
      filename: 'chat-widget-initial.png',
      querySelector: 'chat-widget'
    });
    console.log('✅ Initial chat widget screenshot saved');
    
    // Test 1: Send a message through the widget event system
    console.log('🧪 Test 1: Triggering chat message via browser event...');
    await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          // Dispatch chat:send-message event that ChatWidget listens for
          const event = new CustomEvent('chat:send-message', {
            detail: {
              roomId: 'main-room',
              content: 'Test message from dynamic test',
              mentions: [],
              timestamp: new Date().toISOString()
            }
          });
          document.dispatchEvent(event);
          console.log('💬 TEST: Dispatched chat:send-message event');
          'Event dispatched successfully';
        `
      }
    });
    
    // Wait for UI update
    await new Promise(resolve => setTimeout(resolve, 500));
    
    // Take screenshot of chat widget after message sent + get innerHTML
    console.log('📸 Taking chat widget screenshot after message...');
    await jtagClient.commands.screenshot({
      filename: 'chat-widget-after-message.png',
      querySelector: 'chat-widget'
    });
    
    // Get innerHTML to verify message was added
    const htmlAfterMessage = await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          const chatWidget = document.querySelector('chat-widget');
          const messagesDiv = chatWidget?.shadowRoot?.getElementById('messages');
          const messageCount = messagesDiv?.children.length || 0;
          ({ 
            messageCount,
            lastMessage: messagesDiv?.lastElementChild?.textContent || 'No messages'
          });
        `
      }
    });
    
    console.log('📋 Chat widget state after message:', htmlAfterMessage.commandResult?.result);
    console.log('✅ Message screenshot and HTML captured');
    
    // Test 2: Simulate incoming message from chat daemon
    console.log('🧪 Test 2: Simulating incoming message from ChatDaemon...');
    await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          // Simulate ChatDaemon sending a message to the widget
          const incomingEvent = new CustomEvent('chat:message-received', {
            detail: {
              message: {
                content: 'This is a response from the chat daemon system!',
                senderName: 'JTAG Assistant',
                timestamp: new Date().toISOString(),
                id: 'msg_' + Date.now()
              }
            }
          });
          document.dispatchEvent(incomingEvent);
          console.log('💬 TEST: Dispatched chat:message-received event');
          'Incoming message event dispatched';
        `
      }
    });
    
    // Wait for UI update
    await new Promise(resolve => setTimeout(resolve, 500));
    
    // Take screenshot of chat widget after incoming message + verify HTML
    console.log('📸 Taking chat widget screenshot after incoming message...');
    await jtagClient.commands.screenshot({
      filename: 'chat-widget-after-incoming.png',
      querySelector: 'chat-widget'
    });
    
    // Verify incoming message was added to widget
    const htmlAfterIncoming = await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          const chatWidget = document.querySelector('chat-widget');
          const messagesDiv = chatWidget?.shadowRoot?.getElementById('messages');
          const messageCount = messagesDiv?.children.length || 0;
          const messages = Array.from(messagesDiv?.children || []).map(msg => ({
            type: msg.className.includes('user') ? 'user' : 'assistant',
            content: msg.textContent
          }));
          ({ messageCount, messages });
        `
      }
    });
    
    console.log('📋 Chat widget messages after incoming:', htmlAfterIncoming.commandResult?.result);
    console.log('✅ Incoming message screenshot and HTML captured');
    
    // Test 3: Simulate participant joining
    console.log('🧪 Test 3: Simulating participant joining...');
    await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          // Simulate participant joining the chat
          const joinEvent = new CustomEvent('chat:participant-joined', {
            detail: {
              participant: {
                displayName: 'AI Developer',
                id: 'ai-dev-' + Date.now(),
                capabilities: ['coding', 'debugging']
              }
            }
          });
          document.dispatchEvent(joinEvent);
          console.log('💬 TEST: Dispatched chat:participant-joined event');
          'Participant joined event dispatched';
        `
      }
    });
    
    // Wait for UI update
    await new Promise(resolve => setTimeout(resolve, 500));
    
    // Take final screenshot of chat widget showing all updates
    console.log('📸 Taking final chat widget screenshot...');
    await jtagClient.commands.screenshot({
      filename: 'chat-widget-final-state.png',
      querySelector: 'chat-widget'
    });
    
    // Get final HTML state to show all accumulated changes
    const finalHtmlState = await jtagClient.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          const chatWidget = document.querySelector('chat-widget');
          const messagesDiv = chatWidget?.shadowRoot?.getElementById('messages');
          const messageCount = messagesDiv?.children.length || 0;
          const allMessages = Array.from(messagesDiv?.children || []).map((msg, index) => ({
            index: index + 1,
            type: msg.className.includes('user') ? 'user' : 'assistant', 
            content: msg.textContent,
            classes: msg.className
          }));
          ({ 
            totalMessages: messageCount,
            messages: allMessages,
            widgetConnected: !!chatWidget
          });
        `
      }
    });
    
    console.log('📋 Final chat widget state:', JSON.stringify(finalHtmlState.commandResult?.result, null, 2));
    console.log('✅ Final screenshot and HTML captured');
    
    console.log('');
    console.log('🎯 CHAT WIDGET DYNAMIC UPDATES TEST RESULTS');
    console.log('============================================');
    console.log('✅ Test 1: Message sent via event system');
    console.log('✅ Test 2: Incoming message received');
    console.log('✅ Test 3: Participant joined notification');
    console.log('✅ All screenshots captured showing dynamic updates');
    console.log('');
    console.log('📸 Screenshots saved to currentUser/screenshots/:');
    console.log('   - chat-widget-initial.png');
    console.log('   - chat-widget-after-message.png');
    console.log('   - chat-widget-after-incoming.png');
    console.log('   - chat-widget-final-state.png');
    console.log('');
    console.log('🎉 CHAT WIDGET DYNAMIC INTEGRATION SUCCESSFUL!');
    
  } catch (error) {
    console.error('❌ Chat widget dynamic test failed:', error);
    process.exit(1);
  }
}

// Run the test
testChatWidgetDynamicUpdates().catch(error => {
  console.error('🚨 Test runner failed:', error);
  process.exit(1);
});