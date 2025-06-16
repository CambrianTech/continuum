/**
 * LoadRoomsCommand Tests
 * Test suite for room state management and unread message tracking
 */

const LoadRoomsCommand = require('../LoadRoomsCommand.cjs');
const CreateRoomCommand = require('../../createroom/CreateRoomCommand.cjs');
const ChatCommand = require('../../chat/ChatCommand.cjs');
const fs = require('fs');
const path = require('path');
const os = require('os');

describe('LoadRoomsCommand', () => {
  let testDataDir;
  let mockContinuum;

  beforeEach(async () => {
    // Create temp directory for testing
    testDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'continuum-test-'));
    
    // Mock continuum context
    mockContinuum = {
      dataDir: testDataDir,
      userId: 'test-user',
      username: 'TestUser'
    };

    // Ensure rooms directory exists
    const roomsDir = path.join(testDataDir, 'rooms');
    if (!fs.existsSync(roomsDir)) {
      fs.mkdirSync(roomsDir, { recursive: true });
    }

    // Create test rooms
    await CreateRoomCommand.execute(JSON.stringify({
      name: 'General Chat',
      description: 'Main discussion room'
    }), mockContinuum);

    await CreateRoomCommand.execute(JSON.stringify({
      name: 'Development',
      description: 'Code and dev discussions',
      inviteAgents: ['claude', 'gpt-4']
    }), mockContinuum);

    await CreateRoomCommand.execute(JSON.stringify({
      name: 'Private Team',
      description: 'Internal team discussions',
      isPrivate: true
    }), mockContinuum);
  });

  afterEach(() => {
    // Cleanup test directory
    if (fs.existsSync(testDataDir)) {
      fs.rmSync(testDataDir, { recursive: true, force: true });
    }
  });

  describe('Command Definition', () => {
    test('should have valid command definition', () => {
      const definition = LoadRoomsCommand.getDefinition();
      
      expect(definition).toBeDefined();
      expect(definition.name).toBe('LOAD_ROOMS');
      expect(definition.category).toBe('Core');
      expect(definition.description).toContain('room state management');
      expect(definition.icon).toBe('📋');
    });

    test('should have proper parameter definition', () => {
      const definition = LoadRoomsCommand.getDefinition();
      
      expect(definition.parameters).toBeDefined();
      expect(definition.parameters.userId).toBeDefined();
      expect(definition.parameters.markSeen).toBeDefined();
    });
  });

  describe('Basic Room Loading', () => {
    test('should load all rooms for user', async () => {
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.rooms).toBeDefined();
      expect(result.data.rooms).toHaveLength(3);
    });

    test('should include room metadata', async () => {
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      const room = result.data.rooms.find(r => r.roomId === 'general-chat');
      expect(room).toBeDefined();
      expect(room.name).toBe('General Chat');
      expect(room.description).toBe('Main discussion room');
      expect(room.category).toBe('general');
      expect(room.createdBy).toBe('test-user');
      expect(room.createdAt).toBeDefined();
    });

    test('should show subscription status', async () => {
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      const room = result.data.rooms.find(r => r.roomId === 'general-chat');
      expect(room.subscription).toBeDefined();
      expect(room.subscription.status).toBe('active');
      expect(room.subscription.role).toBe('owner');
      expect(room.subscription.joinedAt).toBeDefined();
    });
  });

  describe('Unread Message Tracking', () => {
    test('should calculate unread messages correctly', async () => {
      // Add some messages to a room
      await ChatCommand.execute(JSON.stringify({
        roomId: 'general-chat',
        message: 'Hello everyone!'
      }), mockContinuum);

      await ChatCommand.execute(JSON.stringify({
        roomId: 'general-chat',
        message: 'How is everyone doing?'
      }), mockContinuum);

      // Load rooms for different user
      const otherUser = { ...mockContinuum, userId: 'other-user', username: 'OtherUser' };
      const result = await LoadRoomsCommand.execute('{}', otherUser);
      
      const room = result.data.rooms.find(r => r.roomId === 'general-chat');
      expect(room.unreadCount).toBeGreaterThan(0);
    });

    test('should track last seen timestamp', async () => {
      // Mark room as seen
      await LoadRoomsCommand.execute(JSON.stringify({
        markSeen: true
      }), mockContinuum);

      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      const room = result.data.rooms.find(r => r.roomId === 'general-chat');
      expect(room.subscription.lastSeenAt).toBeDefined();
      expect(room.unreadCount).toBe(0);
    });

    test('should handle rooms with no messages', async () => {
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      result.data.rooms.forEach(room => {
        expect(room.unreadCount).toBe(0);
        expect(room.lastMessage).toBeNull();
      });
    });
  });

  describe('Room Status and Invitations', () => {
    test('should show invitation status', async () => {
      // Load rooms for invited agent
      const agentUser = { ...mockContinuum, userId: 'claude', username: 'Claude' };
      const result = await LoadRoomsCommand.execute('{}', agentUser);
      
      const room = result.data.rooms.find(r => r.roomId === 'development');
      expect(room).toBeDefined();
      expect(room.subscription.status).toBe('invited');
      expect(room.subscription.invitedAt).toBeDefined();
    });

    test('should show pending invitations count', async () => {
      const agentUser = { ...mockContinuum, userId: 'claude', username: 'Claude' };
      const result = await LoadRoomsCommand.execute('{}', agentUser);
      
      expect(result.data.pendingInvitations).toBeGreaterThan(0);
      
      const invitedRooms = result.data.rooms.filter(r => r.subscription.status === 'invited');
      expect(invitedRooms).toHaveLength(result.data.pendingInvitations);
    });

    test('should filter by subscription status', async () => {
      const result = await LoadRoomsCommand.execute(JSON.stringify({
        status: 'active'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      result.data.rooms.forEach(room => {
        expect(room.subscription.status).toBe('active');
      });
    });
  });

  describe('Room Filtering and Sorting', () => {
    test('should filter by category', async () => {
      const result = await LoadRoomsCommand.execute(JSON.stringify({
        category: 'general'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      result.data.rooms.forEach(room => {
        expect(room.category).toBe('general');
      });
    });

    test('should sort by last activity', async () => {
      // Add message to development room
      await ChatCommand.execute(JSON.stringify({
        roomId: 'development',
        message: 'Latest message'
      }), mockContinuum);

      const result = await LoadRoomsCommand.execute(JSON.stringify({
        sortBy: 'lastActivity'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.rooms[0].roomId).toBe('development');
    });

    test('should limit results', async () => {
      const result = await LoadRoomsCommand.execute(JSON.stringify({
        limit: 2
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.rooms).toHaveLength(2);
    });
  });

  describe('Last Message Information', () => {
    test('should include last message details', async () => {
      // Add a message
      await ChatCommand.execute(JSON.stringify({
        roomId: 'general-chat',
        message: 'This is the last message'
      }), mockContinuum);

      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      const room = result.data.rooms.find(r => r.roomId === 'general-chat');
      expect(room.lastMessage).toBeDefined();
      expect(room.lastMessage.content).toBe('This is the last message');
      expect(room.lastMessage.sender).toBe('test-user');
      expect(room.lastMessage.timestamp).toBeDefined();
    });

    test('should handle system messages', async () => {
      // System messages are created when rooms are created
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      const room = result.data.rooms.find(r => r.roomId === 'general-chat');
      if (room.lastMessage) {
        expect(room.lastMessage).toHaveProperty('type');
        expect(room.lastMessage).toHaveProperty('content');
        expect(room.lastMessage).toHaveProperty('timestamp');
      }
    });
  });

  describe('User Context and Permissions', () => {
    test('should respect user permissions', async () => {
      // Load rooms for user not in private room
      const outsideUser = { ...mockContinuum, userId: 'outside-user', username: 'OutsideUser' };
      const result = await LoadRoomsCommand.execute('{}', outsideUser);
      
      expect(result.success).toBe(true);
      
      // Should not see private room
      const privateRoom = result.data.rooms.find(r => r.roomId === 'private-team');
      expect(privateRoom).toBeUndefined();
    });

    test('should handle different user contexts', async () => {
      const result1 = await LoadRoomsCommand.execute(JSON.stringify({
        userId: 'test-user'
      }), mockContinuum);

      const result2 = await LoadRoomsCommand.execute(JSON.stringify({
        userId: 'different-user'
      }), mockContinuum);
      
      expect(result1.success).toBe(true);
      expect(result2.success).toBe(true);
      expect(result1.data.rooms.length).toBeGreaterThan(result2.data.rooms.length);
    });
  });

  describe('Error Handling', () => {
    test('should handle corrupted room files', async () => {
      // Corrupt a room file
      const roomPath = path.join(testDataDir, 'rooms', 'general-chat', 'room.json');
      fs.writeFileSync(roomPath, 'invalid-json');
      
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(true);
      // Should skip corrupted room and continue with others
      expect(result.data.rooms.length).toBe(2);
    });

    test('should handle missing rooms directory', async () => {
      // Remove rooms directory
      const roomsDir = path.join(testDataDir, 'rooms');
      fs.rmSync(roomsDir, { recursive: true, force: true });
      
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.rooms).toHaveLength(0);
      expect(result.data.totalRooms).toBe(0);
    });

    test('should handle missing continuum context', async () => {
      const result = await LoadRoomsCommand.execute('{}', null);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Continuum context required');
    });
  });

  describe('Performance and Caching', () => {
    test('should handle large number of rooms', async () => {
      // Create many rooms
      for (let i = 0; i < 50; i++) {
        await CreateRoomCommand.execute(JSON.stringify({
          name: `Test Room ${i}`,
          description: `Room number ${i}`
        }), mockContinuum);
      }

      const startTime = Date.now();
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      const endTime = Date.now();
      
      expect(result.success).toBe(true);
      expect(result.data.rooms.length).toBe(53); // 3 original + 50 new
      expect(endTime - startTime).toBeLessThan(5000); // Should complete in under 5 seconds
    });

    test('should efficiently calculate unread counts', async () => {
      // Add many messages
      for (let i = 0; i < 100; i++) {
        await ChatCommand.execute(JSON.stringify({
          roomId: 'general-chat',
          message: `Message ${i}`
        }), mockContinuum);
      }

      const otherUser = { ...mockContinuum, userId: 'other-user', username: 'OtherUser' };
      const result = await LoadRoomsCommand.execute('{}', otherUser);
      
      expect(result.success).toBe(true);
      const room = result.data.rooms.find(r => r.roomId === 'general-chat');
      expect(room.unreadCount).toBe(100);
    });
  });

  describe('Integration with BaseCommand', () => {
    test('should inherit from BaseCommand', () => {
      const BaseCommand = require('../../BaseCommand.cjs');
      expect(LoadRoomsCommand.prototype.__proto__.constructor).toBe(BaseCommand);
    });

    test('should use BaseCommand helper methods', async () => {
      const result = await LoadRoomsCommand.execute('{}', mockContinuum);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('data');
      expect(result).toHaveProperty('timestamp');
    });
  });
});