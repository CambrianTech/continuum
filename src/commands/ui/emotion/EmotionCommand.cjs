const BaseCommand = require('../../core/BaseCommand.cjs');
const { 
  emotionConfigs, 
  calculateEmotionProperties, 
  shouldReturnHome, 
  isPersistentEmotion 
} = require('./emotionConfigs.cjs');
const emotionDefinition = require('./emotionDefinition.cjs');

/**
 * Emotion Command - Express emotions through continuon animations
 * Makes the AI visual system show feelings and personality
 */
class EmotionCommand extends BaseCommand {
  static getDefinition() {
    return emotionDefinition;
  }

  static async execute(params, context) {
    try {
      const { feeling, intensity = 'medium', duration = 3000, persist = false, target } = this.parseParams(params);

      // Validate emotion
      const validEmotions = [
        // Fleeting emotions (fade automatically)
        'love', 'joy', 'excitement', 'surprised', 'grateful', 'playful', 'amazed',
        // Contextual emotions (fade unless persist=true)
        'thinking', 'curious', 'focused', 'confused', 'sleepy',
        // System states (should usually persist)
        'error', 'warning', 'offline', 'connecting', 'processing', 'success'
      ];

      // Determine if emotion should persist based on type and config
      const shouldPersist = persist || isPersistentEmotion(feeling.toLowerCase());

      if (!validEmotions.includes(feeling.toLowerCase())) {
        return this.createErrorResult(`Unknown emotion: ${feeling}. Available emotions: ${validEmotions.join(', ')}`);
      }

      const emotionType = shouldPersist ? 'persistent' : 'fleeting';
      console.log(`💚 Continuon expressing ${feeling} emotion (${emotionType}) with ${intensity} intensity`);

      if (context && context.webSocketServer) {
        // Get emotion animation configuration
        const emotionConfig = calculateEmotionProperties(feeling, intensity, target);
        emotionConfig.target = target;
        emotionConfig.persistent = shouldPersist;
        emotionConfig.returnToHome = shouldReturnHome(feeling);
        emotionConfig.duration = shouldPersist ? 0 : duration;
        
        // Send emotion command to browser
        context.webSocketServer.broadcast({
          type: 'continuon_emotion',
          emotion: feeling,
          intensity,
          duration: shouldPersist ? 0 : duration, // 0 duration means persistent
          persist: shouldPersist,
          target,
          animation: emotionConfig,
          timestamp: new Date().toISOString()
        });

        const durationText = shouldPersist ? 'persistent until cleared' : `${duration}ms`;
        console.log(`✅ Continuon ${feeling} animation triggered (${durationText})`);
        
        return this.createSuccessResult({
          expressed: true,
          emotion: feeling,
          intensity,
          duration,
          target,
          message: `Continuon expressing ${feeling} with ${intensity} intensity`,
          timestamp: new Date().toISOString()
        });
      } else {
        return this.createErrorResult('No browser connection available for emotion expression');
      }

    } catch (error) {
      console.error('❌ Emotion command failed:', error);
      return this.createErrorResult(`Emotion expression failed: ${error.message}`);
    }
  }

}

module.exports = EmotionCommand;