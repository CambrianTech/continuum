/**
 * Session Console Manager - Manages console logging for sessions
 * 
 * Handles DevTools Protocol integration and console output capture
 * for session-specific browser.log files
 */

import { SessionConsoleLogger } from './SessionConsoleLogger.js';

export interface ConsoleLoggingRequest {
  sessionId: string;
  debugUrl: string;
  targetId?: string;
  logPath: string;
}

export class SessionConsoleManager {
  private loggers = new Map<string, SessionConsoleLogger>(); // sessionId -> logger

  /**
   * Start console logging for a session
   */
  async startLogging(request: ConsoleLoggingRequest): Promise<{ success: boolean; error?: string }> {
    const { sessionId, debugUrl, targetId, logPath } = request;

    try {
      // Check if already logging for this session
      if (this.loggers.has(sessionId)) {
        return { success: true }; // Already active
      }

      // Create and configure logger
      const logger = new SessionConsoleLogger();
      logger.setSessionLogPath(logPath);

      // Start logging
      await logger.startLogging(debugUrl, targetId);
      this.loggers.set(sessionId, logger);

      return { success: true };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: `Failed to start console logging: ${errorMessage}` };
    }
  }

  /**
   * Stop console logging for a session
   */
  async stopLogging(sessionId: string): Promise<{ success: boolean; error?: string }> {
    try {
      const logger = this.loggers.get(sessionId);
      if (!logger) {
        return { success: true }; // Already stopped
      }

      await logger.stopLogging();
      this.loggers.delete(sessionId);

      return { success: true };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: `Failed to stop console logging: ${errorMessage}` };
    }
  }

  /**
   * Stop all console loggers (cleanup)
   */
  async stopAll(): Promise<void> {
    const stopPromises = Array.from(this.loggers.keys()).map(sessionId => 
      this.stopLogging(sessionId)
    );
    await Promise.all(stopPromises);
  }

  /**
   * Check if console logging is active for a session
   */
  isLogging(sessionId: string): boolean {
    return this.loggers.has(sessionId);
  }

  /**
   * Get count of active console loggers
   */
  getActiveCount(): number {
    return this.loggers.size;
  }

  /**
   * Get all active session IDs with console logging
   */
  getActiveSessionIds(): string[] {
    return Array.from(this.loggers.keys());
  }
}