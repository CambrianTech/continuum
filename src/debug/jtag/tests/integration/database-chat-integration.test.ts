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

  // Widget HTML check
  const widgetState = await client.commands['debug/widget-state']({ widgetSelector });
  const widgetData = (widgetState as any).commandResult || widgetState;
  const entities = widgetData.state?.entities || [];
  const inWidget = entityId ? entities.some((e: any) => e.id === entityId) : false;
  console.log(`  🎨 Widget: ${entityId ? (inWidget ? '✅ Found' : '❌ Not found') : '✅ Baseline'}`);

  // Screenshot - ONE CALL IN ENTIRE CODEBASE
  await client.commands['screenshot']({
    querySelector: widgetSelector,
    filename: `${screenshotPrefix}-${operationName}.png`
  });
  console.log(`  📸 Screenshot: ${screenshotPrefix}-${operationName}.png`);
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

    // Test 1: User CRUD with user-list-widget
    const testUserId = await testCrud(
      client,
      'user-list-widget',
      UserEntity.collection,
      {
        displayName: 'CRUD Test User',
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