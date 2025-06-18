/**
 * JoinRoomCommand Tests
 * Test suite for Discord-style room joining functionality
 */

const JoinRoomCommand = require('../JoinRoomCommand.cjs');
const CreateRoomCommand = require('../../createroom/CreateRoomCommand.cjs');
const fs = require('fs');
const path = require('path');
const os = require('os');

describe('JoinRoomCommand', () => {
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

    // Create a test room for joining
    await CreateRoomCommand.execute(JSON.stringify({
      name: 'Test Room',
      description: 'A room for testing joins'
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
      const definition = JoinRoomCommand.getDefinition();
      
      expect(definition).toBeDefined();
      expect(definition.name).toBe('JOIN_ROOM');
      expect(definition.category).toBe('Core');
      expect(definition.description).toContain('Join Discord-style room');
      expect(definition.icon).toBe('🚪');
    });

    test('should have proper parameter definition', () => {
      const definition = JoinRoomCommand.getDefinition();
      
      expect(definition.parameters).toBeDefined();
      expect(definition.parameters.roomId).toBeDefined();
      expect(definition.parameters.roomId.required).toBe(true);
      expect(definition.parameters.roomId.type).toBe('string');
    });
  });

  describe('Parameter Validation', () => {
    test('should fail without room ID', async () => {
      const result = await JoinRoomCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('required');
    });

    test('should fail with invalid parameters', async () => {
      const result = await JoinRoomCommand.execute('invalid-json', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Invalid parameters');
    });

    test('should validate room ID format', async () => {
      const result = await JoinRoomCommand.execute('{"roomId": ""}', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Room ID cannot be empty');
    });
  });

  describe('Room Joining', () => {
    test('should join existing room successfully', async () => {
      // Switch to different user
      const joinUser = { ...mockContinuum, userId: 'join-user', username: 'JoinUser' };
      
      const params = { roomId: 'test-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      
      expect(result.success).toBe(true);
      expect(result.data.roomId).toBe('test-room');
      expect(result.data.userId).toBe('join-user');
      expect(result.data.status).toBe('active');
    });

    test('should update subscriptions file', async () => {
      const joinUser = { ...mockContinuum, userId: 'join-user', username: 'JoinUser' };
      const params = { roomId: 'test-room' };
      
      await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      
      const subscriptionsPath = path.join(testDataDir, 'rooms', 'test-room', 'subscriptions.json');
      const subscriptions = JSON.parse(fs.readFileSync(subscriptionsPath, 'utf8'));
      
      expect(subscriptions).toHaveProperty('join-user');
      expect(subscriptions['join-user'].status).toBe('active');
      expect(subscriptions['join-user'].role).toBe('member');
      expect(subscriptions['join-user'].joinedAt).toBeDefined();
    });

    test('should handle user already in room', async () => {
      // Original creator tries to join their own room
      const params = { roomId: 'test-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('already active');
    });

    test('should handle non-existent room', async () => {
      const params = { roomId: 'non-existent-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Room not found');
    });
  });

  describe('Invitation Status', () => {
    test('should allow invited users to join', async () => {
      // Create room with agent invitation
      const inviteUser = { ...mockContinuum, userId: 'invite-user', username: 'InviteUser' };
      
      await CreateRoomCommand.execute(JSON.stringify({
        name: 'Invite Room',
        inviteAgents: ['invite-user']
      }), mockContinuum);

      const params = { roomId: 'invite-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), inviteUser);
      
      expect(result.success).toBe(true);
      expect(result.data.previousStatus).toBe('invited');
      expect(result.data.status).toBe('active');
    });

    test('should update invitation status to active', async () => {
      const inviteUser = { ...mockContinuum, userId: 'invite-user', username: 'InviteUser' };
      
      await CreateRoomCommand.execute(JSON.stringify({
        name: 'Status Room',
        inviteAgents: ['invite-user']
      }), mockContinuum);

      await JoinRoomCommand.execute(JSON.stringify({ roomId: 'status-room' }), inviteUser);
      
      const subscriptionsPath = path.join(testDataDir, 'rooms', 'status-room', 'subscriptions.json');
      const subscriptions = JSON.parse(fs.readFileSync(subscriptionsPath, 'utf8'));
      
      expect(subscriptions['invite-user'].status).toBe('active');
      expect(subscriptions['invite-user'].invitedAt).toBeDefined();
      expect(subscriptions['invite-user'].joinedAt).toBeDefined();
    });
  });

  describe('Join Message', () => {
    test('should add join message to room', async () => {
      const joinUser = { ...mockContinuum, userId: 'join-user', username: 'JoinUser' };
      const params = { roomId: 'test-room' };
      
      await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      
      const messagesPath = path.join(testDataDir, 'rooms', 'test-room', 'messages.json');
      const messages = JSON.parse(fs.readFileSync(messagesPath, 'utf8'));
      
      const joinMessage = messages.find(msg => msg.type === 'system' && msg.action === 'join');
      expect(joinMessage).toBeDefined();
      expect(joinMessage.userId).toBe('join-user');
      expect(joinMessage.username).toBe('JoinUser');
      expect(joinMessage.content).toContain('joined the room');
    });

    test('should not duplicate join messages', async () => {
      const joinUser = { ...mockContinuum, userId: 'join-user', username: 'JoinUser' };
      const params = { roomId: 'test-room' };
      
      // Join twice
      await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      
      const messagesPath = path.join(testDataDir, 'rooms', 'test-room', 'messages.json');
      const messages = JSON.parse(fs.readFileSync(messagesPath, 'utf8'));
      
      const joinMessages = messages.filter(msg => 
        msg.type === 'system' && 
        msg.action === 'join' && 
        msg.userId === 'join-user'
      );
      
      expect(joinMessages).toHaveLength(1);
    });
  });

  describe('Access Control', () => {
    test('should handle private rooms', async () => {
      // Create private room
      await CreateRoomCommand.execute(JSON.stringify({
        name: 'Private Room',
        isPrivate: true
      }), mockContinuum);

      const outsideUser = { ...mockContinuum, userId: 'outside-user', username: 'OutsideUser' };
      const params = { roomId: 'private-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), outsideUser);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('private room');
    });

    test('should allow joining public rooms', async () => {
      // Create public room
      await CreateRoomCommand.execute(JSON.stringify({
        name: 'Public Room',
        isPrivate: false
      }), mockContinuum);

      const outsideUser = { ...mockContinuum, userId: 'outside-user', username: 'OutsideUser' };
      const params = { roomId: 'public-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), outsideUser);
      
      expect(result.success).toBe(true);
    });
  });

  describe('Error Handling', () => {
    test('should handle file system errors', async () => {
      // Create room then make it unwritable
      const roomDir = path.join(testDataDir, 'rooms', 'test-room');
      fs.chmodSync(roomDir, 0o444);
      
      const joinUser = { ...mockContinuum, userId: 'join-user', username: 'JoinUser' };
      const params = { roomId: 'test-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Failed to join room');
      
      // Restore permissions for cleanup
      fs.chmodSync(roomDir, 0o755);
    });

    test('should handle corrupted room files', async () => {
      // Corrupt subscriptions file
      const subscriptionsPath = path.join(testDataDir, 'rooms', 'test-room', 'subscriptions.json');
      fs.writeFileSync(subscriptionsPath, 'invalid-json');
      
      const joinUser = { ...mockContinuum, userId: 'join-user', username: 'JoinUser' };
      const params = { roomId: 'test-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Failed to join room');
    });

    test('should handle missing continuum context', async () => {
      const params = { roomId: 'test-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), null);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Continuum context required');
    });
  });

  describe('Integration with BaseCommand', () => {
    test('should inherit from BaseCommand', () => {
      const BaseCommand = require('../../BaseCommand.cjs');
      expect(JoinRoomCommand.prototype.__proto__.constructor).toBe(BaseCommand);
    });

    test('should use BaseCommand helper methods', async () => {
      const joinUser = { ...mockContinuum, userId: 'join-user', username: 'JoinUser' };
      const params = { roomId: 'test-room' };
      const result = await JoinRoomCommand.execute(JSON.stringify(params), joinUser);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('data');
      expect(result).toHaveProperty('timestamp');
    });
  });
});