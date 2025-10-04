#!/usr/bin/env npx tsx
/**
 * State System Integration Test
 *
 * Tests the state command system that provides elegant user context injection
 * and delegates to the underlying data commands:
 * - state/create with automatic user context injection
 * - state/update with user-aware data enhancement
 * - state/get with proper type safety and context
 * - Cross-entity state operations (User, Room, ChatMessage)
 *
 * Following the established test pattern from database-chat-integration.test.ts
 */

import { jtag } from '../../server-index';
import { UserEntity } from '../../system/data/entities/UserEntity';
import { RoomEntity } from '../../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../../system/data/entities/ChatMessageEntity';
import type { StateCreateResult } from '../../commands/state/create/shared/StateCreateTypes';
import type { StateGetResult } from '../../commands/state/get/shared/StateGetTypes';
import type { StateUpdateResult } from '../../commands/state/update/shared/StateUpdateTypes';
import type { BaseEntity } from '../../system/data/entities/BaseEntity';
import type { CommandSuccessResponse } from '../../daemons/command-daemon/shared/CommandResponseTypes';
import type { DataReadResult } from '../../commands/data/read/shared/DataReadTypes';

async function testStateSystemIntegration(): Promise<void> {
  console.log('🔧 STATE SYSTEM INTEGRATION TEST');
  console.log('=================================');

  let client = null;
  let stateUserId: string | undefined;
  let stateRoomId: string | undefined;
  let stateMessageId: string | undefined;
  const testTimestamp = Date.now();
  const testContext = `state-test-${testTimestamp}`;
  const testSessionId = `session-${testTimestamp}`;

  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected');

    // Test 1: State/Create - User with context injection
    console.log('👤 1. Testing state/create for user with context injection...');
    const userStateResponse: CommandSuccessResponse = await client.commands['state/create']({
      collection: UserEntity.collection,
      context: testContext,
      sessionId: testSessionId,
      userId: `state-test-user-${testTimestamp}`, // This should be injected
      data: {
        displayName: 'State Test User',
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

    if (!userStateResponse.success) {
      throw new Error(`State user creation failed: ${(userStateResponse as any).error ?? 'Unknown error'}`);
    }

    // Extract the actual StateCreateResult from the commandResult
    const userStateResult = userStateResponse.commandResult as StateCreateResult<UserEntity>;
    stateUserId = userStateResult.id || userStateResult.item?.id;
    console.log(`👤 Created state user with ID: ${stateUserId}`);
    if (!stateUserId) {
      console.error('❌ No user ID returned from state/create');
      console.error('Full result:', JSON.stringify(userStateResponse, null, 2));
      throw new Error('State user ID is undefined');
    }
    console.log('✅ State user created successfully');

    // Test 2: State/Create - Room with context and user injection
    console.log('🏠 2. Testing state/create for room with user context...');
    const roomStateResponse: CommandSuccessResponse = await client.commands['state/create']({
      collection: RoomEntity.collection,
      context: testContext,
      sessionId: testSessionId,
      userId: stateUserId, // Should be injected into room data
      data: {
        uniqueId: `state-test-room-${testTimestamp}`,
        name: 'state-test-room',
        displayName: 'State Test Room',
        description: 'Test room for state integration',
        topic: 'State system testing room',
        type: 'public',
        status: 'active',
        ownerId: stateUserId,
        lastMessageAt: new Date().toISOString(),
        privacy: {
          isPublic: true,
          requiresInvite: false,
          allowGuestAccess: true,
          searchable: true
        },
        settings: {
          allowReactions: true,
          allowThreads: true,
          allowFileSharing: true,
          messageRetentionDays: 365
        },
        stats: {
          memberCount: 1,
          messageCount: 0,
          createdAt: new Date().toISOString(),
          lastActivityAt: new Date().toISOString()
        },
        members: [],
        tags: ['state', 'test', 'integration']
      }
    });

    if (!roomStateResponse.success) {
      throw new Error(`State room creation failed: ${(roomStateResponse as any).error ?? 'Unknown error'}`);
    }

    const roomStateResult = roomStateResponse.commandResult as StateCreateResult<RoomEntity>;
    stateRoomId = roomStateResult.id || roomStateResult.item?.id;
    console.log(`🏠 Created state room with ID: ${stateRoomId}`);
    if (!stateRoomId) {
      console.error('❌ No room ID returned from state/create');
      console.error('Full result:', JSON.stringify(roomStateResponse, null, 2));
      throw new Error('State room ID is undefined');
    }
    console.log('✅ State room created successfully');

    // Test 3: State/Create - Chat message with full context
    console.log('💬 3. Testing state/create for chat message with context...');
    const messageStateResponse: CommandSuccessResponse = await client.commands['state/create']({
      collection: ChatMessageEntity.collection,
      context: testContext,
      sessionId: testSessionId,
      userId: stateUserId,
      data: {
        roomId: stateRoomId,
        senderId: stateUserId,
        senderName: 'State Test User',
        content: {
          text: 'This message was created via state/create with context injection',
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

    if (!messageStateResponse.success) {
      throw new Error(`State message creation failed: ${(messageStateResponse as any).error ?? 'Unknown error'}`);
    }

    const messageStateResult = messageStateResponse.commandResult as StateCreateResult<ChatMessageEntity>;
    stateMessageId = messageStateResult.id || messageStateResult.item?.id;
    console.log(`💬 Created state message with ID: ${stateMessageId}`);
    if (!stateMessageId) {
      console.error('❌ No message ID returned from state/create');
      console.error('Full result:', JSON.stringify(messageStateResponse, null, 2));
      throw new Error('State message ID is undefined');
    }
    console.log('✅ State message created successfully');

    // Test 4: State/Get - Retrieve with context (filter by ID)
    console.log('👤 4. Testing state/get for user retrieval with context...');
    const getUserResponse: CommandSuccessResponse = await client.commands['state/get']({
      collection: UserEntity.collection,
      context: testContext,
      sessionId: testSessionId,
      filter: { id: stateUserId }
    });

    if (!getUserResponse.success) {
      throw new Error(`State user retrieval failed: ${(getUserResponse as any).error || 'Command failed'}`);
    }
    const getUserResult = getUserResponse.commandResult as StateGetResult<UserEntity>;
    if (!getUserResult.items?.length) {
      throw new Error('No user found in state/get result');
    }
    const retrievedUser = getUserResult.items[0];
    console.log(`👤 Retrieved state user: ${retrievedUser.displayName}`);
    console.log('✅ State user retrieved successfully');

    // Test 5: State/Update - Update with context enhancement
    console.log('🔧 5. Testing state/update with context enhancement...');
    const updateResponse: CommandSuccessResponse = await client.commands['state/update']({
      collection: UserEntity.collection,
      context: testContext,
      sessionId: testSessionId,
      userId: stateUserId,
      id: stateUserId,
      data: {
        displayName: 'Updated State Test User',
        status: 'busy',
        lastActiveAt: new Date().toISOString()
      }
    });

    if (!updateResponse.success) {
      throw new Error(`State user update failed: ${(updateResponse as any).error ?? 'Unknown error'}`);
    }
    const updateResult = updateResponse.commandResult as StateUpdateResult<UserEntity>;
    console.log(`🔧 Updated state user version: ${updateResult.version}`);
    console.log('✅ State user updated successfully');

    // Test 6: Verify update took effect via state/get
    console.log('👤 6. Verifying update via state/get...');
    const getUpdatedUserResponse: CommandSuccessResponse = await client.commands['state/get']({
      collection: UserEntity.collection,
      context: testContext,
      sessionId: testSessionId,
      filter: { id: stateUserId }
    });

    if (!getUpdatedUserResponse.success) {
      throw new Error(`Updated state user retrieval failed`);
    }
    const getUpdatedUserResult = getUpdatedUserResponse.commandResult as StateGetResult<UserEntity>;
    if (!getUpdatedUserResult.items?.length) {
      throw new Error('No updated user found');
    }

    const updatedUser = getUpdatedUserResult.items[0];
    if (updatedUser.displayName !== 'Updated State Test User' || updatedUser.status !== 'busy') {
      throw new Error(`Update not reflected: got ${updatedUser.displayName} with status ${updatedUser.status}`);
    }
    console.log(`👤 Verified updated user: ${updatedUser.displayName} (${updatedUser.status})`);
    console.log('✅ State update verification successful');

    // Test 7: Cross-entity state operations - Room update
    console.log('🏠 7. Testing cross-entity state operations (room update)...');
    const roomUpdateResponse: CommandSuccessResponse = await client.commands['state/update']({
      collection: RoomEntity.collection,
      context: testContext,
      sessionId: testSessionId,
      userId: stateUserId,
      id: stateRoomId,
      data: {
        description: 'Updated via state/update with user context injection',
        topic: 'State system validation complete',
        stats: {
          memberCount: 1,
          messageCount: 1,
          createdAt: new Date().toISOString(),
          lastActivityAt: new Date().toISOString()
        }
      }
    });

    if (!roomUpdateResponse.success) {
      throw new Error(`State room update failed: ${(roomUpdateResponse as any).error ?? 'Unknown error'}`);
    }
    const roomUpdateResult = roomUpdateResponse.commandResult as StateUpdateResult<RoomEntity>;
    console.log(`🏠 Updated state room version: ${roomUpdateResult.version}`);
    console.log('✅ Cross-entity state update successful');

    // Test 8: Verify state commands delegate to data layer properly
    console.log('🔍 8. Verifying state delegation to data layer...');
    const dataVerifyResponse: CommandSuccessResponse = await client.commands['data/read']({
      collection: UserEntity.collection,
      id: stateUserId
    });

    if (!dataVerifyResponse.success) {
      throw new Error('Data layer verification failed - command failed');
    }
    // data/read returns data directly, not wrapped in commandResult like state commands
    const dataVerifyResult = dataVerifyResponse.commandResult as DataReadResult;
    if (dataVerifyResult && dataVerifyResult.success && dataVerifyResult.data) {
      // Wrapped format
      if (dataVerifyResult.data.displayName !== 'Updated State Test User') {
        throw new Error('State→Data delegation failed - changes not persisted to database');
      }
      console.log(`🔍 Verified data layer contains: ${dataVerifyResult.data.displayName}`);
    } else {
      // Direct format (common for data commands)
      const directResult = dataVerifyResponse as any;
      if (!directResult.success || !directResult.data) {
        throw new Error('Data layer verification failed - state commands not persisting');
      }
      if (directResult.data.displayName !== 'Updated State Test User') {
        throw new Error('State→Data delegation failed - changes not persisted to database');
      }
      console.log(`🔍 Verified data layer contains: ${directResult.data.displayName}`);
    }
    console.log('✅ State→Data delegation verified');

    console.log('');
    console.log('🎉 STATE SYSTEM INTEGRATION TEST PASSED');
    console.log('✅ All state commands working correctly:');
    console.log('  - state/create: ✅ User context injection');
    console.log('  - state/update: ✅ Context enhancement & delegation');
    console.log('  - state/get: ✅ Type-safe retrieval with context');
    console.log('  - Cross-entity: ✅ Works with User, Room, ChatMessage');
    console.log('  - Data layer: ✅ Proper delegation and persistence');

  } catch (error) {
    console.error('❌ State system test failed:', error);
    console.error('❌ Error details:', {
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined
    });
    throw error;
  } finally {
    // Cleanup: Delete test data with verification
    if (client && (stateMessageId || stateRoomId || stateUserId)) {
      console.log(`🧹 Cleaning up test data...`);

      // Delete message first (has foreign key to user)
      if (stateMessageId) {
        console.log(`🗑️ Deleting message ${stateMessageId}...`);
        const deleteResult = await client.commands['data/delete']({
          collection: ChatMessageEntity.collection,
          id: stateMessageId
        });
        if (deleteResult.error) {
          throw new Error(`Failed to delete message ${stateMessageId}: ${deleteResult.error}`);
        }

        // VERIFY deletion worked
        const verifyResult = await client.commands['data/read']({
          collection: ChatMessageEntity.collection,
          id: stateMessageId
        });
        if (verifyResult.success && verifyResult.data) {
          throw new Error(`❌ DELETE VERIFICATION FAILED: Message ${stateMessageId} still exists!`);
        }
        console.log(`✅ Deleted message ${stateMessageId}`);
      }

      // Delete room
      if (stateRoomId) {
        console.log(`🗑️ Deleting room ${stateRoomId}...`);
        const deleteResult = await client.commands['data/delete']({
          collection: RoomEntity.collection,
          id: stateRoomId
        });
        if (deleteResult.error) {
          throw new Error(`Failed to delete room ${stateRoomId}: ${deleteResult.error}`);
        }

        // VERIFY deletion worked
        const verifyResult = await client.commands['data/read']({
          collection: RoomEntity.collection,
          id: stateRoomId
        });
        if (verifyResult.success && verifyResult.data) {
          throw new Error(`❌ DELETE VERIFICATION FAILED: Room ${stateRoomId} still exists!`);
        }
        console.log(`✅ Deleted room ${stateRoomId}`);
      }

      // Delete user last
      if (stateUserId) {
        console.log(`🗑️ Deleting user ${stateUserId}...`);
        const deleteResult = await client.commands['data/delete']({
          collection: UserEntity.collection,
          id: stateUserId
        });
        if (deleteResult.error) {
          throw new Error(`Failed to delete user ${stateUserId}: ${deleteResult.error}`);
        }

        // VERIFY deletion worked
        const verifyResult = await client.commands['data/read']({
          collection: UserEntity.collection,
          id: stateUserId
        });
        if (verifyResult.success && verifyResult.data) {
          throw new Error(`❌ DELETE VERIFICATION FAILED: User ${stateUserId} still exists!`);
        }
        console.log(`✅ Deleted user ${stateUserId}`);
      }

      console.log('✅ Test data cleanup verified');
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
testStateSystemIntegration().then(() => {
  console.log('✅ State system integration test completed successfully');
  process.exit(0);
}).catch(error => {
  console.error('🚨 State system integration test failed:', error);
  process.exit(1);
});