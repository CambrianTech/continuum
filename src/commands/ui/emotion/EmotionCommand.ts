// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Implementing typed parameter execution pattern
/**
 * Emotion Command - TypeScript Implementation
 * Express emotions through continuon animations with full type safety
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { 
  EmotionParams, 
  EmotionConfig, 
  EmotionContext, 
  EmotionResult,
  VALID_EMOTIONS,
  ValidEmotion
} from './types';
import { 
  calculateEmotionProperties, 
  shouldReturnHome, 
  isPersistentEmotion 
} from './emotionConfigs';
import { emotionDefinition } from './emotionDefinition';

export class EmotionCommand extends BaseCommand {
  private static readonly DEFAULT_INTENSITY = 'medium';
  private static readonly DEFAULT_DURATION = 3000;

  static getDefinition() {
    return emotionDefinition;
  }

  static async execute(params: EmotionParams, context?: EmotionContext): Promise<EmotionResult> {
    try {
      // Parameters are automatically parsed by UniversalCommandRegistry
      const {
        feeling,
        intensity = EmotionCommand.DEFAULT_INTENSITY,
        duration = EmotionCommand.DEFAULT_DURATION,
        persist = false,
        target
      } = params;

      // Type-safe emotion validation
      if (!this.isValidEmotion(feeling)) {
        return this.createEmotionErrorResult(
          `Unknown emotion: ${feeling}. Available emotions: ${VALID_EMOTIONS.join(', ')}`
        );
      }

      // Determine persistence with type safety
      const shouldPersist = persist || isPersistentEmotion(feeling as ValidEmotion);
      const emotionType = shouldPersist ? 'persistent' : 'fleeting';
      
      console.log(`💚 Continuon expressing ${feeling} emotion (${emotionType}) with ${intensity} intensity`);

      // Create typed emotion configuration
      const emotionConfig: EmotionConfig = {
        ...calculateEmotionProperties(feeling as ValidEmotion, intensity, target),
        target: target || undefined,
        persistent: shouldPersist,
        returnToHome: shouldReturnHome(feeling as ValidEmotion),
        duration: shouldPersist ? 0 : duration
      };

      // Execute emotion through context
      if (context?.webSocketServer) {
        await this.broadcastEmotion(context.webSocketServer, emotionConfig);
      }

      if (context?.continuonStatus) {
        await this.updateContinuonStatus(context.continuonStatus, feeling as ValidEmotion, emotionConfig);
      }

      return this.createEmotionSuccessResult(
        `Emotion '${feeling}' expressed successfully`,
        {
          emotion: feeling,
          config: emotionConfig,
          timestamp: new Date().toISOString()
        }
      );

    } catch (error) {
      console.error('❌ Emotion Command Error:', error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createEmotionErrorResult(`Emotion command failed: ${errorMessage}`);
    }
  }

  /**
   * Type guard for emotion validation with null safety
   */
  private static isValidEmotion(feeling: string | undefined): feeling is ValidEmotion {
    if (!feeling || typeof feeling !== 'string') {
      return false;
    }
    return VALID_EMOTIONS.includes(feeling.toLowerCase() as ValidEmotion);
  }

  /**
   * Broadcast emotion to all connected clients
   */
  private static async broadcastEmotion(webSocketServer: any, config: EmotionConfig): Promise<void> {
    const emotionMessage = {
      type: 'emotion_update',
      emotion: config,
      timestamp: new Date().toISOString()
    };

    webSocketServer.broadcast(JSON.stringify(emotionMessage));
    console.log(`📡 Emotion broadcasted: ${config.emoji} (${config.animation})`);
  }

  /**
   * Update continuon visual status
   */
  private static async updateContinuonStatus(continuonStatus: any, emotion: ValidEmotion, config: EmotionConfig): Promise<void> {
    if (typeof continuonStatus.setEmotion === 'function') {
      continuonStatus.setEmotion(emotion, config);
      console.log(`🎭 Continuon status updated: ${emotion}`);
    }
  }

  /**
   * Create typed error result using base class
   */
  private static createEmotionErrorResult(message: string): EmotionResult {
    return {
      success: false,
      message: message,
      error: message
    };
  }

  /**
   * Create typed success result using base class
   */
  private static createEmotionSuccessResult(message: string, data: { emotion: string; config: EmotionConfig; timestamp: string; }): EmotionResult {
    return {
      success: true,
      message: message,
      data: data
    };
  }
}

// Default export for easier module loading
export default EmotionCommand;