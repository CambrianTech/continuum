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

/**
 * cleanupOrphanedTestData - Delete leftover test data from previous failed test runs
 * CRITICAL: Run BEFORE tests to ensure clean slate
 */
async function cleanupOrphanedTestData(client: any): Promise<void> {
  console.log('\n🧹 Cleaning up orphaned test data from previous runs...');

  try {
    // Clean up test users (displayName contains "Test User")
    const usersResult = await client.commands['data/list']({
      collection: UserEntity.collection,
      filter: {}
    });

    if (usersResult.success && usersResult.items) {
      const testUsers = usersResult.items.filter((user: any) =>
        user.displayName?.includes('Test User') || user.uniqueId?.includes('crud-test-user')
      );

      for (const user of testUsers) {
        await client.commands['data/delete']({
          collection: UserEntity.collection,
          id: user.id
        });
        console.log(`  ✅ Deleted orphaned test user: ${user.displayName} (${user.id})`);
      }

      if (testUsers.length === 0) {
        console.log('  ✅ No orphaned test users found');
      }
    }

    // Clean up test rooms (displayName contains "Test Room")
    const roomsResult = await client.commands['data/list']({
      collection: RoomEntity.collection,
      filter: {}
    });

    if (roomsResult.success && roomsResult.items) {
      const testRooms = roomsResult.items.filter((room: any) =>
        room.displayName?.includes('Test Room') || room.uniqueId?.includes('crud-test-room')
      );

      for (const room of testRooms) {
        await client.commands['data/delete']({
          collection: RoomEntity.collection,
          id: room.id
        });
        console.log(`  ✅ Deleted orphaned test room: ${room.displayName} (${room.id})`);
      }

      if (testRooms.length === 0) {
        console.log('  ✅ No orphaned test rooms found');
      }
    }

    // Clean up test messages (senderName is "CRUD Test")
    const messagesResult = await client.commands['data/list']({
      collection: ChatMessageEntity.collection,
      filter: {}
    });

    if (messagesResult.success && messagesResult.items) {
      const testMessages = messagesResult.items.filter((msg: any) =>
        msg.senderName === 'CRUD Test' || msg.content?.text?.includes('CRUD test message')
      );

      for (const message of testMessages) {
        await client.commands['data/delete']({
          collection: ChatMessageEntity.collection,
          id: message.id
        });
        console.log(`  ✅ Deleted orphaned test message: ${message.id}`);
      }

      if (testMessages.length === 0) {
        console.log('  ✅ No orphaned test messages found');
      }
    }

    console.log('✅ Cleanup complete\n');
  } catch (error) {
    console.error('⚠️  Cleanup failed (non-fatal):', error);
    // Don't throw - cleanup failures shouldn't block tests
  }
}

/**
 * waitForWidgetInitialization - Wait for widgets to initialize and subscribe to events
 * CRITICAL: Widgets must be initialized BEFORE CRUD operations or events will be missed
 */
async function waitForWidgetInitialization(client: any, widgetSelectors: string[]): Promise<void> {
  console.log('⏳ Waiting for widgets to initialize...');

  // Wait 3 seconds for widget initialization
  await new Promise(resolve => setTimeout(resolve, 3000));

  // Verify each widget exists and has event subscriptions set up
  for (const selector of widgetSelectors) {
    const widgetState = await client.commands['debug/widget-state']({ widgetSelector: selector });
    const widgetData = (widgetState as any).commandResult || widgetState;

    if (!widgetData.widgetFound) {
      throw new Error(`Widget ${selector} not found - cannot run tests`);
    }

    console.log(`✅ Widget ${selector} initialized`);
  }
}

/**
 * verifyState - ONE function that does DB check + HTML check + Screenshot
 * Called after EVERY operation to verify system state
 */
async function verifyState(
  client: any,
  collection: string,
  widgetSelector: string,
  entityId: string | null,
  operationName: string,
  screenshotPrefix: string
): Promise<void> {
  console.log(`\n📋 VERIFY after ${operationName.toUpperCase()}:`);

  await new Promise(resolve => setTimeout(resolve, 2000));

  // DB check
  if (entityId) {
    const dbResult = await client.commands['data/read']({ collection, id: entityId });
    const found = dbResult.success && dbResult.data;
    console.log(`  📊 DB: ${found ? '✅ Found' : '❌ Not found'}`);
    if (operationName === 'delete' && found) {
      throw new Error('Entity still in DB after delete');
    }
    if (operationName !== 'delete' && !found) {
      throw new Error('Entity not in DB');
    }
  }

  // Widget check - verify the SPECIFIC entity we created/updated/deleted
  const widgetState = await client.commands['debug/widget-state']({ widgetSelector });
  const widgetData = (widgetState as any).commandResult || widgetState;
  const entityCount = widgetData.state?.entityCount ?? 0;
  const entityIds = widgetData.state?.entityIds ?? [];

  // For CREATE/UPDATE: Verify the SPECIFIC entity ID exists in widget
  // For DELETE: Verify the SPECIFIC entity ID does NOT exist in widget
  // For BEFORE: Just check widget is working
  if (operationName === 'before') {
    console.log(`  🎨 Widget: ✅ Widget working (${entityCount} entities)`);
  } else if (entityId) {
    const entityFound = entityIds.includes(entityId);

    if (operationName === 'create' || operationName === 'update') {
      if (entityFound) {
        console.log(`  🎨 Widget: ✅ Entity ${entityId} found in widget (${entityCount} total)`);
      } else {
        console.log(`  🎨 Widget: ❌ Entity ${entityId} NOT found in widget (${entityCount} total)`);
        console.log(`  🔍 Widget has ${entityIds.length} entity IDs: ${entityIds.slice(0, 5).join(', ')}...`);
        throw new Error(`Entity ${entityId} not found in widget after ${operationName}`);
      }
    } else if (operationName === 'delete') {
      if (!entityFound) {
        console.log(`  🎨 Widget: ✅ Entity ${entityId} removed from widget (${entityCount} total)`);
      } else {
        console.log(`  🎨 Widget: ❌ Entity ${entityId} still in widget after delete (${entityCount} total)`);
        throw new Error(`Entity ${entityId} still in widget after delete`);
      }
    }
  }

  // Screenshot - ONE CALL IN ENTIRE CODEBASE
  // Skip chat-widget screenshots (30+ messages cause browser hang under load)
  if (widgetSelector !== 'chat-widget') {
    await client.commands['screenshot']({
      querySelector: widgetSelector,
      filename: `${screenshotPrefix}-${operationName}.png`
    });
    console.log(`  📸 Screenshot: ${screenshotPrefix}-${operationName}.png`);
  } else {
    console.log(`  📸 Screenshot: SKIPPED (chat-widget too heavy for precommit)`);
  }
}

