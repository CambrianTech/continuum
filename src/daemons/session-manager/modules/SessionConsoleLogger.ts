/**
 * Session Console Logger - Captures browser console output to session logs
 * Integrates DevTools Protocol with session management for live logging
 */

import { ChromiumDevToolsAdapter } from '../../browser-manager/adapters/ChromiumDevToolsAdapter';
import { ConsoleMessage } from '../../browser-manager/types/index';
import * as fs from 'fs/promises';

export class SessionConsoleLogger {
  private devTools: ChromiumDevToolsAdapter | null = null;
  private sessionLogPath: string | null = null;
  private isLogging: boolean = false;

  /**
   * Set the session browser log path for console output
   */
  setSessionLogPath(logPath: string): void {
    this.sessionLogPath = logPath;
  }

  /**
   * Connect to browser and start capturing console logs
   */
  async startLogging(debugUrl: string, targetId?: string): Promise<void> {
    if (this.isLogging) {
      return; // Already logging
    }

    this.devTools = new ChromiumDevToolsAdapter();
    
    try {
      // Connect to DevTools
      await this.devTools.connect(debugUrl, targetId);
      
      // Enable console capture with session logging callback
      await this.devTools.enableConsole(this.handleConsoleMessage.bind(this), targetId);
      
      this.isLogging = true;
      console.log(`🔌 Session console logging started: ${debugUrl}`);
      
      // Log the connection to session file
      if (this.sessionLogPath) {
        await this.writeToSessionLog(`[${new Date().toISOString()}] 🔌 DevTools console logging enabled: ${debugUrl}`);
      }
    } catch (error) {
      console.error('❌ Failed to start session console logging:', error);
      throw error;
    }
  }

  /**
   * Stop console logging and disconnect
   */
  async stopLogging(): Promise<void> {
    if (!this.isLogging || !this.devTools) {
      return;
    }

    try {
      await this.devTools.disableConsole();
      await this.devTools.disconnect();
      
      this.isLogging = false;
      console.log('🔌 Session console logging stopped');
      
      // Log the disconnection to session file
      if (this.sessionLogPath) {
        await this.writeToSessionLog(`[${new Date().toISOString()}] 🔌 DevTools console logging disabled`);
      }
    } catch (error) {
      console.error('⚠️ Error stopping session console logging:', error);
    } finally {
      this.devTools = null;
    }
  }

  /**
   * Handle console messages from DevTools and write to session log
   */
  private async handleConsoleMessage(message: ConsoleMessage): Promise<void> {
    if (!this.sessionLogPath) {
      return; // No session log path configured
    }

    try {
      // Format the console message like the git hook does
      const timestamp = new Date(message.timestamp).toISOString();
      const level = message.level.toUpperCase();
      const formattedMessage = `🌐 [${timestamp}] ${level}: ${message.text}`;

      // Write to session browser log
      await this.writeToSessionLog(formattedMessage);

      // Also log to daemon console for debugging
      console.log(`📝 Browser console → session log: [${level}] ${message.text}`);
    } catch (error) {
      console.error('❌ Failed to write console message to session log:', error);
    }
  }

  /**
   * Write message to session browser log file
   */
  private async writeToSessionLog(message: string): Promise<void> {
    if (!this.sessionLogPath) {
      return;
    }

    try {
      await fs.appendFile(this.sessionLogPath, message + '\n');
    } catch (error) {
      console.error(`❌ Session log write failed to ${this.sessionLogPath}:`, error);
    }
  }

  /**
   * Check if console logging is active
   */
  isActive(): boolean {
    return this.isLogging;
  }

  /**
   * Get current session log path
   */
  getSessionLogPath(): string | null {
    return this.sessionLogPath;
  }
}