/**
 * Session Console Logger - Captures browser console output to session logs
 * Integrates DevTools Protocol with session management for live logging
 */

import { ChromiumDevToolsAdapter } from '../../browser-manager/adapters/ChromiumDevToolsAdapter';
import { ConsoleMessage } from '../../browser-manager/types/index';
import { UniversalLogger } from '../../logger/UniversalLogger';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';

export class SessionConsoleLogger {
  private devTools: ChromiumDevToolsAdapter | null = null;
  private isLogging: boolean = false;

  /**
   * Connect to browser and start capturing console logs
   */
  async startLogging(debugUrl: string, targetId?: string, context?: ContinuumContext): Promise<void> {
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
      
      // Log the connection via UniversalLogger
      if (context) {
        UniversalLogger.log('browser', 'SessionConsoleLogger', `🔌 DevTools console logging enabled: ${debugUrl}`, 'info', context);
      }
    } catch (error) {
      console.error('❌ Failed to start session console logging:', error);
      throw error;
    }
  }

  /**
   * Stop console logging and disconnect
   */
  async stopLogging(context?: ContinuumContext): Promise<void> {
    if (!this.isLogging || !this.devTools) {
      return;
    }

    try {
      await this.devTools.disableConsole();
      await this.devTools.disconnect();
      
      this.isLogging = false;
      console.log('🔌 Session console logging stopped');
      
      // Log the disconnection via UniversalLogger
      if (context) {
        UniversalLogger.log('browser', 'SessionConsoleLogger', `🔌 DevTools console logging disabled`, 'info', context);
      }
    } catch (error) {
      console.error('⚠️ Error stopping session console logging:', error);
    } finally {
      this.devTools = null;
    }
  }

  private context?: ContinuumContext;

  /**
   * Set context for logging
   */
  setContext(context: ContinuumContext): void {
    this.context = context;
  }

  /**
   * Handle console messages from DevTools and write to session log
   */
  private async handleConsoleMessage(message: ConsoleMessage): Promise<void> {
    try {
      // Map DevTools console level to UniversalLogger level
      const universalLogLevel = message.level === 'log' ? 'info' : 
                               message.level === 'warn' ? 'warn' :
                               message.level === 'error' ? 'error' :
                               'info';

      // Use UniversalLogger to write to session-specific browser.log
      if (this.context) {
        UniversalLogger.log('browser', 'DevToolsConsole', `🌐 ${message.text}`, universalLogLevel, this.context);
      }

      // Also log to daemon console for debugging
      console.log(`📝 Browser console → session log: [${message.level.toUpperCase()}] ${message.text}`);
    } catch (error) {
      console.error('❌ Failed to write console message to session log:', error);
    }
  }

  /**
   * Check if console logging is active
   */
  isActive(): boolean {
    return this.isLogging;
  }
}