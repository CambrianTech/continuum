/**
 * Browser Console Forwarder - Helper to forward browser console messages to LoggerDaemon
 * This replaces the console handling logic in WebSocketDaemon
 */

import { DAEMON_REGISTRY } from '../../base/DaemonRegistry';
import { BrowserConsoleMessage, LogLevel } from '../shared/LoggerMessageTypes';

export class BrowserConsoleForwarder {
  /**
   * Forward browser console message to LoggerDaemon
   * This is called by WebSocketDaemon when it receives console messages
   */
  static async forwardConsoleMessage(
    connectionId: string,
    sessionId: string | undefined,
    consoleData: {
      level?: string;
      message?: string;
      data?: any;
      timestamp?: string;
    }
  ): Promise<void> {
    try {
      // Get LoggerDaemon from registry
      const loggerDaemon = DAEMON_REGISTRY.findDaemon('logger');
      if (!loggerDaemon) {
        console.warn('⚠️ LoggerDaemon not found in registry - cannot forward console message');
        return;
      }

      // Normalize log level
      const level = BrowserConsoleForwarder.normalizeLogLevel(consoleData.level);
      
      // Create browser console message
      const browserConsoleMessage: BrowserConsoleMessage = {
        connectionId,
        sessionId: sessionId || '',
        level,
        message: consoleData.message || JSON.stringify(consoleData.data),
        data: consoleData.data,
        timestamp: consoleData.timestamp || new Date().toISOString()
      };

      // Send to LoggerDaemon (using public API since we're external)
      console.log('📝 Browser console message forwarded to LoggerDaemon:', browserConsoleMessage);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Error forwarding console message: ${errorMessage}`);
    }
  }

  /**
   * Normalize log level from browser console to LogLevel enum
   */
  private static normalizeLogLevel(level?: string): LogLevel {
    if (!level) return 'info';
    
    const normalized = level.toLowerCase();
    switch (normalized) {
      case 'error': return 'error';
      case 'warn': case 'warning': return 'warn';
      case 'info': return 'info';
      case 'debug': return 'debug';
      case 'log': return 'info';
      default: return 'info';
    }
  }
}