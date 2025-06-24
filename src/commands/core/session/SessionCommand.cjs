/**
 * Session Command - Low-level OS-like session management for Continuum
 * 
 * Provides unified session management across all Continuum systems:
 * - verification/ (git hook sessions)
 * - portal/ (AI portal sessions) 
 * - personas/ (persona execution sessions)
 * - sentinels/ (sentinel monitoring sessions)
 * - devtools/ (DevTools automation sessions)
 */

const SessionManager = require('./SessionManager.cjs');

class SessionCommand {
  static getDefinition() {
    return {
      name: 'session',
      category: 'Core', 
      icon: '📁',
      description: 'Unified session management system',
      params: '<action> [options]',
      examples: [
        '{"action": "create", "type": "portal", "runId": "screenshot_test", "metadata": {"user": "claude"}}',
        '{"action": "complete", "type": "portal", "runId": "screenshot_test", "results": {"success": true, "summary": "Screenshot captured"}}',
        '{"action": "read", "type": "verification", "runId": "latest", "artifact": "client-logs"}',
        '{"action": "write", "type": "portal", "runId": "latest", "artifact": "debug-info", "content": "Debug message"}',
        '{"action": "list", "type": "verification", "limit": 5}',
        '{"action": "path", "type": "portal", "runId": "latest"}',
        '{"action": "migrate", "legacyDir": "verification"}'
      ],
      usage: 'Low-level session management - create organized session directories with latest symlinks'
    };
  }

  static async execute(params, continuum) {
    console.log('📁 Session management command executed with params:', params);
    
    try {
      // Parse parameters
      let config;
      if (typeof params === 'string') {
        try {
          config = JSON.parse(params);
        } catch (e) {
          return {
            success: false,
            error: 'Invalid JSON parameters',
            usage: 'Use JSON format: {"action": "create", "type": "portal", "runId": "test123"}'
          };
        }
      } else {
        config = params;
      }

      if (!config.action) {
        return {
          success: false,
          error: 'Missing required parameter: action',
          availableActions: ['create', 'complete', 'read', 'write', 'list', 'path', 'latest', 'migrate']
        };
      }

      // Initialize SessionManager using server's base directory
      const baseDir = continuum.baseDir || process.cwd();
      const sessionManager = SessionManager.createForContinuum(baseDir);
      
      // Execute session command
      const result = await sessionManager.executeSessionCommand(config.action, config);
      
      // Format response based on action
      switch (config.action) {
        case 'create':
          return {
            success: true,
            action: 'create',
            sessionPath: result,
            message: `Session created: sessions/${config.type}/run_${config.runId}/`,
            timestamp: new Date().toISOString()
          };

        case 'complete':
          return {
            success: true,
            action: 'complete', 
            sessionPath: result,
            message: `Session completed: sessions/${config.type}/run_${config.runId}/`,
            latestPath: `sessions/${config.type}/latest/`,
            timestamp: new Date().toISOString()
          };

        case 'read':
          return {
            success: true,
            action: 'read',
            sessionType: config.type,
            runId: config.runId,
            artifact: config.artifact,
            content: result,
            contentLength: typeof result === 'string' ? result.length : result.byteLength
          };

        case 'write':
          return {
            success: true,
            action: 'write',
            artifactPath: result,
            message: `Artifact written: sessions/${config.type}/run_${config.runId}/${config.artifact}`
          };

        case 'list':
          return {
            success: true,
            action: 'list',
            sessionType: config.type,
            sessions: result,
            count: result.length
          };

        case 'path':
          return {
            success: true,
            action: 'path',
            sessionType: config.type,
            runId: config.runId,
            path: result
          };

        case 'latest':
          return {
            success: true,
            action: 'latest',
            sessionType: config.type,
            latestPath: result
          };

        case 'migrate':
          return {
            success: true,
            action: 'migrate',
            message: 'Legacy verification data migrated to new session structure'
          };

        default:
          return {
            success: false,
            error: `Unknown action: ${config.action}`,
            availableActions: ['create', 'complete', 'read', 'write', 'list', 'path', 'latest', 'migrate']
          };
      }
      
    } catch (error) {
      console.error('❌ Session command error:', error);
      return {
        success: false,
        error: error.message,
        stack: error.stack
      };
    }
  }
}

module.exports = SessionCommand;