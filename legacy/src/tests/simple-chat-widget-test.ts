/**
 * Simple Chat Widget Test - Clean before/after screenshots
 * 
 * Tests chat widget without infinite loops or console spam.
 * Shows visual before/after widget updates.
 */

async function testChatWidget() {
  console.log('🧪 SIMPLE CHAT WIDGET TEST');
  console.log('===========================');
  
  try {
    // Wait for system to be ready
    console.log('⏱️  Waiting for browser to be ready...');
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Check if browser window and JTAG are available
    if (typeof window === 'undefined') {
      console.log('❌ Not running in browser context');
      return;
    }
    
    const jtag = (window as any).jtag;
    if (!jtag) {
      console.log('❌ JTAG system not available');
      return;
    }
    
    console.log('📸 1. Taking BEFORE screenshot...');
    await jtag.commands.screenshot({ 
      selector: 'body',
      filename: 'chat-widget-before.png'
    });
    
    console.log('💬 2. Testing chat widget interaction...');
    
    // Find chat widget
    const chatWidget = document.querySelector('chat-widget');
    if (!chatWidget) {
      console.log('❌ Chat widget not found in DOM');
      return;
    }
    
    // Get input and button from shadow DOM
    const shadowRoot = chatWidget.shadowRoot;
    if (!shadowRoot) {
      console.log('❌ Chat widget shadow DOM not accessible');
      return;
    }
    
    const input = shadowRoot.getElementById('messageInput') as HTMLInputElement;
    const button = shadowRoot.getElementById('sendButton') as HTMLButtonElement;
    
    if (!input || !button) {
      console.log('❌ Chat widget input/button not found');
      return;
    }
    
    // Send ONE test message (no loops!)
    console.log('📝 Sending single test message...');
    input.value = 'Test message - checking for loops';
    button.click();
    
    // Wait for message to appear  
    console.log('⏱️  Waiting for message to process...');
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    console.log('📸 3. Taking AFTER screenshot...');
    await jtag.commands.screenshot({ 
      selector: 'body',
      filename: 'chat-widget-after.png'
    });
    
    console.log('✅ Chat widget test completed successfully!');
    console.log('📷 Check screenshots: chat-widget-before.png, chat-widget-after.png');
    
  } catch (error) {
    console.error('❌ Chat widget test failed:', error);
  }
}

// Run test if in browser context
if (typeof window !== 'undefined') {
  // Wait for page load then run test
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      setTimeout(testChatWidget, 3000); // Extra delay for JTAG system
    });
  } else {
    setTimeout(testChatWidget, 3000);
  }
}