/**
 * Recipe System Integration Tests
 *
 * Tests that recipes are loaded from JSON files and integrated into RAG context
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { RecipeLoader } from '../../system/recipes/server/RecipeLoader';
import { ChatRAGBuilder } from '../../system/rag/builders/ChatRAGBuilder';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import { RoomEntity } from '../../system/data/entities/RoomEntity';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { RecipeDefinition } from '../../system/recipes/shared/RecipeTypes';

describe('Recipe System Integration', () => {
  let testRoomId: UUID;

  beforeAll(async () => {
    // Create a test room with recipeId
    const room = new RoomEntity();
    room.uniqueId = 'test-recipe-room';
    room.name = 'test-recipe-room';
    room.displayName = 'Test Recipe Room';
    room.description = 'Test room for recipe integration';
    room.type = 'public';
    room.status = 'active';
    room.ownerId = 'test-user-id' as UUID;
    room.recipeId = 'general-chat';
    room.privacy = {
      isPublic: true,
      requiresInvite: false,
      allowGuestAccess: true,
      searchable: true
    };
    room.settings = {
      allowThreads: true,
      allowReactions: true,
      allowFileSharing: true,
      messageRetentionDays: 365,
      slowMode: 0
    };
    room.members = [];
    room.tags = ['test'];

    const result = await DataDaemon.create(RoomEntity.collection, room);
    if (!result.success || !result.data) {
      throw new Error('Failed to create test room');
    }
    testRoomId = result.data.id as UUID;
  });

  describe('RecipeLoader', () => {
    it('should load general-chat recipe from JSON file', async () => {
      const loader = RecipeLoader.getInstance();
      const recipe = await loader.loadRecipe('general-chat');

      expect(recipe).toBeDefined();
      expect(recipe?.uniqueId).toBe('general-chat');
      expect(recipe?.displayName).toBe('General Chat');
      expect(recipe?.strategy).toBeDefined();
      expect(recipe?.strategy.conversationPattern).toBe('collaborative');
      expect(recipe?.strategy.responseRules).toBeInstanceOf(Array);
      expect(recipe?.strategy.responseRules.length).toBeGreaterThan(0);
    });

    it('should return null for non-existent recipe', async () => {
      const loader = RecipeLoader.getInstance();
      const recipe = await loader.loadRecipe('non-existent-recipe');

      expect(recipe).toBeNull();
    });

    it('should cache recipes for performance', async () => {
      const loader = RecipeLoader.getInstance();

      // First load
      const recipe1 = await loader.loadRecipe('general-chat');

      // Second load (should be cached)
      const recipe2 = await loader.loadRecipe('general-chat');

      expect(recipe1).toBe(recipe2); // Same object reference = cached
    });
  });

  describe('ChatRAGBuilder Recipe Integration', () => {
    it('should load recipe strategy for room with recipeId', async () => {
      const ragBuilder = new ChatRAGBuilder();
      const ragContext = await ragBuilder.buildContext(
        testRoomId,
        'test-persona-id' as UUID,
        { maxMessages: 5, includeArtifacts: false, includeMemories: false }
      );

      expect(ragContext.recipeStrategy).toBeDefined();
      expect(ragContext.recipeStrategy?.conversationPattern).toBe('collaborative');
      expect(ragContext.recipeStrategy?.responseRules).toBeInstanceOf(Array);
      expect(ragContext.recipeStrategy?.decisionCriteria).toBeInstanceOf(Array);
    });

    it('should include recipe metadata in RAG context', async () => {
      const ragBuilder = new ChatRAGBuilder();
      const ragContext = await ragBuilder.buildContext(
        testRoomId,
        'test-persona-id' as UUID,
        { maxMessages: 5 }
      );

      expect(ragContext.metadata.recipeId).toBe('collaborative');
      expect(ragContext.metadata.recipeName).toContain('collaborative conversation');
    });

    it('should handle rooms without recipeId gracefully', async () => {
      // Create room without recipeId
      const room = new RoomEntity();
      room.uniqueId = 'no-recipe-room';
      room.name = 'no-recipe-room';
      room.displayName = 'No Recipe Room';
      room.description = 'Room without recipe';
      room.type = 'public';
      room.status = 'active';
      room.ownerId = 'test-user-id' as UUID;
      // NO recipeId set
      room.privacy = {
        isPublic: true,
        requiresInvite: false,
        allowGuestAccess: true,
        searchable: true
      };
      room.settings = {
        allowThreads: true,
        allowReactions: true,
        allowFileSharing: true,
        messageRetentionDays: 365,
        slowMode: 0
      };
      room.members = [];
      room.tags = ['test'];

      const result = await DataDaemon.create(RoomEntity.collection, room);
      if (!result.success || !result.data) {
        throw new Error('Failed to create no-recipe room');
      }
      const noRecipeRoomId = result.data.id as UUID;

      const ragBuilder = new ChatRAGBuilder();
      const ragContext = await ragBuilder.buildContext(
        noRecipeRoomId,
        'test-persona-id' as UUID,
        { maxMessages: 5 }
      );

      // Should work without errors, just no recipe strategy
      expect(ragContext.recipeStrategy).toBeUndefined();
    });
  });

  describe('Recipe Strategy Content', () => {
    it('should have collaborative response rules', async () => {
      const loader = RecipeLoader.getInstance();
      const recipe = await loader.loadRecipe('general-chat');

      const rules = recipe?.strategy.responseRules || [];

      // Check for key collaborative rules
      const hasMultipleResponseRule = rules.some(rule =>
        rule.toLowerCase().includes('multiple') && rule.toLowerCase().includes('response')
      );
      const hasQuestionRule = rules.some(rule =>
        rule.toLowerCase().includes('question')
      );

      expect(hasMultipleResponseRule).toBe(true);
      expect(hasQuestionRule).toBe(true);
    });

    it('should have decision criteria', async () => {
      const loader = RecipeLoader.getInstance();
      const recipe = await loader.loadRecipe('general-chat');

      const criteria = recipe?.strategy.decisionCriteria || [];

      expect(criteria.length).toBeGreaterThan(0);

      // Check for key decision criteria
      const hasKnowledgeCheck = criteria.some(c =>
        c.toLowerCase().includes('knowledge') || c.toLowerCase().includes('insight')
      );
      const hasValueCheck = criteria.some(c =>
        c.toLowerCase().includes('value') || c.toLowerCase().includes('add')
      );

      expect(hasKnowledgeCheck).toBe(true);
      expect(hasValueCheck).toBe(true);
    });
  });
});
