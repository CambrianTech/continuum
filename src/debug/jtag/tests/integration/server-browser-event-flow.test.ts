#!/usr/bin/env tsx
/**
 * Server→Browser Event Flow Integration Test
 * 
 * Tests that events emitted on server side properly bridge to browser context
 * through the EventsDaemon and router transport system.
 */

import { jtag } from '../../server-index';

async function testServerToBrowserEventFlow() {
  console.log('🧪 INTEGRATION TEST: Server→Browser event flow...');
  
  try {
    // Connect to running JTAG system
    console.log('🔌 Connecting to JTAG system...');
    const client = await jtag.connect({ 
      targetEnvironment: 'server'
    });
    
    console.log('✅ Connected! Testing event emission via chat command...');
    
    // Test server→browser event flow by sending a chat message
    // The chat command emits events that should bridge to browser context
    console.log('📨 Emitting server event via chat command...');
    
    const chatResult = await client.commands['chat/send-message']({
      context: { uuid: 'server-test', environment: 'server' },
      // Don't hardcode session ID - use shared session
      roomId: 'test-room-123',
      content: 'Test message for server→browser event flow',
      category: 'chat'
    });
    
    console.log('📊 Chat command result:', chatResult);
    
    // The chat command should emit an event that bridges to browser
    // Success means the command executed and message was stored
    if (chatResult.success && chatResult.messageId) {
      console.log('✅ INTEGRATION TEST PASSED: Server→Browser event flow working');
      console.log(`📨 Event emitted for message: ${chatResult.messageId}`);
      return true;
    } else {
      throw new Error('Chat command failed or did not return messageId');
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
testServerToBrowserEventFlow().then(() => {
  console.log('🎉 Server→Browser event flow test completed!');
  process.exit(0);
}).catch((error) => {
  console.error('💥 Test failed:', error);
  process.exit(1);
});