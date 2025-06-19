/**
 * StatusText Command
 * Update the Continuon status message text
 */

class StatusTextCommand {
  static getDefinition() {
    return {
      name: 'status_text',
      category: 'Core',
      description: 'Update continuon status message text (shows next to ring)',
      icon: '💬',
      params: 'text, duration (optional)',
      usage: 'Set status message that appears next to continuon ring. Duration in ms for temporary messages.',
      examples: [
        '{"text": "Processing screenshots..."}',
        '{"text": "Agent taking control", "duration": 5000}',
        '{"text": "Working on task #42"}',
        '{"text": "Ready for commands"}'
      ]
    };
  }

  static async execute(params = '{}', continuum = null) {
    try {
      console.log('💬 Status text command triggered');
      
      const parsed = JSON.parse(params);
      const { text, duration = 0 } = parsed;
      
      if (!text) {
        return {
          success: false,
          error: 'Text parameter required',
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

      // Update status text
      continuum.continuonStatus.updateStatusText(text);
      
      // Set timeout to revert if duration specified
      if (duration > 0) {
        setTimeout(() => {
          if (continuum.continuonStatus) {
            const defaultText = continuum.continuonStatus.currentStatus === 'connected' ? 'Ready' : 'Disconnected';
            continuum.continuonStatus.updateStatusText(defaultText);
            console.log(`💬 Status text reverted to default after ${duration}ms`);
          }
        }, duration);
      }
      
      return {
        success: true,
        message: `Status text updated: "${text}"${duration > 0 ? ` for ${duration}ms` : ' permanently'}`,
        data: {
          text: text,
          duration: duration,
          temporary: duration > 0,
          status: continuum.continuonStatus.getStatus()
        },
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.error('❌ Status text command failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

module.exports = StatusTextCommand;