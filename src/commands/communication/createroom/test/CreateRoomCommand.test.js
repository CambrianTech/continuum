/**
 * CreateRoomCommand Tests
 * Test suite for Discord-style room creation functionality
 */

const CreateRoomCommand = require('../CreateRoomCommand.cjs');
const fs = require('fs');
const path = require('path');
const os = require('os');

describe('CreateRoomCommand', () => {
  let testDataDir;
  let mockContinuum;

  beforeEach(() => {
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
  });

  afterEach(() => {
    // Cleanup test directory
    if (fs.existsSync(testDataDir)) {
      fs.rmSync(testDataDir, { recursive: true, force: true });
    }
  });

  describe('Command Definition', () => {
    test('should have valid command definition', () => {
      const definition = CreateRoomCommand.getDefinition();
      
      expect(definition).toBeDefined();
      expect(definition.name).toBe('CREATE_ROOM');
      expect(definition.category).toBe('Core');
      expect(definition.description).toContain('Discord-style room');
      expect(definition.icon).toBe('🏠');
    });

    test('should have proper parameter definition', () => {
      const definition = CreateRoomCommand.getDefinition();
      
      expect(definition.parameters).toBeDefined();
      expect(definition.parameters.name).toBeDefined();
      expect(definition.parameters.name.required).toBe(true);
      expect(definition.parameters.name.type).toBe('string');
    });
  });

  describe('Parameter Validation', () => {
    test('should fail without room name', async () => {
      const result = await CreateRoomCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('required');
    });

    test('should fail with invalid parameters', async () => {
      const result = await CreateRoomCommand.execute('invalid-json', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Invalid parameters');
    });

    test('should validate room name format', async () => {
      const result = await CreateRoomCommand.execute('{"name": ""}', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Room name cannot be empty');
    });
  });

  describe('Room Name Normalization', () => {
    test('should normalize room names correctly', () => {
      expect(CreateRoomCommand.normalizeRoomName('Test Room')).toBe('test-room');
      expect(CreateRoomCommand.normalizeRoomName('AI Development & Testing')).toBe('ai-development-testing');
      expect(CreateRoomCommand.normalizeRoomName('Multi---Dash')).toBe('multi-dash');
      expect(CreateRoomCommand.normalizeRoomName('  Spaces  ')).toBe('spaces');
      expect(CreateRoomCommand.normalizeRoomName('UPPERCASE')).toBe('uppercase');
    });

    test('should handle special characters', () => {
      expect(CreateRoomCommand.normalizeRoomName('Room@#$%')).toBe('room');
      expect(CreateRoomCommand.normalizeRoomName('Test.Room.Name')).toBe('test-room-name');
      expect(CreateRoomCommand.normalizeRoomName('123-Room')).toBe('123-room');
    });
  });

  describe('Room Creation', () => {
    test('should create room successfully', async () => {
      const params = {
        name: 'Test Room',
        description: 'A test room',
        category: 'general'
      };

      const result = await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.roomId).toBe('test-room');
      expect(result.data.name).toBe('Test Room');
      expect(result.data.normalizedName).toBe('test-room');
    });

    test('should create room directory and files', async () => {
      const params = { name: 'Test Room' };
      
      await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      const roomDir = path.join(testDataDir, 'rooms', 'test-room');
      expect(fs.existsSync(roomDir)).toBe(true);
      expect(fs.existsSync(path.join(roomDir, 'room.json'))).toBe(true);
      expect(fs.existsSync(path.join(roomDir, 'messages.json'))).toBe(true);
      expect(fs.existsSync(path.join(roomDir, 'subscriptions.json'))).toBe(true);
    });

    test('should auto-subscribe creator', async () => {
      const params = { name: 'Test Room' };
      
      await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      const subscriptionsPath = path.join(testDataDir, 'rooms', 'test-room', 'subscriptions.json');
      const subscriptions = JSON.parse(fs.readFileSync(subscriptionsPath, 'utf8'));
      
      expect(subscriptions).toHaveProperty('test-user');
      expect(subscriptions['test-user'].status).toBe('active');
      expect(subscriptions['test-user'].role).toBe('owner');
    });

    test('should handle duplicate room names', async () => {
      const params = { name: 'Duplicate Room' };
      
      // Create first room
      const result1 = await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      expect(result1.success).toBe(true);
      
      // Try to create duplicate
      const result2 = await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      expect(result2.success).toBe(false);
      expect(result2.message).toContain('already exists');
    });
  });

  describe('Agent Invitations', () => {
    test('should invite agents to room', async () => {
      const params = {
        name: 'Agent Room',
        inviteAgents: ['claude', 'gpt-4']
      };

      const result = await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      
      const subscriptionsPath = path.join(testDataDir, 'rooms', 'agent-room', 'subscriptions.json');
      const subscriptions = JSON.parse(fs.readFileSync(subscriptionsPath, 'utf8'));
      
      expect(subscriptions).toHaveProperty('claude');
      expect(subscriptions).toHaveProperty('gpt-4');
      expect(subscriptions['claude'].status).toBe('invited');
      expect(subscriptions['gpt-4'].status).toBe('invited');
    });

    test('should handle invalid agent names', async () => {
      const params = {
        name: 'Invalid Agent Room',
        inviteAgents: ['', null, undefined]
      };

      const result = await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      
      const subscriptionsPath = path.join(testDataDir, 'rooms', 'invalid-agent-room', 'subscriptions.json');
      const subscriptions = JSON.parse(fs.readFileSync(subscriptionsPath, 'utf8'));
      
      // Should only have the creator, invalid agents filtered out
      expect(Object.keys(subscriptions)).toHaveLength(1);
      expect(subscriptions).toHaveProperty('test-user');
    });
  });

  describe('Room Metadata', () => {
    test('should set room metadata correctly', async () => {
      const params = {
        name: 'Metadata Room',
        description: 'Test description',
        category: 'development',
        tags: ['ai', 'testing']
      };

      await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      const roomPath = path.join(testDataDir, 'rooms', 'metadata-room', 'room.json');
      const roomData = JSON.parse(fs.readFileSync(roomPath, 'utf8'));
      
      expect(roomData.name).toBe('Metadata Room');
      expect(roomData.description).toBe('Test description');
      expect(roomData.category).toBe('development');
      expect(roomData.tags).toEqual(['ai', 'testing']);
      expect(roomData.createdBy).toBe('test-user');
      expect(roomData.createdAt).toBeDefined();
    });

    test('should handle empty metadata gracefully', async () => {
      const params = { name: 'Simple Room' };
      
      await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      const roomPath = path.join(testDataDir, 'rooms', 'simple-room', 'room.json');
      const roomData = JSON.parse(fs.readFileSync(roomPath, 'utf8'));
      
      expect(roomData.name).toBe('Simple Room');
      expect(roomData.description).toBe('');
      expect(roomData.category).toBe('general');
      expect(roomData.tags).toEqual([]);
    });
  });

  describe('Error Handling', () => {
    test('should handle file system errors', async () => {
      // Make rooms directory read-only to simulate permission error
      const roomsDir = path.join(testDataDir, 'rooms');
      fs.chmodSync(roomsDir, 0o444);
      
      const params = { name: 'Permission Test' };
      const result = await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Failed to create room');
      
      // Restore permissions for cleanup
      fs.chmodSync(roomsDir, 0o755);
    });

    test('should handle missing continuum context', async () => {
      const params = { name: 'Context Test' };
      const result = await CreateRoomCommand.execute(JSON.stringify(params), null);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Continuum context required');
    });
  });

  describe('Integration with BaseCommand', () => {
    test('should inherit from BaseCommand', () => {
      const BaseCommand = require('../../BaseCommand.cjs');
      expect(CreateRoomCommand.prototype.__proto__.constructor).toBe(BaseCommand);
    });

    test('should use BaseCommand helper methods', async () => {
      const params = { name: 'Helper Test' };
      const result = await CreateRoomCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('data');
      expect(result).toHaveProperty('timestamp');
    });
  });
});