/**
 * ChatCommand Tests
 * Test suite for multi-agent conversation orchestration
 */

const ChatCommand = require('../ChatCommand.cjs');
const CreateRoomCommand = require('../../createroom/CreateRoomCommand.cjs');
const JoinRoomCommand = require('../../joinroom/JoinRoomCommand.cjs');
const fs = require('fs');
const path = require('path');
const os = require('os');

describe('ChatCommand', () => {
  let testDataDir;
  let mockContinuum;

  beforeEach(async () => {
    // Create temp directory for testing
    testDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'continuum-test-'));
    
    // Mock continuum context with WebSocket simulation
    mockContinuum = {
      dataDir: testDataDir,
      userId: 'test-user',
      username: 'TestUser',
      broadcast: jest.fn(),
      sendToRoom: jest.fn(),
      agentRegistry: {
        getAgent: jest.fn().mockImplementation((agentId) => ({
          id: agentId,
          name: agentId.charAt(0).toUpperCase() + agentId.slice(1),
          respond: jest.fn().mockResolvedValue({
            success: true,
            response: `Response from ${agentId}`
          })
        }))
      }
    };

    // Ensure rooms directory exists
    const roomsDir = path.join(testDataDir, 'rooms');
    if (!fs.existsSync(roomsDir)) {
      fs.mkdirSync(roomsDir, { recursive: true });
    }

    // Create test room with agents
    await CreateRoomCommand.execute(JSON.stringify({
      name: 'Test Chat Room',
      description: 'Room for chat testing',
      inviteAgents: ['claude', 'gpt-4']
    }), mockContinuum);

    // Have agents join the room
    const claudeUser = { ...mockContinuum, userId: 'claude', username: 'Claude' };
    const gptUser = { ...mockContinuum, userId: 'gpt-4', username: 'GPT-4' };
    
    await JoinRoomCommand.execute(JSON.stringify({ room: 'test-chat-room' }), claudeUser);
    await JoinRoomCommand.execute(JSON.stringify({ room: 'test-chat-room' }), gptUser);
  });

  afterEach(() => {
    // Cleanup test directory
    if (fs.existsSync(testDataDir)) {
      fs.rmSync(testDataDir, { recursive: true, force: true });
    }
    jest.clearAllMocks();
  });

  describe('Command Definition', () => {
    test('should have valid command definition', () => {
      const definition = ChatCommand.getDefinition();
      
      expect(definition).toBeDefined();
      expect(definition.name).toBe('CHAT');
      expect(definition.category).toBe('Core');
      expect(definition.description).toContain('multi-agent conversation');
      expect(definition.icon).toBe('💬');
    });

    test('should have proper parameter definition', () => {
      const definition = ChatCommand.getDefinition();
      
      expect(definition.parameters).toBeDefined();
      expect(definition.parameters.roomId).toBeDefined();
      expect(definition.parameters.message).toBeDefined();
      expect(definition.parameters.roomId.required).toBe(true);
      expect(definition.parameters.message.required).toBe(true);
    });
  });

  describe('Parameter Validation', () => {
    test('should fail without required parameters', async () => {
      const result = await ChatCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('required');
    });

    test('should fail with invalid parameters', async () => {
      const result = await ChatCommand.execute('invalid-json', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Invalid parameters');
    });

    test('should validate empty message', async () => {
      const result = await ChatCommand.execute(JSON.stringify({
        roomId: 'test-chat-room',
        message: ''
      }), mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Message cannot be empty');
    });
  });

  describe('Message Sending', () => {
    test('should send message successfully', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'Hello everyone!'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.messageId).toBeDefined();
      expect(result.data.content).toBe('Hello everyone!');
      expect(result.data.sender).toBe('test-user');
    });

    test('should save message to room file', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'Persistent message test'
      };

      await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      const messagesPath = path.join(testDataDir, 'rooms', 'test-chat-room', 'messages.json');
      const messages = JSON.parse(fs.readFileSync(messagesPath, 'utf8'));
      
      const userMessage = messages.find(msg => 
        msg.content === 'Persistent message test' && 
        msg.sender === 'test-user'
      );
      
      expect(userMessage).toBeDefined();
      expect(userMessage.timestamp).toBeDefined();
      expect(userMessage.type).toBe('user');
    });

    test('should handle non-existent room', async () => {
      const params = {
        roomId: 'non-existent-room',
        message: 'Hello'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Room not found');
    });
  });

  describe('Multi-Agent Orchestration', () => {
    test('should trigger agent responses', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'Hello agents, please respond!',
        triggerAgents: true
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agentsTriggered).toBeDefined();
      expect(result.data.agentsTriggered).toContain('claude');
      expect(result.data.agentsTriggered).toContain('gpt-4');
    });

    test('should provide full conversation context to agents', async () => {
      // Send multiple messages to build context
      await ChatCommand.execute(JSON.stringify({
        roomId: 'test-chat-room',
        message: 'Message 1: Setting up the context'
      }), mockContinuum);

      await ChatCommand.execute(JSON.stringify({
        roomId: 'test-chat-room',
        message: 'Message 2: Building on previous point'
      }), mockContinuum);

      const params = {
        roomId: 'test-chat-room',
        message: 'Message 3: Agents should see all context',
        triggerAgents: true
      };

      await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      // Verify agent was called with context
      expect(mockContinuum.agentRegistry.getAgent).toHaveBeenCalled();
      
      // Check that messages include the conversation history
      const messagesPath = path.join(testDataDir, 'rooms', 'test-chat-room', 'messages.json');
      const messages = JSON.parse(fs.readFileSync(messagesPath, 'utf8'));
      
      expect(messages.length).toBeGreaterThan(3); // Original messages + agent responses
    });

    test('should handle agent response failures gracefully', async () => {
      // Mock agent failure
      mockContinuum.agentRegistry.getAgent.mockImplementation(() => ({
        id: 'failing-agent',
        name: 'Failing Agent',
        respond: jest.fn().mockRejectedValue(new Error('Agent offline'))
      }));

      const params = {
        roomId: 'test-chat-room',
        message: 'Hello failing agent',
        triggerAgents: true
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true); // User message should still succeed
      expect(result.data.agentErrors).toBeDefined();
    });
  });

  describe('Conversation History', () => {
    test('should maintain conversation order', async () => {
      const messages = [
        'First message',
        'Second message', 
        'Third message'
      ];

      for (const message of messages) {
        await ChatCommand.execute(JSON.stringify({
          roomId: 'test-chat-room',
          message
        }), mockContinuum);
        
        // Small delay to ensure timestamp ordering
        await new Promise(resolve => setTimeout(resolve, 10));
      }

      const messagesPath = path.join(testDataDir, 'rooms', 'test-chat-room', 'messages.json');
      const savedMessages = JSON.parse(fs.readFileSync(messagesPath, 'utf8'));
      
      const userMessages = savedMessages
        .filter(msg => msg.type === 'user')
        .sort((a, b) => new Date(a.timestamp) - new Date(b.timestamp));
      
      expect(userMessages[0].content).toContain('First message');
      expect(userMessages[1].content).toContain('Second message');
      expect(userMessages[2].content).toContain('Third message');
    });

    test('should retrieve recent history for context', () => {
      // Create mock room with messages
      const room = {
        messages: [
          { content: 'Old message 1', timestamp: '2024-01-01T00:00:00Z', sender: 'user1' },
          { content: 'Old message 2', timestamp: '2024-01-01T00:01:00Z', sender: 'user2' },
          { content: 'Recent message 1', timestamp: '2024-01-01T00:30:00Z', sender: 'user1' },
          { content: 'Recent message 2', timestamp: '2024-01-01T00:31:00Z', sender: 'user2' },
        ]
      };

      const recentHistory = ChatCommand.getRecentHistory(room, 2);
      
      expect(recentHistory).toHaveLength(2);
      expect(recentHistory[0].content).toBe('Recent message 1');
      expect(recentHistory[1].content).toBe('Recent message 2');
    });
  });

  describe('Room Permissions and Access', () => {
    test('should check room membership', async () => {
      const outsideUser = { ...mockContinuum, userId: 'outside-user', username: 'OutsideUser' };
      const params = {
        roomId: 'test-chat-room',
        message: 'Unauthorized message'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), outsideUser);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('not a member');
    });

    test('should allow room members to chat', async () => {
      const claudeUser = { ...mockContinuum, userId: 'claude', username: 'Claude' };
      const params = {
        roomId: 'test-chat-room',
        message: 'Message from Claude'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), claudeUser);
      
      expect(result.success).toBe(true);
      expect(result.data.sender).toBe('claude');
    });
  });

  describe('Message Types and Metadata', () => {
    test('should handle different message types', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'System announcement',
        type: 'system'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      
      const messagesPath = path.join(testDataDir, 'rooms', 'test-chat-room', 'messages.json');
      const messages = JSON.parse(fs.readFileSync(messagesPath, 'utf8'));
      
      const systemMessage = messages.find(msg => msg.content === 'System announcement');
      expect(systemMessage.type).toBe('system');
    });

    test('should add message metadata', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'Message with metadata',
        metadata: {
          priority: 'high',
          tags: ['important', 'urgent']
        }
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(true);
      
      const messagesPath = path.join(testDataDir, 'rooms', 'test-chat-room', 'messages.json');
      const messages = JSON.parse(fs.readFileSync(messagesPath, 'utf8'));
      
      const metadataMessage = messages.find(msg => msg.content === 'Message with metadata');
      expect(metadataMessage.metadata).toBeDefined();
      expect(metadataMessage.metadata.priority).toBe('high');
    });
  });

  describe('Real-time Broadcasting', () => {
    test('should broadcast message to room subscribers', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'Broadcast test message'
      };

      await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(mockContinuum.sendToRoom).toHaveBeenCalledWith(
        'test-chat-room',
        expect.objectContaining({
          type: 'message',
          content: 'Broadcast test message',
          sender: 'test-user'
        })
      );
    });

    test('should handle broadcast failures', async () => {
      // Mock broadcast failure
      mockContinuum.sendToRoom.mockImplementation(() => {
        throw new Error('WebSocket error');
      });

      const params = {
        roomId: 'test-chat-room',
        message: 'Message with broadcast failure'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      // Message should still be saved even if broadcast fails
      expect(result.success).toBe(true);
    });
  });

  describe('Error Handling', () => {
    test('should handle corrupted message files', async () => {
      // Corrupt messages file
      const messagesPath = path.join(testDataDir, 'rooms', 'test-chat-room', 'messages.json');
      fs.writeFileSync(messagesPath, 'invalid-json');
      
      const params = {
        roomId: 'test-chat-room',
        message: 'Recovery test message'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), params);
      
      // Should handle corruption and continue
      expect(result.success).toBe(false);
    });

    test('should handle file system errors', async () => {
      // Make room directory read-only
      const roomDir = path.join(testDataDir, 'rooms', 'test-chat-room');
      fs.chmodSync(roomDir, 0o444);
      
      const params = {
        roomId: 'test-chat-room',
        message: 'Permission test message'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Failed to send message');
      
      // Restore permissions for cleanup
      fs.chmodSync(roomDir, 0o755);
    });

    test('should handle missing continuum context', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'Context test message'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), null);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Continuum context required');
    });
  });

  describe('Integration with BaseCommand', () => {
    test('should inherit from BaseCommand', () => {
      const BaseCommand = require('../../BaseCommand.cjs');
      expect(ChatCommand.prototype.__proto__.constructor).toBe(BaseCommand);
    });

    test('should use BaseCommand helper methods', async () => {
      const params = {
        roomId: 'test-chat-room',
        message: 'Helper test message'
      };

      const result = await ChatCommand.execute(JSON.stringify(params), mockContinuum);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('data');
      expect(result).toHaveProperty('timestamp');
    });
  });
});