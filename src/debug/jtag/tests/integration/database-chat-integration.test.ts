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
import { UserEntity } from '../../system/data/entities/UserEntity';
import { RoomEntity } from '../../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../../system/data/entities/ChatMessageEntity';

async function testDatabaseChatIntegration(): Promise<void> {
  console.log('🗄️ DATABASE CHAT INTEGRATION TEST');
  console.log('=================================');

  let client = null;
  let userId: string | undefined;
  const messageIds: string[] = [];
  const testTimestamp = Date.now();

  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected');

    // Test 1: Create a test user via data/create (state/create has timeout issue)
    console.log('👤 1. Testing user creation via data/create...');
    const userResult = await client.commands['data/create']({
      collection: UserEntity.collection,
      data: {
        displayName: 'Database Test User',
        type: 'human',
        status: 'online',
        lastActiveAt: new Date().toISOString(),
        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canCreateRooms: false,
          canInviteOthers: false,
          canModerate: false,
          autoResponds: false,
          providesContext: false,
          canTrain: false,
          canAccessPersonas: false
        },
        sessionsActive: []
      }
    });

    if (!userResult.success) {
      throw new Error(`User creation failed: ${userResult.error ?? 'Unknown error'}`);
    }

    // Use the actual generated ID from the result
    const userId = userResult.data?.id || userResult.id;
    console.log(`👤 Created user with ID: ${userId}`);
    console.log('✅ User created successfully');

    // Test 2: Use existing "General" room (displayed in chat widget)
    console.log('🏠 2. Querying for General room...');
    const roomsResult = await client.commands['data/list']({
      collection: RoomEntity.collection,
      filter: { uniqueId: 'general' }
    });

    if (!roomsResult.success || !roomsResult.items || roomsResult.items.length === 0) {
      throw new Error('General room not found');
    }

    const roomId = roomsResult.items[0].id;
    console.log(`🏠 Using General room with ID: ${roomId}`);
    console.log('✅ Room found successfully');

    // Test 3: Store test messages via data/create (state/create has timeout issue)
    console.log('💬 3. Testing message storage via data/create...');
    const messageIds = [];
    for (let i = 1; i <= 3; i++) {
      const messageResult = await client.commands['data/create']({
        collection: ChatMessageEntity.collection,
        data: {
          roomId,
          senderId: userId,
          senderName: 'Database Test User',
          content: {
            text: `Test message ${i} for database integration`,
            attachments: [],
            formatting: {
              markdown: false,
              mentions: [],
              hashtags: [],
              links: [],
              codeBlocks: []
            }
          },
          status: 'sent',
          priority: 'normal',
          timestamp: new Date().toISOString(),
          reactions: []
        }
      });

      if (!messageResult.success) {
        throw new Error(`Message ${i} creation failed: ${messageResult.error ?? 'Unknown error'}`);
      }

      // Use the actual generated ID from the result
      const messageId = messageResult.data?.id || messageResult.id;
      messageIds.push(messageId);
      console.log(`💬 Created message ${i} with ID: ${messageId}`);
    }
    console.log('✅ All messages stored successfully');

    // Test 4: Retrieve user data
    console.log(`👤 4. Testing user data retrieval for userId: ${userId}...`);
    const userListResult = await client.commands['data/list']({
      collection: UserEntity.collection,
      filter: { id: userId }
    });

    console.log(`📊 User list result: success=${userListResult.success}, count=${userListResult.items?.length || 0}`);
    if (!userListResult.success) {
      throw new Error(`User list query failed: ${userListResult.error || 'Unknown error'}`);
    }
    if (!userListResult.items?.length) {
      throw new Error(`No user found with id ${userId}. Total users: ${userListResult.count || 0}`);
    }
    console.log('✅ User data retrieved successfully');

    // Test 5: Retrieve messages for room
    console.log('💬 5. Testing room message retrieval...');
    const messagesResult = await client.commands['data/list']({
      collection: ChatMessageEntity.collection,
      filter: { roomId }
    });

    if (!messagesResult.success || !messagesResult.items?.length) {
      throw new Error('Room messages retrieval failed');
    }

    // Count includes seeded messages + our 3 test messages
    const initialMessageCount = messagesResult.items.length - 3;
    console.log(`✅ Room messages retrieved successfully (${messagesResult.items.length} total, ${initialMessageCount} pre-existing + 3 test)`);

    // Test 6: Retrieve messages by user (should be exactly 3 - only ours)
    console.log('👤 6. Testing user message retrieval...');
    const userMessagesResult = await client.commands['data/list']({
      collection: ChatMessageEntity.collection,
      filter: { senderId: userId }
    });

    if (!userMessagesResult.success || !userMessagesResult.items?.length) {
      throw new Error('User messages retrieval failed');
    }

    if (userMessagesResult.items.length !== 3) {
      throw new Error(`Expected 3 user messages, got ${userMessagesResult.items.length}`);
    }
    console.log('✅ User messages retrieved successfully (3 from test user)');

    // Test 7: CRITICAL - Verify messages appear in chat widget HTML (real-time events working)
    console.log('📱 7. Testing chat widget HTML rendering (CRUD → Event → UI chain)...');
    console.log('⏳ Waiting 2 seconds for events to propagate...');
    await new Promise(resolve => setTimeout(resolve, 2000));

    const widgetStateResult = await client.commands['debug/widget-state']({
      widgetSelector: 'chat-widget'
    });

    if (!widgetStateResult.success) {
      throw new Error('Failed to get widget state for HTML verification');
    }

    // Check widget entities (ChatWidget uses EntityScrollerWidget which stores entities)
    // Command result is wrapped in commandResult envelope
    const widgetData = (widgetStateResult as any).commandResult || widgetStateResult;
    const widgetEntities = widgetData.state?.entities || [];
    console.log(`📊 Chat widget has ${widgetEntities.length} message entities loaded`);

    // Widget uses pagination (20 messages per page), so we can't expect ALL messages
    // Instead, verify that our 3 NEW test messages appear in the widget (real-time events working)
    const testMessagesInWidget = widgetEntities.filter((m: any) => m.senderId === userId);

    if (testMessagesInWidget.length !== 3) {
      throw new Error(`❌ REAL-TIME EVENTS BROKEN: Only ${testMessagesInWidget.length}/3 test messages in chat widget. Event system not working! Widget has ${widgetEntities.length} total messages.`);
    }

    console.log(`✅ Chat widget rendering verified - all 3 test messages present (widget using pagination, has ${widgetEntities.length} loaded)`);

    // Test 8: UPDATE operation - modify first test message
    console.log('✏️ 8. Testing message UPDATE operation...');
    const firstMessageId = messageIds[0];
    const updateResult = await client.commands['data/update']({
      collection: ChatMessageEntity.collection,
      id: firstMessageId,
      data: {
        content: {
          text: 'UPDATED: This message was modified by CRUD test',
          attachments: [],
          formatting: { markdown: false, mentions: [], hashtags: [], links: [], codeBlocks: [] }
        }
      }
    });

    // Check if update succeeded (may return data directly or have success field)
    if (updateResult.error || (!updateResult.data && !updateResult.id)) {
      console.log('❌ Update result:', JSON.stringify(updateResult, null, 2));
      throw new Error(`Message update failed: ${updateResult.error ?? 'No data returned'}`);
    }
    console.log(`✅ Message updated successfully (ID: ${firstMessageId})`);

    // Test 9: DELETE operation - remove second test message
    console.log('🗑️ 9. Testing message DELETE operation...');
    const secondMessageId = messageIds[1];
    const deleteResult = await client.commands['data/delete']({
      collection: ChatMessageEntity.collection,
      id: secondMessageId
    });

    // Check if delete succeeded (returns id if successful)
    if (deleteResult.error || !deleteResult.id) {
      console.log('❌ Delete result:', JSON.stringify(deleteResult, null, 2));
      throw new Error(`Message deletion failed: ${deleteResult.error ?? 'No id returned'}`);
    }
    console.log(`✅ Message deleted successfully (ID: ${secondMessageId})`);

    // Test 10: Verify UPDATE and DELETE reflected in database
    console.log('🔍 10. Verifying UPDATE and DELETE operations...');
    const finalMessagesResult = await client.commands['data/list']({
      collection: ChatMessageEntity.collection,
      filter: { roomId }
    });

    if (!finalMessagesResult.success || !finalMessagesResult.items) {
      throw new Error('Failed to verify final state');
    }

    // Should have one less message (deleted one)
    const expectedFinalCount = messagesResult.items.length - 1;
    if (finalMessagesResult.items.length !== expectedFinalCount) {
      throw new Error(`Expected ${expectedFinalCount} messages after delete, got ${finalMessagesResult.items.length}`);
    }

    // Check updated message content
    const updatedMessage = finalMessagesResult.items.find((m: any) => m.id === firstMessageId);
    if (!updatedMessage || !updatedMessage.content.text.includes('UPDATED:')) {
      throw new Error('UPDATE operation did not persist correctly');
    }

    // Check deleted message is gone
    const deletedMessage = finalMessagesResult.items.find((m: any) => m.id === secondMessageId);
    if (deletedMessage) {
      throw new Error('DELETE operation did not remove message from database');
    }

    console.log(`✅ UPDATE and DELETE operations verified in database`);

    console.log('');
    console.log('🎉 FULL CRUD TEST PASSED');
    console.log('✅ All database operations working: CREATE, READ, UPDATE, DELETE + Event → UI sync');

  } catch (error) {
    console.error('❌ Test failed:', error);
    console.error('❌ Error details:', {
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined
    });
    throw error;
  } finally {
    // Cleanup: Delete test data
    if (client && userId) {
      try {
        console.log('🧹 Cleaning up test data...');

        // Delete test messages
        for (const msgId of messageIds) {
          await client.commands['data/delete']({
            collection: ChatMessageEntity.collection,
            id: msgId
          }).catch(() => {}); // Ignore errors if already deleted
        }

        // Delete test user
        await client.commands['data/delete']({
          collection: UserEntity.collection,
          id: userId
        }).catch(() => {});

        console.log('✅ Test data cleaned up');
      } catch (cleanupError) {
        console.warn('⚠️ Cleanup failed:', cleanupError);
      }
    }

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