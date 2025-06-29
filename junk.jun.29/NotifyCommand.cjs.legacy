/**
 * Notify Command
 * Send clean status notifications via continuon instead of system alerts
 */

class NotifyCommand {
  static getDefinition() {
    return {
      name: 'notify',
      category: 'Core',
      description: 'Send clean status notification via continuon (replaces system alerts)',
      icon: '📢',
      params: 'message, type (optional), duration (optional)',
      usage: 'Display notification through continuon status system. Type: info, success, warning, error.',
      examples: [
        '{"message": "Tab redirected successfully"}',
        '{"message": "Connection established", "type": "success", "duration": 3000}',
        '{"message": "Processing request...", "type": "info"}',
        '{"message": "Error occurred", "type": "error", "duration": 5000}'
      ]
    };
  }

  static async execute(params = '{}', continuum = null) {
    try {
      console.log('📢 Notify command triggered');
      
      const parsed = JSON.parse(params);
      const { message, type = 'info', duration = 4000 } = parsed;
      
      if (!message) {
        return {
          success: false,
          error: 'Message parameter required',
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

      // Set appropriate emoji based on notification type
      let emoji = '💬';
      let emotion = null;
      
      switch (type) {
        case 'success':
          emoji = '✅';
          emotion = 'success';
          break;
        case 'warning':
          emoji = '⚠️';
          emotion = 'warning';
          break;
        case 'error':
          emoji = '❌';
          emotion = 'error';
          break;
        case 'info':
        default:
          emoji = '💬';
          break;
      }

      // Update continuon with notification
      if (emotion) {
        continuum.continuonStatus.updateEmotion(emotion, emoji, duration);
      }
      
      // Update status text with the message
      continuum.continuonStatus.updateStatusText(message);
      
      // Set timeout to revert status text if duration specified
      if (duration > 0) {
        setTimeout(() => {
          if (continuum.continuonStatus) {
            const defaultText = continuum.continuonStatus.currentStatus === 'connected' ? 'Ready' : 'Disconnected';
            continuum.continuonStatus.updateStatusText(defaultText);
            console.log(`📢 Notification cleared after ${duration}ms`);
          }
        }, duration);
      }
      
      return {
        success: true,
        message: `Notification sent: "${message}" (${type})`,
        data: {
          message: message,
          type: type,
          emoji: emoji,
          emotion: emotion,
          duration: duration,
          temporary: duration > 0,
          status: continuum.continuonStatus.getStatus()
        },
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.error('❌ Notify command failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

module.exports = NotifyCommand;