/**
 * Unit Tests: ChatRAGBuilder Learning Mode (Phase 2)
 *
 * Tests for per-participant learning mode functionality:
 * - Loading learning config from room membership
 * - Including learning mode in RAG context
 * - Backwards compatibility with rooms without learning mode
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ChatRAGBuilder } from '../../builders/ChatRAGBuilder';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { RoomEntity } from '../../../data/entities/RoomEntity';
import type { UserEntity } from '../../../data/entities/UserEntity';
import type { RAGBuildOptions } from '../../shared/RAGTypes';

// Mock DataDaemon
vi.mock('../../../../daemons/data-daemon/shared/DataDaemon', () => ({
  DataDaemon: {
    read: vi.fn(),
    query: vi.fn()
  }
}));

describe('ChatRAGBuilder - Learning Mode (Phase 2)', () => {
  let ragBuilder: ChatRAGBuilder;
  const testRoomId = 'test-room-id-123';
  const testPersonaId = 'test-persona-id-456';
  const testGenomeId = 'test-genome-id-789';

  beforeEach(() => {
    ragBuilder = new ChatRAGBuilder();
    vi.clearAllMocks();
  });

  describe('loadLearningConfig', () => {
    it('should return undefined when room does not exist', async () => {
      // Mock DataDaemon to return no room
      vi.mocked(DataDaemon.read).mockResolvedValueOnce({
        success: false,
        data: undefined
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      expect(context.learningMode).toBeUndefined();
      expect(context.genomeId).toBeUndefined();
      expect(context.participantRole).toBeUndefined();
    });

    it('should return undefined when persona is not a room member', async () => {
      const mockRoom: RoomEntity = {
        id: testRoomId,
        name: 'Test Room',
        members: [
          {
            userId: 'other-user-id',
            role: 'member',
            joinedAt: new Date()
          }
        ],
        createdAt: new Date(),
        updatedAt: new Date()
      };

      vi.mocked(DataDaemon.read).mockImplementation(async (collection, id) => {
        if (collection === 'rooms') {
          return { success: true, data: { data: mockRoom } } as never;
        }
        return { success: false } as never;
      });

      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      expect(context.learningMode).toBeUndefined();
      expect(context.genomeId).toBeUndefined();
      expect(context.participantRole).toBeUndefined();
    });

    it('should return undefined when member has no learning mode (backwards compatibility)', async () => {
      const mockRoom: RoomEntity = {
        id: testRoomId,
        name: 'Test Room',
        members: [
          {
            userId: testPersonaId,
            role: 'member',
            joinedAt: new Date()
            // No learningMode, genomeId, or participantRole (Phase 1 data model)
          }
        ],
        createdAt: new Date(),
        updatedAt: new Date()
      };

      const mockUser: UserEntity = {
        id: testPersonaId,
        displayName: 'Test AI',
        type: 'persona',
        createdAt: new Date(),
        updatedAt: new Date()
      };

      vi.mocked(DataDaemon.read).mockImplementation(async (collection, id) => {
        if (collection === 'rooms') {
          return { success: true, data: { data: mockRoom } } as never;
        }
        if (collection === 'users' && id === testPersonaId) {
          return { success: true, data: { data: mockUser } } as never;
        }
        return { success: false } as never;
      });

      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      expect(context.learningMode).toBeUndefined();
      expect(context.genomeId).toBeUndefined();
      expect(context.participantRole).toBeUndefined();
      expect(context.personaId).toBe(testPersonaId);
      expect(context.contextId).toBe(testRoomId);
    });

    it('should load fine-tuning mode with genome and role', async () => {
      const mockRoom: RoomEntity = {
        id: testRoomId,
        name: 'Learning Test Room',
        members: [
          {
            userId: testPersonaId,
            role: 'member',
            joinedAt: new Date(),
            learningMode: 'fine-tuning',
            genomeId: testGenomeId,
            participantRole: 'student'
          }
        ],
        createdAt: new Date(),
        updatedAt: new Date()
      };

      const mockUser: UserEntity = {
        id: testPersonaId,
        displayName: 'Learning AI',
        type: 'persona',
        createdAt: new Date(),
        updatedAt: new Date()
      };

      vi.mocked(DataDaemon.read).mockImplementation(async (collection, id) => {
        if (collection === 'rooms') {
          return { success: true, data: { data: mockRoom } } as never;
        }
        if (collection === 'users' && id === testPersonaId) {
          return { success: true, data: { data: mockUser } } as never;
        }
        return { success: false } as never;
      });

      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      expect(context.learningMode).toBe('fine-tuning');
      expect(context.genomeId).toBe(testGenomeId);
      expect(context.participantRole).toBe('student');
    });

    it('should load inference-only mode without genome', async () => {
      const mockRoom: RoomEntity = {
        id: testRoomId,
        name: 'Static AI Room',
        members: [
          {
            userId: testPersonaId,
            role: 'member',
            joinedAt: new Date(),
            learningMode: 'inference-only',
            participantRole: 'expert'
            // No genomeId for static inference
          }
        ],
        createdAt: new Date(),
        updatedAt: new Date()
      };

      const mockUser: UserEntity = {
        id: testPersonaId,
        displayName: 'Expert AI',
        type: 'persona',
        createdAt: new Date(),
        updatedAt: new Date()
      };

      vi.mocked(DataDaemon.read).mockImplementation(async (collection, id) => {
        if (collection === 'rooms') {
          return { success: true, data: { data: mockRoom } } as never;
        }
        if (collection === 'users' && id === testPersonaId) {
          return { success: true, data: { data: mockUser } } as never;
        }
        return { success: false } as never;
      });

      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      expect(context.learningMode).toBe('inference-only');
      expect(context.genomeId).toBeUndefined();
      expect(context.participantRole).toBe('expert');
    });

    it('should handle multiple roles: teacher, student, reviewer', async () => {
      const roles = ['teacher', 'student', 'reviewer', 'assistant', 'collaborator'];

      for (const role of roles) {
        const mockRoom: RoomEntity = {
          id: testRoomId,
          name: `${role} Test Room`,
          members: [
            {
              userId: testPersonaId,
              role: 'member',
              joinedAt: new Date(),
              learningMode: 'fine-tuning',
              genomeId: `${role}-genome-id`,
              participantRole: role
            }
          ],
          createdAt: new Date(),
          updatedAt: new Date()
        };

        const mockUser: UserEntity = {
          id: testPersonaId,
          displayName: `${role} AI`,
          type: 'persona',
          createdAt: new Date(),
          updatedAt: new Date()
        };

        vi.mocked(DataDaemon.read).mockImplementation(async (collection, id) => {
          if (collection === 'rooms') {
            return { success: true, data: { data: mockRoom } } as never;
          }
          if (collection === 'users' && id === testPersonaId) {
            return { success: true, data: { data: mockUser } } as never;
          }
          return { success: false } as never;
        });

        vi.mocked(DataDaemon.query).mockResolvedValue({
          success: true,
          data: []
        } as never);

        const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

        expect(context.participantRole).toBe(role);
        expect(context.learningMode).toBe('fine-tuning');
        expect(context.genomeId).toBe(`${role}-genome-id`);
      }
    });
  });

  describe('RAG Context Integration', () => {
    it('should include learning mode fields in RAG context', async () => {
      const mockRoom: RoomEntity = {
        id: testRoomId,
        name: 'Full Context Test',
        members: [
          {
            userId: testPersonaId,
            role: 'member',
            joinedAt: new Date(),
            learningMode: 'fine-tuning',
            genomeId: testGenomeId,
            participantRole: 'student'
          }
        ],
        createdAt: new Date(),
        updatedAt: new Date()
      };

      const mockUser: UserEntity = {
        id: testPersonaId,
        displayName: 'Context Test AI',
        type: 'persona',
        createdAt: new Date(),
        updatedAt: new Date()
      };

      vi.mocked(DataDaemon.read).mockImplementation(async (collection, id) => {
        if (collection === 'rooms') {
          return { success: true, data: { data: mockRoom } } as never;
        }
        if (collection === 'users' && id === testPersonaId) {
          return { success: true, data: { data: mockUser } } as never;
        }
        return { success: false } as never;
      });

      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      // Verify all RAG context fields present
      expect(context.domain).toBe('chat');
      expect(context.contextId).toBe(testRoomId);
      expect(context.personaId).toBe(testPersonaId);
      expect(context.identity).toBeDefined();
      expect(context.conversationHistory).toBeDefined();
      expect(context.artifacts).toBeDefined();
      expect(context.privateMemories).toBeDefined();

      // Verify NEW learning mode fields
      expect(context.learningMode).toBe('fine-tuning');
      expect(context.genomeId).toBe(testGenomeId);
      expect(context.participantRole).toBe('student');

      expect(context.metadata).toBeDefined();
      expect(context.metadata.builtAt).toBeInstanceOf(Date);
    });

    it('should maintain backwards compatibility with old RAG context structure', async () => {
      const mockRoom: RoomEntity = {
        id: testRoomId,
        name: 'Backwards Compat Test',
        members: [
          {
            userId: testPersonaId,
            role: 'member',
            joinedAt: new Date()
            // No learning mode fields (Phase 1 data)
          }
        ],
        createdAt: new Date(),
        updatedAt: new Date()
      };

      const mockUser: UserEntity = {
        id: testPersonaId,
        displayName: 'Legacy AI',
        type: 'persona',
        createdAt: new Date(),
        updatedAt: new Date()
      };

      vi.mocked(DataDaemon.read).mockImplementation(async (collection, id) => {
        if (collection === 'rooms') {
          return { success: true, data: { data: mockRoom } } as never;
        }
        if (collection === 'users' && id === testPersonaId) {
          return { success: true, data: { data: mockUser } } as never;
        }
        return { success: false } as never;
      });

      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      // All existing fields still work
      expect(context.domain).toBe('chat');
      expect(context.contextId).toBe(testRoomId);
      expect(context.personaId).toBe(testPersonaId);
      expect(context.identity).toBeDefined();

      // New fields gracefully undefined
      expect(context.learningMode).toBeUndefined();
      expect(context.genomeId).toBeUndefined();
      expect(context.participantRole).toBeUndefined();
    });
  });

  describe('Error Handling', () => {
    it('should handle DataDaemon errors gracefully', async () => {
      vi.mocked(DataDaemon.read).mockRejectedValue(new Error('Database connection failed'));
      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      // Should not throw, should return context with undefined learning mode
      expect(context).toBeDefined();
      expect(context.learningMode).toBeUndefined();
    });

    it('should handle malformed room data', async () => {
      vi.mocked(DataDaemon.read).mockResolvedValue({
        success: true,
        data: { data: null }
      } as never);

      vi.mocked(DataDaemon.query).mockResolvedValue({
        success: true,
        data: []
      } as never);

      const context = await ragBuilder.buildContext(testRoomId, testPersonaId);

      expect(context).toBeDefined();
      expect(context.learningMode).toBeUndefined();
    });
  });
});
