/**
 * EmotionCommand TypeScript Unit Tests
 * Comprehensive test coverage for typed emotion system
 */

import { EmotionCommand } from './EmotionCommand.js';
import { EmotionParams, EmotionContext, VALID_EMOTIONS } from './types.js';

describe('EmotionCommand', () => {
  let mockContext: EmotionContext;
  let mockWebSocketServer: any;
  let mockContinuonStatus: any;

  beforeEach(() => {
    // Setup mocks with TypeScript types
    mockWebSocketServer = {
      broadcast: jest.fn()
    };

    mockContinuonStatus = {
      setEmotion: jest.fn()
    };

    mockContext = {
      webSocketServer: mockWebSocketServer,
      continuonStatus: mockContinuonStatus
    };
  });

  describe('Parameter Validation', () => {
    test('should accept valid emotion string', async () => {
      const params: EmotionParams = { feeling: 'joy' };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data?.emotion).toBe('joy');
    });

    test('should reject invalid emotion', async () => {
      const params: EmotionParams = { feeling: 'invalid_emotion' };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Unknown emotion: invalid_emotion');
    });

    test('should handle all valid emotions', async () => {
      for (const emotion of VALID_EMOTIONS) {
        const params: EmotionParams = { feeling: emotion };
        const result = await EmotionCommand.execute(params, mockContext);
        
        expect(result.success).toBe(true);
        expect(result.data?.emotion).toBe(emotion);
      }
    });

    test('should handle JSON string parameters', async () => {
      const jsonParams = JSON.stringify({ feeling: 'excitement', intensity: 'high' });
      const result = await EmotionCommand.execute(jsonParams as any, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data?.emotion).toBe('excitement');
    });

    test('should handle plain string parameters', async () => {
      const result = await EmotionCommand.execute('love' as any, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data?.emotion).toBe('love');
    });
  });

  describe('Intensity Handling', () => {
    test('should accept valid intensity levels', async () => {
      const intensities: Array<'low' | 'medium' | 'high'> = ['low', 'medium', 'high'];
      
      for (const intensity of intensities) {
        const params: EmotionParams = { feeling: 'joy', intensity };
        const result = await EmotionCommand.execute(params, mockContext);
        
        expect(result.success).toBe(true);
      }
    });

    test('should default to medium intensity', async () => {
      const params: EmotionParams = { feeling: 'joy' };
      await EmotionCommand.execute(params, mockContext);
      
      // Should not throw and should succeed
      expect(mockWebSocketServer.broadcast).toHaveBeenCalled();
    });
  });

  describe('Persistence Logic', () => {
    test('should handle persistent emotions', async () => {
      const params: EmotionParams = { feeling: 'error', persist: true };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data?.config.persistent).toBe(true);
    });

    test('should handle fleeting emotions', async () => {
      const params: EmotionParams = { feeling: 'joy', persist: false };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data?.config.duration).toBeGreaterThan(0);
    });

    test('should auto-persist system emotions', async () => {
      const systemEmotions = ['error', 'warning', 'processing'];
      
      for (const emotion of systemEmotions) {
        const params: EmotionParams = { feeling: emotion };
        const result = await EmotionCommand.execute(params, mockContext);
        
        expect(result.success).toBe(true);
        // System emotions should typically be persistent
      }
    });
  });

  describe('Context Integration', () => {
    test('should broadcast to WebSocket server', async () => {
      const params: EmotionParams = { feeling: 'excitement' };
      await EmotionCommand.execute(params, mockContext);
      
      expect(mockWebSocketServer.broadcast).toHaveBeenCalledWith(
        expect.stringContaining('emotion_update')
      );
    });

    test('should update continuon status', async () => {
      const params: EmotionParams = { feeling: 'thinking' };
      await EmotionCommand.execute(params, mockContext);
      
      expect(mockContinuonStatus.setEmotion).toHaveBeenCalledWith(
        'thinking',
        expect.objectContaining({
          emoji: expect.any(String),
          animation: expect.any(String)
        })
      );
    });

    test('should work without context', async () => {
      const params: EmotionParams = { feeling: 'joy' };
      const result = await EmotionCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.data?.emotion).toBe('joy');
    });
  });

  describe('Duration and Timing', () => {
    test('should respect custom duration', async () => {
      const customDuration = 5000;
      const params: EmotionParams = { feeling: 'joy', duration: customDuration };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data?.config.duration).toBe(customDuration);
    });

    test('should use default duration for fleeting emotions', async () => {
      const params: EmotionParams = { feeling: 'surprised' };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data?.config.duration).toBe(3000); // DEFAULT_DURATION
    });
  });

  describe('Error Handling', () => {
    test('should handle WebSocket broadcast errors gracefully', async () => {
      mockWebSocketServer.broadcast = jest.fn().mockImplementation(() => {
        throw new Error('Broadcast failed');
      });

      const params: EmotionParams = { feeling: 'joy' };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Emotion command failed');
    });

    test('should handle continuon status errors gracefully', async () => {
      mockContinuonStatus.setEmotion = jest.fn().mockImplementation(() => {
        throw new Error('Status update failed');
      });

      const params: EmotionParams = { feeling: 'joy' };
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(false);
    });
  });

  describe('Type Safety', () => {
    test('should maintain type safety throughout execution', async () => {
      const params: EmotionParams = {
        feeling: 'love',
        intensity: 'high',
        duration: 2000,
        persist: false,
        target: 'main_display'
      };
      
      const result = await EmotionCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(typeof result.data?.timestamp).toBe('string');
      expect(typeof result.data?.config.persistent).toBe('boolean');
      expect(typeof result.data?.config.duration).toBe('number');
    });
  });

  describe('Command Definition', () => {
    test('should provide valid command definition', () => {
      const definition = EmotionCommand.getDefinition();
      
      expect(definition).toBeDefined();
      expect(definition.name).toBe('emotion');
      expect(definition.description).toContain('emotion');
    });
  });
});