#!/usr/bin/env npx tsx
/**
 * Simple Multi-User Chat Integration Test
 * Tests core multi-user chat functionality for AI persona integration
 */

import { TestUserManager } from '../shared/TestUserManager';

async function testSimpleMultiUserChat(): Promise<boolean> {
  const userManager = new TestUserManager();
  
  try {
    console.log('🎯 SIMPLE MULTI-USER CHAT INTEGRATION TEST');
    console.log('Testing: Human (server) + AIAssistant (server) + DevAssistant (browser)');
    
    const roomId = `simple-multiuser-chat-${Date.now()}`;
    
    // Connect all test users using encapsulated manager
    console.log('👥 Connecting 3 users...');
    const users = await userManager.connectStandardUsers();
    
    // Execute conversation scenario using encapsulated manager
    console.log('💬 Starting simple conversation...');
    await userManager.executeConversation(roomId, [
      { user: 'Human', content: 'Hello everyone! Testing simple multi-user chat.' },
      { user: 'AIAssistant', content: 'Hello! AI Assistant here. Multi-user chat working.' },
      { user: 'DevAssistant', content: 'DevAssistant connected. Chat system functioning properly.' }
    ]);
    
    // Skip screenshot to avoid timeout issues  
    console.log('📸 Screenshot skipped (avoiding timeout)');
    
    // Validate database persistence
    console.log('🗄️ Validating simple chat persistence...');
    const controlUser = userManager.getUser('Human');
    if (!controlUser) {
      throw new Error('Control user not found');
    }
    
    const dbResult = await controlUser.client.commands['data/list']({
      collection: 'chat_messages',
      format: 'json'
    });
    
    const roomMessages = dbResult.items?.filter((item: any) => 
      item.data?.roomId === roomId
    ) || [];
    
    const success = roomMessages.length >= 3;
    
    console.log('');
    console.log('📊 SIMPLE MULTI-USER CHAT RESULTS:');
    console.log(`💾 Messages stored: ${roomMessages.length}`);
    console.log(`✅ Chat system status: ${success ? 'PASS' : 'FAIL'}`);
    
    if (success) {
      console.log('🎉 Simple multi-user chat working!');
    }
    
    return success;
    
  } catch (error) {
    console.error('❌ Simple multi-user chat integration test failed:', error);
    return false;
  } finally {
    // Disconnect all users using encapsulated manager
    await userManager.disconnectAll();
  }
}

testSimpleMultiUserChat().then(success => {
  console.log(success ? '🎉 SIMPLE MULTI-USER CHAT TEST PASSED' : '💥 SIMPLE MULTI-USER CHAT TEST FAILED');
}).catch(error => {
  console.error('💥 Test execution failed:', error);
  process.exit(1);
});