/**
 * Default Rooms Test - Dynamic validation using JSON configuration
 * 
 * Tests iterate over the same JSON file used by ChatRoomDaemon for default room creation
 * to ensure consistency between configuration and actual behavior.
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { ChatRoomDaemon } from '../../ChatRoomDaemon';
import { ChatRoomType } from '../../../../types/shared/chat/ChatTypes';
import { CommandOperation } from '../../../../types/shared/CommandOperationTypes';
import { loadDefaultRoomsConfig, DefaultRoomSpec } from '../../DefaultRoomsConfig';

describe('ChatRoomDaemon Default Rooms', () => {
  let daemon: ChatRoomDaemon;
  let expectedRooms: DefaultRoomSpec[];

  beforeEach(async () => {
    daemon = new ChatRoomDaemon();
    // Load the same JSON configuration that the daemon uses
    expectedRooms = await loadDefaultRoomsConfig();
    await daemon.start();
  });

  afterEach(async () => {
    if (daemon) {
      await daemon.stop();
    }
  });

  it('should create all rooms defined in JSON configuration', async () => {
    // Get the list rooms handler
    const handlers = daemon.getRequestHandlers();
    const listRoomsHandler = handlers[CommandOperation.LIST_ROOMS];
    
    assert.ok(listRoomsHandler, 'List rooms handler should exist');
    
    // Get list of rooms (don't filter by user to get all default rooms)
    const result = await listRoomsHandler({
      correlationId: 'test-correlation',
      timestamp: Date.now()
    });

    // Validate response structure
    assert.strictEqual(result.success, true, 'List rooms should succeed');
    assert.ok(result.rooms, 'Should have rooms array');
    assert.ok(Array.isArray(result.rooms), 'Rooms should be an array');
    
    // Check that all configured rooms exist
    const actualRoomIds = result.rooms.map((room: any) => room.id);
    const expectedRoomIds = expectedRooms.map(room => room.id);
    
    console.log('📋 Expected rooms from JSON:', expectedRoomIds);
    console.log('📋 Actual rooms found:', actualRoomIds);
    
    // Verify each expected room exists
    for (const expectedRoom of expectedRooms) {
      assert.ok(
        actualRoomIds.includes(expectedRoom.id), 
        `Should have room: ${expectedRoom.id} (${expectedRoom.name})`
      );
      
      // Validate room details match configuration
      const actualRoom = result.rooms.find((room: any) => room.id === expectedRoom.id);
      assert.ok(actualRoom, `Room ${expectedRoom.id} should exist`);
      assert.strictEqual(actualRoom.name, expectedRoom.name, `Room ${expectedRoom.id} should have correct name`);
      assert.strictEqual(actualRoom.type, expectedRoom.type, `Room ${expectedRoom.id} should have correct type`);
    }
    
    console.log(`✅ All ${expectedRooms.length} configured default rooms validated successfully`);
  });

  it('should allow sending messages to all configured default rooms', async () => {
    const handlers = daemon.getRequestHandlers();
    const sendMessageHandler = handlers[CommandOperation.SEND_MESSAGE];
    const joinRoomHandler = handlers[CommandOperation.JOIN_ROOM];
    
    assert.ok(sendMessageHandler, 'Send message handler should exist');
    assert.ok(joinRoomHandler, 'Join room handler should exist');
    
    // Test message sending to each configured room
    for (const expectedRoom of expectedRooms) {
      // First join the room as test-user
      await joinRoomHandler({
        room_id: expectedRoom.id,
        user_id: 'test-user',
        session_id: 'test-session',
        correlationId: 'test-correlation',
        timestamp: Date.now()
      });
      
      // Then send message
      const result = await sendMessageHandler({
        room_id: expectedRoom.id,
        sender_id: 'test-user',
        content: `Test message for ${expectedRoom.name}`,
        correlationId: 'test-correlation',
        timestamp: Date.now()
      });

      assert.strictEqual(result.success, true, `Should successfully send message to ${expectedRoom.name}`);
      assert.ok(result.message_id, `Should return message ID for ${expectedRoom.name}`);
      assert.ok(result.message, `Should return message object for ${expectedRoom.name}`);
      assert.strictEqual(result.message.content, `Test message for ${expectedRoom.name}`);
      assert.strictEqual(result.message.room_id, expectedRoom.id);
    }
    
    console.log(`✅ Message sending validated for all ${expectedRooms.length} configured default rooms`);
  });

  it('should register all expected ChatRoom operation handlers', () => {
    const handlers = daemon.getRequestHandlers();
    
    // Check that all ChatRoom operations have handlers
    const expectedOperations = [
      CommandOperation.CREATE_ROOM,
      CommandOperation.JOIN_ROOM,
      CommandOperation.LEAVE_ROOM,
      CommandOperation.SEND_MESSAGE,
      CommandOperation.GET_MESSAGES,
      CommandOperation.LIST_ROOMS,
      CommandOperation.GET_ROOM_INFO,
      CommandOperation.DELETE_ROOM
    ];
    
    for (const operation of expectedOperations) {
      assert.ok(handlers[operation], `Should have handler for ${operation}`);
    }
    
    console.log('✅ All ChatRoom operation handlers validated');
  });
});