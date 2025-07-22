/**
 * JTAG Console Daemon
 * 
 * Symmetric daemon that handles console logging, interception, and routing
 * on both client and server. Moves console functionality out of the main JTAG system
 * into its own focused daemon.
 */

import { BaseDaemon, DaemonMessage, DaemonResponse } from './shared/MessageSubscriber';
import { JTAGRouter } from './shared/JTAGRouter';

export interface ConsoleMessage {
  level: 'log' | 'info' | 'warn' | 'error' | 'debug';
  component: string;
  message: string;
  data?: any;
  timestamp: string;
  context: 'client' | 'server';
  stack?: string;
}

export interface ConsoleFilter {
  excludePatterns: string[];
  includeComponents?: string[];
  minLevel?: 'log' | 'info' | 'warn' | 'error' | 'debug';
}

/**
 * Universal Console Handler - works in both client and server contexts
 */
export class ConsoleDaemon extends BaseDaemon<ConsoleMessage> {
  private context: 'client' | 'server';
  private intercepting = false;
  private originalConsole: any = {};
  private filters: ConsoleFilter = { excludePatterns: [] };
  private logBuffer: ConsoleMessage[] = [];
  private maxBufferSize = 1000;

  constructor(context: 'client' | 'server' = 'server') {
    super('console'); // Base endpoint is 'console'
    this.context = context;
    this.setupConsoleInterception();
  }

  async handleMessage(message: DaemonMessage<ConsoleMessage>): Promise<DaemonResponse> {
    try {
      const consoleMessage = message.payload;
      
      // Apply filters
      if (this.shouldFilterMessage(consoleMessage)) {
        return this.createResponse(true, { filtered: true });
      }

      // Add to buffer
      this.addToBuffer(consoleMessage);
      
      // Process the console message based on context
      if (this.context === 'server') {
        await this.processServerConsole(consoleMessage);
      } else {
        await this.processClientConsole(consoleMessage);
      }

      return this.createResponse(true, { 
        processed: true, 
        context: this.context,
        level: consoleMessage.level 
      });

    } catch (error: any) {
      console.error(`❌ ConsoleDaemon[${this.context}]: Error processing message:`, error.message);
      return this.createResponse(false, null, error.message);
    }
  }

  /**
   * Setup console interception to catch all console.* calls
   */
  private setupConsoleInterception(): void {
    if (this.context === 'client' && typeof window !== 'undefined') {
      this.setupClientInterception();
    } else if (this.context === 'server') {
      this.setupServerInterception();
    }
  }

  private setupClientInterception(): void {
    // Save original console methods
    this.originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug
    };

    // Intercept console calls
    ['log', 'info', 'warn', 'error', 'debug'].forEach(level => {
      (console as any)[level] = (...args: any[]) => {
        // Call original first
        (this.originalConsole as any)[level](...args);
        
        // Then process through daemon (avoid infinite loops)
        if (!this.intercepting) {
          this.intercepting = true;
          this.processConsoleCall(level as any, args);
          this.intercepting = false;
        }
      };
    });

