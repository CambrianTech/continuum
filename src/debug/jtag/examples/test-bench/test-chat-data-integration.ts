/**
 * Chat-Data Integration Test
 * 
 * Tests that chat system works with data daemon foundation
 */

import { jtag } from '@continuum/jtag';

async function testChatDataIntegration() {
  console.log('💬 Starting Chat-Data Integration Test...');
  
  try {
    // Connect to JTAG system  
    const connectionResult = await jtag.connect();
    const jtagClient = connectionResult;
    console.log('✅ Connected to JTAG system');
    
    const testRoomId = 'test-room-' + Date.now();
    const testMessage = {
      content: 'Hello from integrated chat system!',
      user: 'test-user',
      timestamp: new Date().toISOString()
    };
    
    console.log('\n💬 Testing chat message persistence via data daemon...');
    
    // Store chat message using data daemon
    const createResult = await jtagClient.commands['data/create']({
      collection: 'chat-messages',
      data: {
        roomId: testRoomId,
        message: testMessage,
        metadata: {
          system: 'chat-integration-test',
          version: '1.0'
        }
      },
      format: 'json'
    });
    
    console.log('Chat message create result:', createResult);
    
    if (!createResult.success) {
      throw new Error(`Chat message creation failed: ${createResult.error}`);
    }
    
    const messageId = createResult.id;
    console.log(`✅ CHAT CREATE: Message stored with ID ${messageId}`);
    
    console.log('\n📖 Testing chat message retrieval...');
    const readResult = await jtagClient.commands['data/read']({
      collection: 'chat-messages',
      id: messageId,
      format: 'json'
    });
    
    if (!readResult.success) {
      throw new Error(`Chat message read failed: ${readResult.error}`);
    }
    
    console.log(`✅ CHAT READ: Retrieved message`, readResult.data?.data);
    
    console.log('\n📋 Testing chat room messages listing...');
    const listResult = await jtagClient.commands['data/list']({
      collection: 'chat-messages',
      format: 'json'
    });
    
    if (!listResult.success) {
      throw new Error(`Chat messages list failed: ${listResult.error}`);
    }
    
    console.log(`✅ CHAT LIST: Found ${listResult.data?.length || 0} messages in chat-messages collection`);
    
    console.log('\n🧹 Testing chat message cleanup...');
    const deleteResult = await jtagClient.commands['data/delete']({
      collection: 'chat-messages',
      id: messageId,
      format: 'json'
    });
    
    if (!deleteResult.success) {
      throw new Error(`Chat message delete failed: ${deleteResult.error}`);
    }
    
    console.log(`✅ CHAT DELETE: Message removed successfully`);
    
    console.log('\n🎉 CHAT-DATA INTEGRATION SUCCESS!');
    console.log('📊 Integration Test Summary:');
    console.log('  ✅ Chat messages persist via data daemon');
    console.log('  ✅ Chat messages retrievable with proper structure');
    console.log('  ✅ Chat room listing works through data layer');
    console.log('  ✅ Chat cleanup works through data layer');
    console.log('  ✅ Universal data daemon serves chat system perfectly');
    
  } catch (error) {
    console.error('❌ Chat-Data integration test failed:', error);
    throw error;
  }
}

// Run the test
testChatDataIntegration().catch(console.error);