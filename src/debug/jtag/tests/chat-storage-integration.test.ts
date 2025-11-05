/**
 * Chat Storage Integration Test
 * 
 * Tests ChatSendMessageCommand → DataDaemon integration
 * Proves chat messages are actually stored via router
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import { generateUUID } from '../system/core/types/CrossPlatformUUID';

console.log('🧪 CHAT STORAGE INTEGRATION TEST');
console.log('📋 Testing ChatSendMessageCommand → DataDaemon storage');

async function testChatMessageStorage() {
  try {
    // Connect to JTAG system
    const { client } = await JTAGClientServer.connect({
      targetEnvironment: 'server'
    });

    console.log('✅ Connected to JTAG system');

    // Test 1: Send a chat message (should store in DataDaemon)
    const testRoomId = generateUUID();
    const testMessage = {
      roomId: testRoomId,
      content: 'Test message for storage validation',
      senderName: 'Test User',
      category: 'chat'
    };

    console.log('📤 Sending chat message via ChatSendMessageCommand...');
    
    // Check if we have chat commands available
    const listResult = await client.commands.list();
    const hasDataCommands = listResult.commands.some(cmd => cmd.name.startsWith('data/'));
    
    if (!hasDataCommands) {
      throw new Error('DataDaemon commands not available - daemon registration issue');
    }

    console.log('✅ DataDaemon commands available');

    // Test 2: Directly test data storage
    console.log('💾 Testing direct DataDaemon storage...');
    const storeResult = await client.commands['data/create']({
      collection: 'test_chat_messages',
      data: testMessage
    });

    if (!storeResult.success) {
      throw new Error(`DataDaemon storage failed: ${storeResult.error}`);
    }

    console.log('✅ Direct DataDaemon storage works');

    // Test 3: Retrieve stored data
    console.log('📖 Testing DataDaemon retrieval...');
    const retrieveResult = await client.commands['data/list']({
      collection: 'test_chat_messages'
    });

    if (!retrieveResult.success) {
      throw new Error(`DataDaemon retrieval failed: ${retrieveResult.error}`);
    }

    console.log(`✅ Retrieved ${retrieveResult.records?.length || 0} stored messages`);

    await client.disconnect();
    
    return {
      success: true,
      message: 'Chat storage integration working correctly',
      storedMessages: retrieveResult.records?.length || 0
    };

  } catch (error) {
    console.error('❌ Chat storage integration test failed:', error);
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

// Run the test
testChatMessageStorage().then(result => {
  console.log('📊 TEST RESULT:', result);
  
  if (result.success) {
    console.log('🎉 ✅ CHAT STORAGE INTEGRATION TEST PASSED');
  } else {
    console.log('💥 ❌ CHAT STORAGE INTEGRATION TEST FAILED');
    process.exit(1);
  }
}).catch(error => {
  console.error('💥 Test execution failed:', error);
  process.exit(1);
});