    console.log('🎧 ConsoleDaemon[client]: Console interception enabled');
  }

  private setupServerInterception(): void {
    // Server console interception is simpler - just wrap the methods
    this.originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug
    };

    ['log', 'info', 'warn', 'error', 'debug'].forEach(level => {
      (console as any)[level] = (...args: any[]) => {
        // Call original first
        (this.originalConsole as any)[level](...args);
        
        // Process through daemon
        if (!this.intercepting) {
          this.intercepting = true;
          this.processConsoleCall(level as any, args);
          this.intercepting = false;
        }
      };
    });

    console.log('🎧 ConsoleDaemon[server]: Console interception enabled');
  }

  private processConsoleCall(level: ConsoleMessage['level'], args: any[]): void {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');

    // Skip our own daemon messages and JTAG system messages to prevent loops
    const skipPatterns = [
      'ConsoleDaemon', '🎧', '📝', '⚡', '✅', '❌', '📤', '📊',
      'JTAG:', 'CommandProcessor', 'JTAGRouter', 'BaseDaemon',
      'registerWithRouter', 'handleMessage'
    ];
    
    if (skipPatterns.some(pattern => message.includes(pattern))) {
      return;
    }

    const consoleMessage: ConsoleMessage = {
      level,
      component: this.extractComponent(message),
      message,
      timestamp: new Date().toISOString(),
      context: this.context,
      stack: level === 'error' ? new Error().stack : undefined
    };

    // Process the message through the daemon system
    this.addToBuffer(consoleMessage);
    
    // Route through the daemon's handleMessage for consistency
    const daemonMessage: DaemonMessage<ConsoleMessage> = {
      type: 'console',
      payload: consoleMessage,
      timestamp: new Date().toISOString()
    };
    
    // Process asynchronously to avoid blocking console calls
    this.handleMessage(daemonMessage).catch(error => {
      this.originalConsole.error('ConsoleDaemon: Error processing console call:', error);
    });
  }

  private extractComponent(message: string): string {
    // Try to extract component from message patterns
    const patterns = [
      /^\[([^\]]+)\]/,  // [ComponentName]
      /^(\w+):/,        // ComponentName:
      /^🎯\s*(\w+)/,    // 🎯 ComponentName
      /^📝\s*(\w+)/,    // 📝 ComponentName
    ];

    for (const pattern of patterns) {
      const match = message.match(pattern);
      if (match) {
        return match[1];
      }
    }

    return this.context === 'client' ? 'BROWSER_CONSOLE' : 'SERVER_CONSOLE';
  }

  private async processServerConsole(consoleMessage: ConsoleMessage): Promise<void> {
    // Server-specific console processing
    await this.writeToFile(consoleMessage);
    
    // Could also forward to monitoring systems
    if (consoleMessage.level === 'error') {
      await this.notifyErrorMonitoring(consoleMessage);
    }
  }

  private async processClientConsole(consoleMessage: ConsoleMessage): Promise<void> {
    // Client-specific console processing
    
    // Store in browser storage
    this.storeInBrowser(consoleMessage);
    
    // Send to server for centralized logging
    await this.sendToServer(consoleMessage);
  }

  private async writeToFile(consoleMessage: ConsoleMessage): Promise<void> {
    // Write to JTAG log files (server only)
    try {
      const fs = await eval('import("fs/promises")');
      const path = await eval('import("path")');
      
      // Get log directory from package.json config
      const logDir = this.getLogDirectory();
      await fs.mkdir(logDir, { recursive: true });
      
      // Write to both text and JSON files
      const baseName = `${this.context}-console-${consoleMessage.level}`;
      const txtFile = path.join(logDir, `${baseName}.log`);
      const jsonFile = path.join(logDir, `${baseName}.json`);
      
      // Append to text log
      const logLine = `${consoleMessage.timestamp} [${consoleMessage.component}] ${consoleMessage.message}\n`;
      await fs.appendFile(txtFile, logLine);
      
      // Append to JSON log
      const jsonEntry = JSON.stringify(consoleMessage) + '\n';
      await fs.appendFile(jsonFile, jsonEntry);
      
    } catch (error) {
      // Fallback to original console to avoid loops
      this.originalConsole.error('ConsoleDaemon: Failed to write log files:', error);
    }
  }

  private storeInBrowser(consoleMessage: ConsoleMessage): void {
    // Store in browser localStorage/indexedDB
    try {
      const logs = JSON.parse(localStorage.getItem('jtag-console-logs') || '[]');
      logs.push(consoleMessage);
      
      // Keep only last 100 entries
      if (logs.length > 100) {
        logs.splice(0, logs.length - 100);
      }
      
      localStorage.setItem('jtag-console-logs', JSON.stringify(logs));
    } catch (error) {
      // Ignore localStorage errors
    }
  }

  private async sendToServer(consoleMessage: ConsoleMessage): Promise<void> {
    // Send console message to server via router transport system
    try {
      if (typeof window !== 'undefined' && (window as any).jtagClient) {
        // Use existing JTAG client transport
        await (window as any).jtagClient.sendMessage('console', consoleMessage);
      } else {
        // Direct server processing - no transport needed
        await this.processServerConsole(consoleMessage);
      }
    } catch (error) {
      // Fallback to original console to avoid loops
      this.originalConsole.warn('ConsoleDaemon: Failed to send to server:', error);
    }
  }

  private async notifyErrorMonitoring(consoleMessage: ConsoleMessage): Promise<void> {
    // Send errors to monitoring systems
    console.log(`🚨 Error monitoring notification:`, consoleMessage.message);
  }

  private shouldFilterMessage(consoleMessage: ConsoleMessage): boolean {
    // Check exclude patterns
    for (const pattern of this.filters.excludePatterns) {
      if (consoleMessage.message.includes(pattern)) {
        return true;
      }
    }

    // Check include components (if specified)
    if (this.filters.includeComponents && 
        !this.filters.includeComponents.includes(consoleMessage.component)) {
      return true;
    }

    return false;
  }

  private addToBuffer(consoleMessage: ConsoleMessage): void {
    this.logBuffer.push(consoleMessage);
    
    // Maintain buffer size
    if (this.logBuffer.length > this.maxBufferSize) {
      this.logBuffer.shift();
    }
  }

  /**
   * Configure console filters
   */
  setFilters(filters: Partial<ConsoleFilter>): void {
    this.filters = { ...this.filters, ...filters };
    console.log(`🔧 ConsoleDaemon[${this.context}]: Filters updated`, this.filters);
  }

  /**
   * Get recent console messages from buffer
   */
  getRecentLogs(count: number = 50): ConsoleMessage[] {
    return this.logBuffer.slice(-count);
  }

  /**
   * Get log directory from package.json config
   */
  private getLogDirectory(): string {
    try {
      const packageJson = require('../package.json');
      return packageJson.config?.logsPath || '.continuum/jtag/logs';
    } catch (error) {
      // Fallback to default if package.json not found
      return '.continuum/jtag/logs';
    }
  }

  /**
   * Clear the log buffer
   */
  clearBuffer(): void {
    this.logBuffer = [];
    console.log(`🧹 ConsoleDaemon[${this.context}]: Buffer cleared`);
  }

  /**
   * Restore original console (for cleanup)
   */
  restoreConsole(): void {
    if (typeof console !== 'undefined' && this.originalConsole.log) {
      console.log = this.originalConsole.log;
      console.info = this.originalConsole.info;
      console.warn = this.originalConsole.warn;
      console.error = this.originalConsole.error;
      console.debug = this.originalConsole.debug;
      
      console.log(`🔄 ConsoleDaemon[${this.context}]: Console restored`);
    }
  }
}

// Export factory functions for different contexts
export function createServerConsoleDaemon(): ConsoleDaemon {
  return new ConsoleDaemon('server');
}

export function createClientConsoleDaemon(): ConsoleDaemon {
  return new ConsoleDaemon('client');
}