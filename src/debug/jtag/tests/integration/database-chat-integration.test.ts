#!/usr/bin/env npx tsx
/**
 * Database Chat Integration Test
 *
 * Tests database operations that are critical for chat functionality:
 * - User creation and retrieval
 * - Chat room creation
 * - Message storage and retrieval
 * - Cross-collection queries for chat history
 *
 * Following the established test pattern from working tests.
 */

import { jtag } from '../../server-index';

async function testDatabaseChatIntegration(): Promise<void> {
  console.log('🗄️ DATABASE CHAT INTEGRATION TEST');
  console.log('=================================');

  let client = null;
  const testTimestamp = Date.now();

  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected');

    // Test 1: Create a test user
    console.log('👤 1. Testing user creation...');
    const userId = `test-user-${testTimestamp}`;
    const userResult = await client.commands['data/create']({
      collection: 'users',
      data: {
        userId,
        name: 'Database Test User',
        type: 'human',
        created: new Date().toISOString()
      },
      id: userId
    });

    if (!userResult.success) {
      throw new Error(`User creation failed: ${userResult.error ?? 'Unknown error'}`);
    }
    console.log('✅ User created successfully');

    // Test 2: Create a test room
    console.log('🏠 2. Testing room creation...');
    const roomId = `test-room-${testTimestamp}`;
    const roomResult = await client.commands['data/create']({
      collection: 'chat-rooms',
      data: {
        roomId,
        name: 'Database Test Room',
        description: 'Test room for database integration',
        category: 'test',
        allowAI: true,
        isPrivate: false,
        createdAt: new Date().toISOString()
      },
      id: roomId
    });

    if (!roomResult.success) {
      throw new Error(`Room creation failed: ${roomResult.error ?? 'Unknown error'}`);
    }
    console.log('✅ Room created successfully');

    // Test 3: Store test messages
    console.log('💬 3. Testing message storage...');
    const messageIds = [];
    for (let i = 1; i <= 3; i++) {
      const messageId = `test-msg-${testTimestamp}-${i}`;
      const messageResult = await client.commands['data/create']({
        collection: 'messages',
        data: {
          messageId,
          roomId,
          senderId: userId,
          content: `Test message ${i} for database integration`,
          timestamp: new Date().toISOString(),
          category: 'chat'
        },
        id: messageId
      });

      if (!messageResult.success) {
        throw new Error(`Message ${i} creation failed: ${messageResult.error ?? 'Unknown error'}`);
      }
      messageIds.push(messageId);
    }
    console.log('✅ All messages stored successfully');

    // Test 4: Retrieve user data
    console.log('👤 4. Testing user data retrieval...');
    const userListResult = await client.commands['data/list']({
      collection: 'users',
      filter: { userId }
    });

    if (!userListResult.success || !userListResult.items?.length) {
      throw new Error('User retrieval failed');
    }
    console.log('✅ User data retrieved successfully');

    // Test 5: Retrieve messages for room
    console.log('💬 5. Testing room message retrieval...');
    const messagesResult = await client.commands['data/list']({
      collection: 'messages',
      filter: { roomId }
    });

    if (!messagesResult.success || !messagesResult.items?.length) {
      throw new Error('Room messages retrieval failed');
    }

    if (messagesResult.items.length !== 3) {
      throw new Error(`Expected 3 messages, got ${messagesResult.items.length}`);
    }
    console.log('✅ Room messages retrieved successfully');

    // Test 6: Retrieve messages by user
    console.log('👤 6. Testing user message retrieval...');
    const userMessagesResult = await client.commands['data/list']({
      collection: 'messages',
      filter: { senderId: userId }
    });

    if (!userMessagesResult.success || !userMessagesResult.items?.length) {
      throw new Error('User messages retrieval failed');
    }

    if (userMessagesResult.items.length !== 3) {
      throw new Error(`Expected 3 user messages, got ${userMessagesResult.items.length}`);
    }
    console.log('✅ User messages retrieved successfully');

    console.log('');
    console.log('🎉 DATABASE CHAT INTEGRATION TEST PASSED');
    console.log('✅ All database operations for chat functionality working correctly');

  } catch (error) {
    console.error('❌ Test failed:', error);
    console.error('❌ Error details:', {
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined
    });
    throw error;
  } finally {
    // Always disconnect
    if (client) {
      try {
        console.log('🔌 Disconnecting...');
        await client.disconnect();
        console.log('✅ Disconnected cleanly');
      } catch (disconnectError) {
        console.error('Disconnect error:', disconnectError);
      }
    }
  }
}

// Run test and exit
testDatabaseChatIntegration().then(() => {
  console.log('✅ Database chat integration test completed successfully');
  process.exit(0);
}).catch(error => {
  console.error('🚨 Database chat integration test failed:', error);
  process.exit(1);
});