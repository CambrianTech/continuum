#!/usr/bin/env npx tsx
/**
 * Minimal Working Chat Test
 * 
 * Start simple, get it working, then build up.
 * No fancy abstractions - just basic functionality proof.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGClient } from '../../system/core/client/shared/JTAGClient';

interface TestUser {
  client: JTAGClient;
  name: string;
}

async function testMinimalWorkingChat(): Promise<boolean> {
  const users: TestUser[] = [];
  
  try {
    console.log('🧪 MINIMAL WORKING CHAT TEST');
    console.log('Goal: Get basic 2-user chat working without complications');
    
    const roomId = `test-room-${Date.now()}`;
    const serverUrl = 'ws://localhost:9001'; // Test-specific URL
    
    // Connect User 1 (Server)
    console.log('👤 Connecting User1...');
    const { client: user1Client } = await JTAGClientServer.connect({
      targetEnvironment: 'server',
      transportType: 'websocket',
      serverUrl,
      context: { displayName: 'TestUser1' }
    });
    users.push({ client: user1Client, name: 'TestUser1' });
    console.log('✅ TestUser1 connected');
    
    // Connect User 2 (Server)
    console.log('👤 Connecting User2...');
    const { client: user2Client } = await JTAGClientServer.connect({
      targetEnvironment: 'server',
      transportType: 'websocket', 
      serverUrl,
      context: { displayName: 'TestUser2' }
    });
    users.push({ client: user2Client, name: 'TestUser2' });
    console.log('✅ TestUser2 connected');
    
    // Send messages
    console.log('💬 Testing basic message sending...');
    
    // User 1 sends message
    await user1Client.commands['collaboration/chat/send']({
      roomId,
      content: 'Hello from User1!',
      messageContext: { role: 'tester' }
    });
    console.log('📨 User1 sent message');
    
    // Wait a bit
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // User 2 sends message  
    await user2Client.commands['collaboration/chat/send']({
      roomId,
      content: 'Hello back from User2!',
      messageContext: { role: 'tester' }
    });
    console.log('📨 User2 sent message');
    
    // Check if messages were stored
    console.log('🔍 Checking message storage...');
    const historyResult = await user1Client.commands['data/list']({
      collection: 'chat_messages',
      format: 'json'
    });
    
    const roomMessages = historyResult.items?.filter((item: any) => 
      item.data?.roomId === roomId
    ) || [];
    
    console.log(`📊 Found ${roomMessages.length} messages in database`);
    
    if (roomMessages.length >= 2) {
      console.log('🎉 SUCCESS: Minimal chat test working!');
      console.log('✅ Messages sent and stored successfully');
      return true;
    } else {
      console.log('❌ FAIL: Not enough messages stored');
      return false;
    }
    
  } catch (error) {
    console.error('❌ Minimal chat test failed:', error);
    return false;
  } finally {
    // Cleanup
    for (const user of users) {
      try {
        if ('disconnect' in user.client && typeof user.client.disconnect === 'function') {
          await user.client.disconnect();
          console.log(`🔌 ${user.name} disconnected`);
        }
      } catch (error) {
        console.log(`⚠️ Error disconnecting ${user.name}:`, error);
      }
    }
  }
}

// Run the test
testMinimalWorkingChat().then(success => {
  console.log(success ? '🎉 MINIMAL CHAT TEST PASSED' : '💥 MINIMAL CHAT TEST FAILED');
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('💥 Test execution failed:', error);
  process.exit(1);
});