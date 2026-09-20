/**
 * Unit tests for TrainingDatasetBuilder
 *
 * Tests dataset building from chat conversations for Phase 7 fine-tuning.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { TrainingDatasetBuilder } from '../../system/genome/fine-tuning/server/TrainingDatasetBuilder';
import type { ChatMessageEntity, MessageContent } from '../../system/data/entities/ChatMessageEntity';
import type { TraitType } from '../../system/genome/entities/GenomeLayerEntity';
import type { DataListResult } from '../../daemons/data-daemon/shared/DataTypes';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';

// Mock DataDaemon
vi.mock('../../daemons/data-daemon/shared/DataDaemon', () => ({
  DataDaemon: {
    shared: vi.fn(() => ({
      list: vi.fn()
    }))
  }
}));

describe('TrainingDatasetBuilder', () => {
  let builder: TrainingDatasetBuilder;
  let mockDataDaemon: any;

  const testPersonaId = 'persona-123';
  const testPersonaName = 'Test Persona';
  const testRoomId = 'room-456';
  const testTraitType: TraitType = 'conversational';

  beforeEach(() => {
    builder = new TrainingDatasetBuilder();
    mockDataDaemon = {
      list: vi.fn()
    };
    vi.mocked(DataDaemon.shared).mockReturnValue(mockDataDaemon);
  });

  describe('Configuration', () => {
    it('should use default configuration', () => {
      const builder = new TrainingDatasetBuilder();
      expect(builder).toBeDefined();
    });

    it('should accept custom configuration', () => {
      const builder = new TrainingDatasetBuilder({
        maxMessages: 50,
        minMessages: 5,
        minMessageLength: 20,
        excludeSystemMessages: false,
        includeOwnMessages: false,
        includeOtherPersonas: false
      });
      expect(builder).toBeDefined();
    });

    it('should merge partial configuration with defaults', () => {
      const builder = new TrainingDatasetBuilder({
        maxMessages: 200
      });
      expect(builder).toBeDefined();
    });
  });

  describe('buildFromConversation', () => {
    it('should build dataset from sufficient messages', async () => {
      const messages = createTestMessages(20, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      expect(result.dataset).toBeDefined();
      expect(result.dataset!.examples.length).toBeGreaterThan(0);
      expect(result.dataset!.metadata.personaId).toBe(testPersonaId);
      expect(result.dataset!.metadata.personaName).toBe(testPersonaName);
      expect(result.dataset!.metadata.traitType).toBe(testTraitType);
      expect(result.dataset!.metadata.source).toBe('conversations');
    });

    it('should fail with insufficient messages', async () => {
      const messages = createTestMessages(5, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain('Insufficient messages');
    });

    it('should fail when DataDaemon fails', async () => {
      mockDataDaemon.list.mockResolvedValue({
        success: false,
        items: null
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it('should track statistics', async () => {
      const messages = createTestMessages(20, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.stats).toBeDefined();
      expect(result.stats!.messagesProcessed).toBe(20);
      expect(result.stats!.examplesCreated).toBeGreaterThan(0);
    });
  });

  describe('Sliding Window Strategy', () => {
    it('should create training examples from message windows', async () => {
      const messages = createTestMessages(20, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      // With 20 messages and window size 5, we should have ~16 potential windows
      expect(result.dataset!.examples.length).toBeGreaterThan(0);
      expect(result.dataset!.examples.length).toBeLessThanOrEqual(16);
    });

    it('should only create examples ending with persona response', async () => {
      // Create messages where persona responds every other message
      const messages = createAlternatingMessages(20, testPersonaId, 'user-123', testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      // Each example should end with assistant (persona) message
      result.dataset!.examples.forEach(example => {
        const lastMessage = example.messages[example.messages.length - 1];
        expect(lastMessage.role).toBe('assistant');
      });
    });

    it('should require at least 2 messages per window', async () => {
      // Create messages with many short gaps (filtered out)
      const messages = [
        createMessage('user-1', 'Hi', testRoomId, 1000),
        createMessage(testPersonaId, 'Hi', testRoomId, 1001),
        createMessage('user-2', 'Short', testRoomId, 1002),
        createMessage(testPersonaId, 'Test', testRoomId, 1003)
      ];
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const builder = new TrainingDatasetBuilder({
        minMessages: 2,
        minMessageLength: 5
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      // Some windows will be filtered due to short messages
      expect(result.stats!.messagesFiltered).toBeGreaterThan(0);
    });
  });

  describe('Message Filtering', () => {
    it('should filter messages by length', async () => {
      const messages = [
        createMessage('user-1', 'Hi', testRoomId, 1000),
        createMessage(testPersonaId, 'Short', testRoomId, 1001),
        createMessage('user-2', 'This is a longer message that should pass', testRoomId, 1002),
        createMessage(testPersonaId, 'This is also a longer response', testRoomId, 1003),
        ...createTestMessages(10, testPersonaId, testRoomId, 1004)
      ];
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const builder = new TrainingDatasetBuilder({
        minMessageLength: 20
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      // Short messages should be filtered
      expect(result.stats!.messagesTooShort).toBeGreaterThan(0);
    });

    it('should filter system messages by default', async () => {
      const messages = [
        createMessage('user-1', 'Hello', testRoomId, 1000),
        createSystemMessage('System notification', testRoomId, 1001),
        createMessage(testPersonaId, 'Hi', testRoomId, 1002),
        ...createTestMessages(10, testPersonaId, testRoomId, 1003)
      ];
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      // System message should be filtered out
    });

    it('should include system messages when configured', async () => {
      const messages = [
        createMessage('user-1', 'Hello', testRoomId, 1000),
        createSystemMessage('System notification', testRoomId, 1001),
        createMessage(testPersonaId, 'Hi', testRoomId, 1002),
        ...createTestMessages(10, testPersonaId, testRoomId, 1003)
      ];
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const builder = new TrainingDatasetBuilder({
        excludeSystemMessages: false
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
    });

    it('should handle MessageContent objects', async () => {
      const messages = [
        createMessageWithContent('user-1', { text: 'Hello', attachments: [] } as MessageContent, testRoomId, 1000),
        createMessageWithContent(testPersonaId, { text: 'Hi there', attachments: [] } as MessageContent, testRoomId, 1001),
        ...createTestMessages(10, testPersonaId, testRoomId, 1002)
      ];
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      expect(result.dataset!.examples.length).toBeGreaterThan(0);
    });
  });

  describe('Training Example Format', () => {
    it('should create standard chat completions format', async () => {
      const messages = createAlternatingMessages(20, testPersonaId, 'user-123', testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      const example = result.dataset!.examples[0];
      expect(example.messages).toBeDefined();
      expect(Array.isArray(example.messages)).toBe(true);
      expect(example.messages.length).toBeGreaterThan(1);

      // Check message format
      example.messages.forEach(msg => {
        expect(msg).toHaveProperty('role');
        expect(msg).toHaveProperty('content');
        expect(['user', 'assistant']).toContain(msg.role);
        expect(typeof msg.content).toBe('string');
      });
    });

    it('should assign correct roles', async () => {
      const messages = [
        createMessage('user-1', 'Hello', testRoomId, 1000),
        createMessage('user-2', 'Hi', testRoomId, 1001),
        createMessage(testPersonaId, 'Hello everyone', testRoomId, 1002),
        createMessage('user-1', 'How are you?', testRoomId, 1003),
        createMessage(testPersonaId, 'I am doing well', testRoomId, 1004),
        ...createTestMessages(10, testPersonaId, testRoomId, 1005)
      ];
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      result.dataset!.examples.forEach(example => {
        example.messages.forEach(msg => {
          // Persona messages should be 'assistant', others 'user'
          if (msg.role === 'assistant') {
            // This is the persona's message
            expect(msg.content.length).toBeGreaterThan(0);
          } else {
            // This is a user message
            expect(msg.role).toBe('user');
          }
        });

        // Last message should always be assistant (persona response)
        const lastMsg = example.messages[example.messages.length - 1];
        expect(lastMsg.role).toBe('assistant');
      });
    });

    it('should include metadata', async () => {
      const messages = createTestMessages(20, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      const example = result.dataset!.examples[0];
      expect(example.metadata).toBeDefined();
      expect(example.metadata!.timestamp).toBeDefined();
      expect(example.metadata!.roomId).toBe(testRoomId);
      expect(example.metadata!.confidence).toBe(1.0);
    });
  });

  describe('buildFromMultipleRooms', () => {
    it('should aggregate examples from multiple rooms', async () => {
      const room1Messages = createTestMessages(20, testPersonaId, 'room-1');
      const room2Messages = createTestMessages(20, testPersonaId, 'room-2');

      mockDataDaemon.list
        .mockResolvedValueOnce({ success: true, items: room1Messages })
        .mockResolvedValueOnce({ success: true, items: room2Messages });

      const result = await builder.buildFromMultipleRooms(
        testPersonaId,
        testPersonaName,
        ['room-1', 'room-2'],
        testTraitType
      );

      expect(result.success).toBe(true);
      expect(result.dataset!.examples.length).toBeGreaterThan(0);
      expect(result.stats!.messagesProcessed).toBeGreaterThan(30);
    });

    it('should handle mixed success across rooms', async () => {
      const room1Messages = createTestMessages(20, testPersonaId, 'room-1');

      mockDataDaemon.list
        .mockResolvedValueOnce({ success: true, items: room1Messages })
        .mockResolvedValueOnce({ success: true, items: [] }); // Empty room

      const result = await builder.buildFromMultipleRooms(
        testPersonaId,
        testPersonaName,
        ['room-1', 'room-2'],
        testTraitType
      );

      // Should succeed with room 1 data
      expect(result.success).toBe(true);
      expect(result.dataset!.examples.length).toBeGreaterThan(0);
    });

    it('should fail when no rooms have sufficient data', async () => {
      mockDataDaemon.list
        .mockResolvedValueOnce({ success: true, items: [] })
        .mockResolvedValueOnce({ success: true, items: [] });

      const result = await builder.buildFromMultipleRooms(
        testPersonaId,
        testPersonaName,
        ['room-1', 'room-2'],
        testTraitType
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain('No training examples');
    });
  });

  describe('exportToJSONL', () => {
    it('should export dataset to JSONL format', async () => {
      const messages = createTestMessages(20, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      const jsonl = TrainingDatasetBuilder.exportToJSONL(result.dataset!);

      // JSONL format: one JSON object per line
      const lines = jsonl.split('\n');
      expect(lines.length).toBe(result.dataset!.examples.length);

      // Each line should be valid JSON
      lines.forEach(line => {
        const parsed = JSON.parse(line);
        expect(parsed).toHaveProperty('messages');
        expect(Array.isArray(parsed.messages)).toBe(true);
      });
    });

    it('should be compatible with OpenAI format', async () => {
      const messages = createTestMessages(20, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      const jsonl = TrainingDatasetBuilder.exportToJSONL(result.dataset!);
      const lines = jsonl.split('\n');

      lines.forEach(line => {
        const parsed = JSON.parse(line);
        expect(parsed.messages).toBeDefined();
        parsed.messages.forEach((msg: any) => {
          expect(['system', 'user', 'assistant']).toContain(msg.role);
          expect(typeof msg.content).toBe('string');
        });
      });
    });
  });

  describe('validateDataset', () => {
    it('should validate sufficient examples', async () => {
      const messages = createTestMessages(50, testPersonaId, testRoomId);
      mockDataDaemon.list.mockResolvedValue({
        success: true,
        items: messages
      });

      const result = await builder.buildFromConversation(
        testPersonaId,
        testPersonaName,
        testRoomId,
        testTraitType
      );

      expect(result.success).toBe(true);
      const validation = TrainingDatasetBuilder.validateDataset(result.dataset!);

      expect(validation.valid).toBe(true);
      expect(validation.errors.length).toBe(0);
    });

    it('should warn about low example count', () => {
      const dataset = {
        examples: [
          {
            messages: [
              { role: 'user', content: 'Hello' },
              { role: 'assistant', content: 'Hi' }
            ]
          }
        ],
        metadata: {
          personaId: testPersonaId,
          personaName: testPersonaName,
          traitType: testTraitType,
          createdAt: Date.now(),
          source: 'conversations' as const,
          totalExamples: 1
        }
      };

      const validation = TrainingDatasetBuilder.validateDataset(dataset);
      expect(validation.warnings.length).toBeGreaterThan(0);
      expect(validation.warnings[0]).toContain('Low example count');
    });

    it('should error on missing assistant responses', () => {
      const dataset = {
        examples: [
          {
            messages: [
              { role: 'user', content: 'Hello' },
              { role: 'user', content: 'Anyone there?' }
            ]
          }
        ],
        metadata: {
          personaId: testPersonaId,
          personaName: testPersonaName,
          traitType: testTraitType,
          createdAt: Date.now(),
          source: 'conversations' as const,
          totalExamples: 1
        }
      };

      const validation = TrainingDatasetBuilder.validateDataset(dataset);
      expect(validation.valid).toBe(false);
      expect(validation.errors.length).toBeGreaterThan(0);
      expect(validation.errors[0]).toContain('No assistant response');
    });

    it('should error on too few messages per example', () => {
      const dataset = {
        examples: [
          {
            messages: [
              { role: 'assistant', content: 'Hi' }
            ]
          }
        ],
        metadata: {
          personaId: testPersonaId,
          personaName: testPersonaName,
          traitType: testTraitType,
          createdAt: Date.now(),
          source: 'conversations' as const,
          totalExamples: 1
        }
      };

      const validation = TrainingDatasetBuilder.validateDataset(dataset);
      expect(validation.valid).toBe(false);
      expect(validation.errors.length).toBeGreaterThan(0);
      expect(validation.errors[0]).toContain('Too few messages');
    });

    it('should warn about short messages', () => {
      const dataset = {
        examples: [
          {
            messages: [
              { role: 'user', content: 'Hi' },
              { role: 'assistant', content: 'Hi' }
            ]
          }
        ],
        metadata: {
          personaId: testPersonaId,
          personaName: testPersonaName,
          traitType: testTraitType,
          createdAt: Date.now(),
          source: 'conversations' as const,
          totalExamples: 1
        }
      };

      const validation = TrainingDatasetBuilder.validateDataset(dataset);
      expect(validation.warnings.length).toBeGreaterThan(0);
      expect(validation.warnings.some(w => w.includes('very short messages'))).toBe(true);
    });
  });
});

// Helper functions

function createTestMessages(count: number, personaId: string, roomId: string, startTime = 1000): ChatMessageEntity[] {
  const messages: ChatMessageEntity[] = [];
  for (let i = 0; i < count; i++) {
    const isPersona = i % 2 === 0;
    messages.push(createMessage(
      isPersona ? personaId : `user-${i}`,
      `Test message ${i} with enough length to pass filtering`,
      roomId,
      startTime + i * 1000
    ));
  }
  return messages;
}

function createAlternatingMessages(count: number, personaId: string, userId: string, roomId: string): ChatMessageEntity[] {
  const messages: ChatMessageEntity[] = [];
  for (let i = 0; i < count; i++) {
    const isUser = i % 2 === 0;
    messages.push(createMessage(
      isUser ? userId : personaId,
      `Message ${i} with sufficient length for training`,
      roomId,
      1000 + i * 1000
    ));
  }
  return messages;
}

function createMessage(senderId: string, content: string, roomId: string, timestamp: number): ChatMessageEntity {
  return {
    id: `msg-${timestamp}-${Math.random()}`,
    senderId,
    roomId,
    content,
    createdAt: new Date(timestamp),
    metadata: {}
  } as ChatMessageEntity;
}

function createMessageWithContent(senderId: string, content: MessageContent, roomId: string, timestamp: number): ChatMessageEntity {
  return {
    id: `msg-${timestamp}-${Math.random()}`,
    senderId,
    roomId,
    content,
    createdAt: new Date(timestamp),
    metadata: {}
  } as ChatMessageEntity;
}

function createSystemMessage(content: string, roomId: string, timestamp: number): ChatMessageEntity {
  return {
    id: `msg-${timestamp}-${Math.random()}`,
    senderId: 'system',
    roomId,
    content,
    createdAt: new Date(timestamp),
    metadata: { source: 'system' }
  } as ChatMessageEntity;
}
