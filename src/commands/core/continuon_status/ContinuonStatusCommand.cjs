/**
 * ContinuonStatus Command
 * Check current continuon (AI entity) status and emotion state
 */

class ContinuonStatusCommand {
  static getDefinition() {
    return {
      name: 'continuon_status',
      category: 'Core',
      description: 'Check current continuon status, emotion, and display state',
      icon: '🎯',
      params: 'include_browser (optional)',
      usage: 'Get current continuon status including browser display state',
      examples: [
        '{}',
        '{"include_browser": true}'
      ]
    };
  }

  static async execute(params = '{}', continuum = null) {
    try {
      console.log('🎯 ContinuonStatus check requested');
      
      const parsed = JSON.parse(params);
      const { include_browser = false } = parsed;
      
      if (!continuum?.continuonStatus) {
        return {
          success: false,
          error: 'ContinuonStatus not available',
          timestamp: new Date().toISOString()
        };
      }

      // Get server-side status
      const status = continuum.continuonStatus.getStatus();
      const clientCount = continuum.continuonStatus.getClientCount();
      
      const result = {
        success: true,
        data: {
          server: {
            status: status.status,
            emotion: status.emotion,
            emoji: status.emoji,
            connected_clients: clientCount
          },
          browser: null
        },
        timestamp: new Date().toISOString()
      };

      // Optionally include browser display state
      if (include_browser && continuum.webSocketServer) {
        try {
          // Request browser state via WebSocket
          const browserCheckScript = `
            const favicon = document.getElementById('favicon');
            const emotionEl = document.getElementById('continuon-emotion');
            ({
              favicon_href: favicon?.href || 'not found',
              ring_content: emotionEl?.textContent || '',
              ring_display: emotionEl?.style.display || 'unknown',
              ring_exists: !!emotionEl
            })
          `;
          
          result.data.browser = {
            check_requested: true,
            note: 'Use browser_js command for live browser state check',
            script: browserCheckScript.trim()
          };
        } catch (browserError) {
          result.data.browser = {
            error: 'Browser check failed',
            message: browserError.message
          };
        }
      }
      
      return result;
      
    } catch (error) {
      console.error('❌ ContinuonStatus check failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

module.exports = ContinuonStatusCommand;