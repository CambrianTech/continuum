/**
 * Emotion Command - TypeScript Implementation
 * Express emotions through continuon animations with full type safety
 */

import { BaseCommand } from '../../core/BaseCommand';
import { 
  EmotionParams, 
  EmotionConfig, 
  EmotionContext, 
  EmotionResult,
  VALID_EMOTIONS,
  ValidEmotion
} from './types';
import { 
  emotionConfigs, 
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
      // Parse and validate parameters with type safety
      const {
        feeling,
        intensity = EmotionCommand.DEFAULT_INTENSITY,
        duration = EmotionCommand.DEFAULT_DURATION,
        persist = false,
        target
      } = this.parseParams(params);

      // Type-safe emotion validation
      if (!this.isValidEmotion(feeling)) {
        return this.createErrorResult(
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
        target,
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

      return this.createSuccessResult({
        emotion: feeling,
        config: emotionConfig,
        timestamp: new Date().toISOString()
      });

    } catch (error) {
      console.error('❌ Emotion Command Error:', error);
      return this.createErrorResult(`Emotion command failed: ${error.message}`);
    }
  }

  /**
   * Type guard for emotion validation
   */
  private static isValidEmotion(feeling: string): feeling is ValidEmotion {
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
   * Create typed success result
   */
  private static createSuccessResult(data: EmotionResult['data']): EmotionResult {
    return {
      success: true,
      message: `Emotion '${data.emotion}' expressed successfully`,
      data
    };
  }

  /**
   * Create typed error result
   */
  private static createErrorResult(error: string): EmotionResult {
    return {
      success: false,
      message: 'Emotion command failed',
      error
    };
  }

  /**
   * Type-safe parameter parsing
   */
  private static parseParams(params: any): EmotionParams {
    if (typeof params === 'string') {
      try {
        return JSON.parse(params) as EmotionParams;
      } catch {
        return { feeling: params };
      }
    }
    return params as EmotionParams;
  }
}

// Default export for easier module loading
export default EmotionCommand;