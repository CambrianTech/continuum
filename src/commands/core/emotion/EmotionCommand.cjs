/**
 * Emotion Command
 * Updates continuon (AI entity) emotional state and visual representation
 */

class EmotionCommand {
  static getDefinition() {
    return {
      name: 'emotion',
      category: 'Core',
      description: 'Update continuon emotional state (AI entity mood)',
      icon: '🎭',
      params: 'emotion, emoji (optional), duration (optional)',
      usage: 'Set continuon emotion when system is healthy - shows in favicon and CLI. Duration in ms.',
      examples: [
        '{"emotion": "wink", "duration": 2000}',
        '{"emotion": "excited", "emoji": "🚀"}',
        '{"emotion": "smile", "duration": 5000}',
        '{"emotion": "celebration", "duration": 3000}'
      ]
    };
  }

  static async execute(params = '{}', continuum = null) {
    try {
      console.log('🎭 Emotion command triggered');
      
      const parsed = JSON.parse(params);
      const { emotion, emoji, duration = 0 } = parsed;
      
      if (!emotion) {
        return {
          success: false,
          error: 'Emotion parameter required',
          timestamp: new Date().toISOString()
        };
      }

      if (!continuum?.continuonStatus) {
        return {
          success: false,
          error: 'ContinuonStatus not available',
          timestamp: new Date().toISOString()
        };
      }

      // Update continuon emotion with duration support
      continuum.continuonStatus.updateEmotion(emotion, emoji, duration);
      
      return {
        success: true,
        message: `Continuon emotion updated to: ${emotion}${duration > 0 ? ` for ${duration}ms` : ' permanently'}`,
        data: {
          emotion: emotion,
          emoji: emoji || continuum.continuonStatus.getEmotionEmoji(emotion),
          duration: duration,
          temporary: duration > 0,
          status: continuum.continuonStatus.getStatus()
        },
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.error('❌ Emotion command failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

module.exports = EmotionCommand;