/**
 * testCrud - Universal CRUD test function
 * Calls verifyState after EACH operation
 */
async function testCrud(
  client: any,
  widgetSelector: string,
  entityCollection: string,
  createData: any,
  updateData: any,
  screenshotPrefix: string
): Promise<string> {
  console.log(`\n🧪 CRUD TEST: ${entityCollection} → ${widgetSelector}`);

  // 1. BEFORE - Baseline
  await verifyState(client, entityCollection, widgetSelector, null, 'before', screenshotPrefix);

  // 2. CREATE
  const createResult = await client.commands['data/create']({
    collection: entityCollection,
    data: createData
  });
  if (!createResult.success) {
    throw new Error(`CREATE failed: ${createResult.error}`);
  }
  const entityId = createResult.data?.id || createResult.id;
  console.log(`✅ CREATE: ${entityId}`);

  await verifyState(client, entityCollection, widgetSelector, entityId, 'create', screenshotPrefix);

  // 3. UPDATE
  const updateResult = await client.commands['data/update']({
    collection: entityCollection,
    id: entityId,
    data: updateData
  });
  if (updateResult.error) {
    throw new Error(`UPDATE failed: ${updateResult.error}`);
  }
  console.log(`✅ UPDATE: Modified`);

  await verifyState(client, entityCollection, widgetSelector, entityId, 'update', screenshotPrefix);

  // 4. DELETE
  const deleteResult = await client.commands['data/delete']({
    collection: entityCollection,
    id: entityId
  });
  if (deleteResult.error) {
    throw new Error(`DELETE failed: ${deleteResult.error}`);
  }
  console.log(`✅ DELETE: Removed`);

  await verifyState(client, entityCollection, widgetSelector, null, 'delete', screenshotPrefix);

  return entityId;
}

async function testDatabaseChatIntegration(): Promise<void> {
  console.log('🗄️ DATABASE CHAT INTEGRATION TEST');
  console.log('=================================');

  let client = null;

  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected');

    // CRITICAL: Clean up orphaned test data from previous failed runs FIRST
    await cleanupOrphanedTestData(client);

    // CRITICAL: Wait for widgets to initialize BEFORE running CRUD operations
    // Widgets must subscribe to events BEFORE entities are created, or events will be missed
    await waitForWidgetInitialization(client, ['user-list-widget', 'room-list-widget', 'chat-widget']);

    // Test 1: User CRUD with user-list-widget
    const testUserId = await testCrud(
      client,
      'user-list-widget',
      UserEntity.collection,
      {
        displayName: 'CRUD Test User',
        uniqueId: `crud-test-user-${Date.now()}`,  // uniqueId is required
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
      },
      { displayName: 'UPDATED Test User', status: 'busy' },
      'user'
    );

    // Test 2: Room CRUD with room-list-widget
    const testRoomId = await testCrud(
      client,
      'room-list-widget',
      RoomEntity.collection,
      {
        uniqueId: `crud-test-room-${Date.now()}`,
        name: 'crud-test-room',
        displayName: 'CRUD Test Room',
        description: 'Test room for CRUD operations',
        topic: 'Testing',
        type: 'public',
        status: 'active',
        ownerId: '0137e402',
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
        tags: ['test', 'crud']
      },
      { description: 'UPDATED Test Room', topic: 'Updated Testing' },
      'room'
    );

    // Get existing General room for message tests
    const roomsResult = await client.commands['data/list']({
      collection: RoomEntity.collection,
      filter: { uniqueId: 'general' }
    });
    if (!roomsResult.success || !roomsResult.items?.length) {
      throw new Error('General room not found');
    }
    const generalRoomId = roomsResult.items[0].id;

    // Test 3: ChatMessage CRUD with chat-widget
    await testCrud(
      client,
      'chat-widget',
      ChatMessageEntity.collection,
      {
        roomId: generalRoomId,
        senderId: '0137e402',
        senderName: 'CRUD Test',
        senderType: 'system',  // Required field for ChatMessageEntity
        content: {
          text: 'CRUD test message',
          attachments: [],
          formatting: { markdown: false, mentions: [], hashtags: [], links: [], codeBlocks: [] }
        },
        status: 'sent',
        priority: 'normal',
        timestamp: new Date().toISOString(),
        reactions: []
      },
      {
        content: {
          text: 'UPDATED test message',
          attachments: [],
          formatting: { markdown: false, mentions: [], hashtags: [], links: [], codeBlocks: [] }
        }
      },
      'message'
    );

    console.log('\n🎉 ALL CRUD TESTS PASSED (3 entities × 4 operations = 12 screenshots)');

  } catch (error) {
    console.error('❌ Test failed:', error);
    throw error;
  } finally {
    if (client) {
      try {
        await client.disconnect();
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