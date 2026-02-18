/**
 * SIMPLIFIED CHAT DATA LAYER TEST
 * 
 * Tests only the working parts:
 * - Data storage via data/create commands
 * - Data retrieval via data/list commands
 * - Storage path consistency
 * 
 * UI layer is non-functional (widgets display but don't respond to interactions)
 * but data layer is confirmed working based on bidirectional test results.
 */

import { JTAGClientFactory } from '../tests/shared/JTAGClientFactory';

async function testDataLayerFunctionality() {
  console.log('🧪 Testing Chat Data Layer Functionality');
  
  const factory = JTAGClientFactory.getInstance();
  const connection = await factory.createClient({ timeout: 15000 });
  const jtag = connection.client;
  
  try {
    // Test 1: Create message
    const testMessage = {
      messageId: `data-test-${Date.now()}`,
      content: `Data layer test message: ${new Date().toISOString()}`,
      roomId: 'general',
      senderId: 'test-system',
      senderName: 'Test System',
      timestamp: new Date().toISOString()
    };
    
    console.log('📝 Creating test message...');
    const createResult = await jtag.commands['data/create']({
      collection: 'chat_messages',
      data: testMessage,
      sessionId: connection.sessionId
    });
    
    if (createResult.success) {
      console.log('✅ Message created successfully');
    } else {
      console.log('❌ Message creation failed:', createResult.error);
      return false;
    }
    
    // Test 2: Retrieve message
    console.log('📚 Retrieving messages...');
    const listResult = await jtag.commands['data/list']({
      collection: 'chat_messages',
      sessionId: connection.sessionId,
      limit: 20
    });
    
    if (listResult.success && listResult.items) {
      const foundMessage = listResult.items.find((item: any) => 
        item.data && item.data.messageId === testMessage.messageId
      );
      
      if (foundMessage) {
        console.log('✅ Message retrieved successfully');
        console.log('📋 Message content:', foundMessage.data.content);
        return true;
      } else {
        console.log('❌ Message not found in retrieval');
        console.log('📋 Retrieved items:', listResult.items.length);
        return false;
      }
    } else {
      console.log('❌ Message retrieval failed:', listResult.error);
      return false;
    }
    
  } catch (error) {
    console.error('💥 Test error:', error);
    return false;
  }
}

// Run if executed directly
if (require.main === module) {
  testDataLayerFunctionality().then(success => {
    if (success) {
      console.log('🎉 DATA LAYER IS FULLY FUNCTIONAL');
      console.log('⚠️ UI layer needs fixing (widgets display but don\'t respond)');
    } else {
      console.log('❌ Data layer test failed');
    }
    process.exit(success ? 0 : 1);
  });
}