/**
 * ChatRoomDaemon Unit Tests
 * 
 * Tests for ChatRoom daemon functionality including default room creation,
 * message handling, and room management operations.
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { ChatRoomDaemon } from '../../ChatRoomDaemon';
import { ChatRoomType, ParticipantRole, ParticipantStatus } from '../../../../types/shared/ChatRoomTypes';
import { CommandOperation } from '../../../../types/shared/CommandOperationTypes';
import { loadDefaultRoomsConfig, DefaultRoomSpec } from '../../DefaultRoomsConfig';

describe('ChatRoomDaemon Unit Tests', () => {
  let daemon: ChatRoomDaemon;
  let expectedRooms: DefaultRoomSpec[];

  beforeEach(async () => {
    daemon = new ChatRoomDaemon();
    // Load the same JSON configuration that the daemon uses
    expectedRooms = await loadDefaultRoomsConfig();
    // Start the daemon to trigger default room creation
    await daemon.start();
  });

  afterEach(async () => {
    if (daemon) {
      await daemon.stop();
    }
  });

  describe('Default Rooms Creation', () => {
    it('should create default rooms on startup', async () => {
      // Get the request handlers to access the list rooms functionality
      const handlers = daemon.getRequestHandlers();
      const listRoomsHandler = handlers[CommandOperation.LIST_ROOMS];
      
      assert.ok(listRoomsHandler, 'List rooms handler should be defined');
      
      // Call list rooms to verify default rooms exist (don't filter by user_id to get all rooms)
      const result = await listRoomsHandler({
        correlationId: 'test-correlation',
        timestamp: Date.now()
      });

      assert.strictEqual(result.success, true, 'List rooms should succeed');
      assert.ok(result.rooms, 'Rooms should be defined');
      assert.ok(Array.isArray(result.rooms), 'Rooms should be an array');
      assert.ok(result.rooms.length >= expectedRooms.length, `Should have at least ${expectedRooms.length} default rooms`);

      // Verify all configured default rooms exist
      const actualRoomIds = result.rooms.map((room: any) => room.id);
      const expectedRoomIds = expectedRooms.map(room => room.id);
      
      for (const expectedRoomId of expectedRoomIds) {
        assert.ok(actualRoomIds.includes(expectedRoomId), `Should contain ${expectedRoomId} room`);
      }
    });

    it('should create each configured room with correct properties', async () => {
      const handlers = daemon.getRequestHandlers();
      const listRoomsHandler = handlers[CommandOperation.LIST_ROOMS];
      
      const result = await listRoomsHandler({
        correlationId: 'test-correlation',
        timestamp: Date.now()
      });

      // Verify each room from configuration exists with correct properties
      for (const expectedRoom of expectedRooms) {
        const actualRoom = result.rooms.find((room: any) => room.id === expectedRoom.id);
        assert.ok(actualRoom, `Room ${expectedRoom.id} should exist`);
        assert.strictEqual(actualRoom.name, expectedRoom.name, `Room ${expectedRoom.id} should have correct name`);
        assert.strictEqual(actualRoom.type, expectedRoom.type, `Room ${expectedRoom.id} should have correct type`);
        assert.ok(actualRoom.participant_count > 0, `Room ${expectedRoom.id} should have participants`);
      }
    });
  });

  describe('Room Operations', () => {
    it('should allow sending messages to all configured default rooms', async () => {
      const handlers = daemon.getRequestHandlers();
      const sendMessageHandler = handlers[CommandOperation.SEND_MESSAGE];
      const joinRoomHandler = handlers[CommandOperation.JOIN_ROOM];
      
      assert.ok(sendMessageHandler, 'Send message handler should be defined');
      assert.ok(joinRoomHandler, 'Join room handler should be defined');
      
      // Test sending messages to each configured room
      for (const expectedRoom of expectedRooms) {
        // First join the room as test-user
        await joinRoomHandler({
          room_id: expectedRoom.id,
          user_id: 'test-user',
          session_id: 'test-session',
          correlationId: 'test-correlation',
          timestamp: Date.now()
        });
        
        // Then send a message
        const result = await sendMessageHandler({
          room_id: expectedRoom.id,
          sender_id: 'test-user',
          content: `Hello from ${expectedRoom.name}!`,
          correlationId: 'test-correlation',
          timestamp: Date.now()
        });

        assert.strictEqual(result.success, true, `Should successfully send message to ${expectedRoom.name}`);
        assert.ok(result.message_id, `Should have message ID for ${expectedRoom.name}`);
        assert.ok(result.message, `Should have message object for ${expectedRoom.name}`);
        assert.strictEqual(result.message.content, `Hello from ${expectedRoom.name}!`);
        assert.strictEqual(result.message.room_id, expectedRoom.id);
      }
    });

    it('should get room info for all configured default rooms', async () => {
      const handlers = daemon.getRequestHandlers();
      const getRoomInfoHandler = handlers[CommandOperation.GET_ROOM_INFO];
      
      assert.ok(getRoomInfoHandler, 'Get room info handler should be defined');
      
      // Test getting room info for each configured room
      for (const expectedRoom of expectedRooms) {
        const result = await getRoomInfoHandler({
          room_id: expectedRoom.id,
          user_id: 'test-user',
          correlationId: 'test-correlation',
          timestamp: Date.now()
        });

        assert.strictEqual(result.success, true, `Should get room info for ${expectedRoom.name}`);
        assert.ok(result.room, `Should have room object for ${expectedRoom.name}`);
        assert.strictEqual(result.room.id, expectedRoom.id);
        assert.strictEqual(result.room.name, expectedRoom.name);
        assert.ok(result.participants, `Should have participants for ${expectedRoom.name}`);
        assert.ok(Array.isArray(result.participants), `Participants should be array for ${expectedRoom.name}`);
      }
    });

    it('should handle joining existing default rooms', async () => {
      const handlers = daemon.getRequestHandlers();
      const joinRoomHandler = handlers[CommandOperation.JOIN_ROOM];
      
      assert.ok(joinRoomHandler, 'Join room handler should be defined');
      
      // Test joining each configured room (use first room as example)
      const firstRoom = expectedRooms[0];
      const result = await joinRoomHandler({
        room_id: firstRoom.id,
        user_id: 'new-user',
        session_id: 'session-123',
        correlationId: 'test-correlation',
        timestamp: Date.now()
      });

      assert.strictEqual(result.success, true, `Should successfully join ${firstRoom.name}`);
      assert.ok(result.room, `Should have room object for ${firstRoom.name}`);
      assert.strictEqual(result.room.id, firstRoom.id);
      assert.ok(result.participant_count > 1, `Should have multiple participants in ${firstRoom.name}`); // System + new user
    });
  });

  describe('Error Handling', () => {
    it('should handle requests for non-existent rooms gracefully', async () => {
      const handlers = daemon.getRequestHandlers();
      const sendMessageHandler = handlers[CommandOperation.SEND_MESSAGE];
      
      try {
        await sendMessageHandler({
          room_id: 'non-existent-room',
          sender_id: 'test-user',
          content: 'This should fail',
          correlationId: 'test-correlation',
          timestamp: Date.now()
        });
        
        // Should not reach here
        assert.fail('Expected error for non-existent room');
      } catch (error) {
        assert.ok(error, 'Should throw error for non-existent room');
        const errorMessage = error instanceof Error ? error.message : String(error);
        assert.ok(errorMessage.includes('not found'), 'Error should mention room not found');
      }
    });
  });

  describe('Handler Registration', () => {
    it('should register all ChatRoom operation handlers', () => {
      const handlers = daemon.getRequestHandlers();
      
      // Verify all expected handlers are registered
      assert.ok(handlers[CommandOperation.CREATE_ROOM], 'Should have CREATE_ROOM handler');
      assert.ok(handlers[CommandOperation.JOIN_ROOM], 'Should have JOIN_ROOM handler');
      assert.ok(handlers[CommandOperation.LEAVE_ROOM], 'Should have LEAVE_ROOM handler');
      assert.ok(handlers[CommandOperation.SEND_MESSAGE], 'Should have SEND_MESSAGE handler');
      assert.ok(handlers[CommandOperation.GET_MESSAGES], 'Should have GET_MESSAGES handler');
      assert.ok(handlers[CommandOperation.LIST_ROOMS], 'Should have LIST_ROOMS handler');
      assert.ok(handlers[CommandOperation.GET_ROOM_INFO], 'Should have GET_ROOM_INFO handler');
      assert.ok(handlers[CommandOperation.DELETE_ROOM], 'Should have DELETE_ROOM handler');
    });

    it('should register correct number of handlers', () => {
      const handlers = daemon.getRequestHandlers();
      const handlerCount = Object.keys(handlers).length;
      
      // Should have exactly 8 ChatRoom operation handlers
      assert.strictEqual(handlerCount, 8, 'Should have exactly 8 ChatRoom operation handlers');
    });
  });
});