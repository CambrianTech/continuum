/**
 * Chat Attribution Test - Automated version of manual testing process
 * 
 * This test replicates the manual steps for testing chat message attribution:
 * 1. Navigate shadow DOM to find chat widget
 * 2. Send message via widget  
 * 3. Verify attribution logic (current user vs other user)
 * 4. Check session ID handling
 */

// Simple test that can run directly with tsx
async function testChatAttribution() {
  console.log('🧪 Testing Chat Attribution...');
  
  try {
    // This is the exact manual workflow turned into a test
    console.log('🧪 Replicating manual test workflow...');
    
    // Step 1: Navigate shadow DOM (same as manual)
    const continuumWidget = document.querySelector('continuum-widget');
    const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
    const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget') as any;
    
    if (!chatWidget || typeof chatWidget.sendMessage !== 'function') {
      throw new Error('Chat widget not found or sendMessage method missing');
    }
    console.log('✅ Step 1: Found chat widget with sendMessage method');
    
    // Step 2: Send test message (same as manual)
    const input = chatWidget.shadowRoot?.querySelector('.message-input') as HTMLInputElement;
    if (!input) {
      throw new Error('Message input not found');
    }
    
    const testMessage = 'automated-attribution-test-' + Date.now();
    input.value = testMessage;
    const result = await chatWidget.sendMessage();
    
    console.log('✅ Step 2: Sent test message programmatically');
    
    // Step 3: Trigger room history reload (same as manual loadRoomHistory call)
    await chatWidget.loadRoomHistory();
    console.log('✅ Step 3: Triggered room history reload');
    
    // Step 4: Analyze attribution (automated version of visual inspection)
    const messages = chatWidget.messages;
    const currentSessionId = chatWidget.currentSessionId;
    
    console.log(`🔧 Analysis: ${messages.length} messages total, currentSessionId: ${currentSessionId}`);
    
    // Find our test message and verify it
    const ourMessage = messages.find((m: any) => m.content === testMessage);
    if (!ourMessage) {
      throw new Error('Test message not found in messages');
    }
    
    const shouldBeUser = currentSessionId && ourMessage.senderId === currentSessionId;
    console.log(`🔧 Our message: senderId=${ourMessage.senderId}, type=${ourMessage.type}, shouldBeUser=${shouldBeUser}`);
    
    // The assertion that would pass if attribution worked correctly
    if (shouldBeUser) {
      console.log('✅ Attribution working correctly - current user message shows as "user"');
      return { success: true, issue: null };
    } else {
      console.log('❌ Attribution broken - investigating session ID mismatch');
      console.log(`   Expected senderId to equal currentSessionId`);
      console.log(`   senderId: "${ourMessage.senderId}"`);
      console.log(`   currentSessionId: "${currentSessionId}"`);
      return { 
        success: false, 
        issue: 'session_id_mismatch',
        details: {
          senderId: ourMessage.senderId,
          currentSessionId: currentSessionId,
          messageType: ourMessage.type
        }
      };
    }
    
  } catch (error) {
    console.error('❌ Test failed:', error);
    return { success: false, issue: 'test_error', error: error.message };
  }
}

// Run if this is the main module
if (typeof window !== 'undefined') {
  // Browser environment - run the test
  testChatAttribution().then(result => {
    console.log('🏁 Test completed:', result);
  });
} else {
  console.log('This test needs to run in a browser environment with the chat widget loaded');
}