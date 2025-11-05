/**
 * BrowserConsoleDaemon - Modular console capture and forwarding system
 * 
 * Extracted from monolithic continuum-browser.ts (Phase 2 migration)
 * Handles all console capture, forwarding, and queue management
 * 
 * Features:
 * - Intercepts ALL console methods (log, info, warn, error, debug, trace, table, group)
 * - Captures unhandled errors and promise rejections
 * - Deep argument inspection with source location tracking
 * - Intelligent queuing and rate limiting for server forwarding
 * - Session-aware logging with proper sessionId correlation
 * - Fallback to original console methods on errors
 */

import { BaseBrowserDaemon, BrowserDaemonMessage, BrowserDaemonResponse } from '../base/BaseBrowserDaemon';
import { BrowserFeatureFlags } from '../BrowserFeatureFlags';

interface ConsoleCapture {
  type: string;
  timestamp: string;
  level: string;
  args: any[];
  message: string;
  stackTrace: string;
  sourceLocation: string;
  url: string;
  userAgent: string;
  viewport: {
    width: number;
    height: number;
  };
  errorDetails?: {
    name: string;
    message: string;
    stack: string;
    cause?: any;
  };
}

interface ConsoleCommand {
  action: string;
  message: string;
  source: string;
  data: ConsoleCapture;
  sessionId: string | null;
}

export class BrowserConsoleDaemon extends BaseBrowserDaemon {
  private originalConsole: Record<string, any> = {};
  private consoleQueue: ConsoleCommand[] = [];
  private consoleProcessing = false;
  private sessionId: string | null = null;
  private errorCount = 0;
  private isInitialized = false;

  constructor() {
    super('BrowserConsoleDaemon');
  }

  getMessageTypes(): string[] {
    return [
      'console:capture',
      'console:process_queue',
      'console:set_session',
      'console:get_status',
      'console:disable',
      'console:enable'
    ];
  }

  async handleMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse> {
    switch (message.type) {
      case 'console:capture':
        return this.handleCaptureMessage(message);
      
      case 'console:process_queue':
        await this.processConsoleQueue();
        return { 
          success: true, 
          data: { queueLength: this.consoleQueue.length },
          timestamp: new Date().toISOString()
        };
      
      case 'console:set_session':
        this.sessionId = message.data?.sessionId || null;
        this.log(`Session ID updated: ${this.sessionId}`, 'info');
        // Process any queued commands that were waiting for session
        if (this.sessionId) {
          setTimeout(() => this.processConsoleQueue(), 100);
        }
        return { 
          success: true, 
          data: { sessionId: this.sessionId },
          timestamp: new Date().toISOString()
        };
      
      case 'console:get_status':
        return this.getStatus();
      
      case 'console:disable':
        this.disable();
        return { 
          success: true, 
          data: { status: 'disabled' },
          timestamp: new Date().toISOString()
        };
      
      case 'console:enable':
        await this.enable();
        return { 
          success: true, 
          data: { status: 'enabled' },
          timestamp: new Date().toISOString()
        };
      
      default:
        return { 
          success: false, 
          error: `Unknown message type: ${message.type}`,
          timestamp: new Date().toISOString()
        };
    }
  }

  protected async onStart(): Promise<void> {
    if (BrowserFeatureFlags.CONSOLE_DAEMON_ENABLED) {
      await this.enable();
      this.log('Console daemon started and enabled', 'info');
    } else {
      this.log('Console daemon started but disabled by feature flag', 'info');
    }
  }

  protected async onStop(): Promise<void> {
    this.disable();
    this.log('Console daemon stopped', 'info');
  }

  /**
   * Enable console capture system
   */
  private async enable(): Promise<void> {
    if (this.isInitialized) {
      this.log('Console capture already enabled', 'info');
      return;
    }

    try {
      this.setupConsoleCapture();
      this.setupErrorCapture();
      this.isInitialized = true;
      this.log('Console capture system enabled', 'info');
    } catch (error) {
      this.log('Failed to enable console capture', 'error');
      throw error;
    }
  }

  /**
   * Disable console capture and restore original console
   */
  private disable(): void {
    if (!this.isInitialized) {
      return;
    }

    try {
      // Restore original console methods
      Object.keys(this.originalConsole).forEach(method => {
        (console as any)[method] = this.originalConsole[method];
      });

      this.isInitialized = false;
      this.log('Console capture system disabled - restored original console', 'info');
    } catch (error) {
      this.log('Failed to disable console capture', 'error');
    }
  }

  /**
   * Setup console method interception
   */
  private setupConsoleCapture(): void {
    // Store original console methods
    this.originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug,
      trace: console.trace,
      table: console.table,
      group: console.group,
      groupEnd: console.groupEnd
    };

    // Store globally for error recovery
    (window as any).__originalConsole__ = this.originalConsole;

    // Override console methods with capture logic
    console.log = (...args: any[]) => {
      const timestamp = `[${(Date.now() % 100000)/1000}s]`;
      this.originalConsole.log.apply(this.originalConsole, [timestamp, ...args]);
      this.forwardConsoleLog('log', args);
    };

    console.info = (...args: any[]) => {
      this.originalConsole.info.call(this.originalConsole, ...args);
      this.forwardConsoleLog('info', args);
    };

    console.warn = (...args: any[]) => {
      const timestamp = `[${(Date.now() % 100000)/1000}s]`;
      this.originalConsole.warn.call(this.originalConsole, timestamp, ...args);
      this.forwardConsoleLog('warn', args);
    };

    console.error = (...args: any[]) => {
      const timestamp = `[${(Date.now() % 100000)/1000}s]`;
      this.originalConsole.error.call(this.originalConsole, timestamp, ...args);
      this.errorCount++;
      (window as any).continuumErrorCount = this.errorCount;
      this.forwardConsoleLog('error', args);
    };

    console.debug = (...args: any[]) => {
      this.originalConsole.debug.call(this.originalConsole, ...args);
      this.forwardConsoleLog('debug', args);
    };

    console.trace = (...args: any[]) => {
      this.originalConsole.trace.call(this.originalConsole, ...args);
      this.forwardConsoleLog('trace', args);
    };

    console.table = (...args: any[]) => {
      (this.originalConsole.table as any).call(this.originalConsole, ...args);
      this.forwardConsoleLog('table', args);
    };

    console.group = (...args: any[]) => {
      this.originalConsole.group.call(this.originalConsole, ...args);
      this.forwardConsoleLog('group', args);
    };

    console.groupEnd = () => {
      this.originalConsole.groupEnd.call(this.originalConsole);
      this.forwardConsoleLog('groupEnd', []);
    };
  }

  /**
   * Setup error and rejection capture
   */
  private setupErrorCapture(): void {
    // Capture unhandled errors
    window.addEventListener('error', (event) => {
      this.errorCount++;
      (window as any).continuumErrorCount = this.errorCount;
      
      this.forwardConsoleLog('unhandled', [{
        message: event.message,
        filename: event.filename,
        lineno: event.lineno,
        colno: event.colno,
        error: event.error?.stack || event.error
      }]);
    });

    // Capture unhandled promise rejections
    window.addEventListener('unhandledrejection', (event) => {
      this.errorCount++;
      (window as any).continuumErrorCount = this.errorCount;
      
      this.forwardConsoleLog('promise-rejection', [{
        reason: event.reason,
        promise: 'Promise rejection'
      }]);
    });
  }

  /**
   * Forward console log to server
   */
  private forwardConsoleLog(type: string, args: any[]): void {
    try {
      // Create console capture data
      const stackTrace = new Error().stack;
      const sourceLocation = this.getSourceLocation(stackTrace);
      
      const consoleData: ConsoleCapture = {
        type,
        timestamp: new Date().toISOString(),
        level: type,
        
        // Capture full argument data with deep inspection
        args: args.map(arg => this.inspectArgument(arg)),
        
        // Raw string representation (what user sees)
        message: args.map(arg => {
          if (typeof arg === 'string') return arg;
          if (typeof arg === 'number' || typeof arg === 'boolean') return String(arg);
          if (arg === null) return 'null';
          if (arg === undefined) return 'undefined';
          if (typeof arg === 'function') return arg.toString();
          try {
            return JSON.stringify(arg, null, 2);
          } catch {
            return String(arg);
          }
        }).join(' '),
        
        // Stack trace and source location
        stackTrace: stackTrace || 'No stack trace available',
        sourceLocation,
        
        // Browser context
        url: window.location.href,
        userAgent: navigator.userAgent,
        viewport: {
          width: window.innerWidth,
          height: window.innerHeight
        },
        
        // Error-specific data for errors
        ...(type === 'error' && args.length > 0 && args[0] instanceof Error ? {
          errorDetails: {
            name: args[0].name,
            message: args[0].message,
            stack: args[0].stack || 'No stack trace available',
            cause: (args[0] as any).cause
          }
        } : {})
      };

      // Create console command for forwarding
      const consoleCommand: ConsoleCommand = {
        action: type,
        message: consoleData.message,
        source: 'console-complete-capture',
        data: consoleData,
        sessionId: this.sessionId
      };

      // Queue for processing
      this.queueConsoleCommand(consoleCommand);
    } catch (error) {
      // Fail silently to avoid error loops
    }
  }

  /**
   * Deep argument inspection with type analysis
   */
  private inspectArgument(arg: any): any {
    try {
      if (arg === null) return { type: 'null', value: null };
      if (arg === undefined) return { type: 'undefined', value: undefined };
      
      const type = typeof arg;
      
      if (type === 'string' || type === 'number' || type === 'boolean') {
        return { type, value: arg };
      }
      
      if (type === 'function') {
        return { 
          type: 'function', 
          name: arg.name || 'anonymous',
          value: arg.toString(),
          length: arg.length
        };
      }
      
      if (arg instanceof Error) {
        return {
          type: 'Error',
          name: arg.name,
          message: arg.message,
          stack: arg.stack,
          cause: (arg as any).cause
        };
      }
      
      if (Array.isArray(arg)) {
        return {
          type: 'Array',
          length: arg.length,
          value: arg.map(item => this.inspectArgument(item)),
          preview: `Array(${arg.length})`
        };
      }
      
      if (arg instanceof Date) {
        return {
          type: 'Date',
          value: arg.toISOString(),
          timestamp: arg.getTime()
        };
      }
      
      if (arg instanceof Promise) {
        return {
          type: 'Promise',
          state: 'unknown',
          value: '[Promise object]'
        };
      }
      
      if (type === 'object') {
        const keys = Object.keys(arg);
        const result = {
          type: 'Object',
          constructor: arg.constructor?.name || 'Object',
          keys: keys,
          length: keys.length,
          value: {} as any,
          preview: `{${keys.slice(0, 3).join(', ')}${keys.length > 3 ? '...' : ''}}`
        };
        
        // Capture first level of properties (avoid infinite recursion)
        for (const key of keys.slice(0, 10)) {
          try {
            result.value[key] = this.limitDepthInspection(arg[key], 2);
          } catch {
            result.value[key] = '[Inspection failed]';
          }
        }
        
        return result;
      }
      
      return { type, value: String(arg) };
    } catch {
      return { type: 'unknown', value: '[Inspection failed]' };
    }
  }

  /**
   * Limit inspection depth to prevent infinite recursion
   */
  private limitDepthInspection(value: any, maxDepth: number): any {
    if (maxDepth <= 0) return '[Max depth reached]';
    
    if (value === null || value === undefined) return value;
    if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return value;
    if (typeof value === 'function') return '[Function]';
    if (Array.isArray(value)) return value.slice(0, 5).map(item => this.limitDepthInspection(item, maxDepth - 1));
    
    if (typeof value === 'object') {
      const result: any = {};
      const keys = Object.keys(value).slice(0, 5);
      for (const key of keys) {
        result[key] = this.limitDepthInspection(value[key], maxDepth - 1);
      }
      return result;
    }
    
    return String(value);
  }

  /**
   * Extract source location from stack trace
   */
  private getSourceLocation(stackTrace?: string): string {
    if (!stackTrace) return 'Unknown location';
    
    const lines = stackTrace.split('\n');
    // Find the first line that's not this file or console methods
    for (const line of lines) {
      if (line.includes('http') && !line.includes('continuum-browser') && !line.includes('console')) {
        return line.trim();
      }
    }
    
    return lines[2] || 'Unknown location';
  }

  /**
   * Queue console command for processing
   */
  private queueConsoleCommand(consoleCommand: ConsoleCommand): void {
    this.consoleQueue.push(consoleCommand);
    
    if (!this.consoleProcessing) {
      this.processConsoleQueue();
    }
  }

  /**
   * Process console command queue with rate limiting
   */
  private async processConsoleQueue(): Promise<void> {
    if (this.consoleProcessing || this.consoleQueue.length === 0) {
      return;
    }

    this.consoleProcessing = true;

    while (this.consoleQueue.length > 0) {
      const consoleCommand = this.consoleQueue.shift();
      if (!consoleCommand) break;
      
      if (this.sessionId) {
        try {
          // Update the sessionId in the console command to ensure it's current
          consoleCommand.sessionId = this.sessionId;
          
          // Send to server via event bus
          this.emit('console:forward', {
            command: 'console',
            params: consoleCommand
          });
        } catch (error) {
          // Log failed console forwards to original console to avoid loops
          setTimeout(() => {
            const originalConsole = (window as any).__originalConsole__ || console;
            originalConsole.warn('⚠️ Console forward failed:', error);
          }, 0);
        }
      } else {
        // No sessionId yet - keep the console command in queue
        this.consoleQueue.unshift(consoleCommand);
        
        // Log debug message about waiting for sessionId
        const originalConsole = (window as any).__originalConsole__ || console;
        originalConsole.log('🔍 [SESSION_DEBUG] Console command queued - waiting for session_ready message');
        
        // Stop processing until we have sessionId
        break;
      }
      
      // Rate limit: wait 50ms between console commands to prevent overwhelming
      if (this.consoleQueue.length > 0) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }
    }

    this.consoleProcessing = false;
  }

  /**
   * Handle console capture messages
   */
  private async handleCaptureMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse> {
    const { type, args } = message.data || {};
    
    if (!type || !Array.isArray(args)) {
      return { 
        success: false, 
        error: 'Invalid capture message format',
        timestamp: new Date().toISOString()
      };
    }

    // Forward the console log
    this.forwardConsoleLog(type, args);
    
    return { 
      success: true, 
      data: { queued: true, queueLength: this.consoleQueue.length },
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Get console daemon status
   */
  private getStatus(): BrowserDaemonResponse {
    return {
      success: true,
      data: {
        isInitialized: this.isInitialized,
        sessionId: this.sessionId,
        queueLength: this.consoleQueue.length,
        isProcessing: this.consoleProcessing,
        errorCount: this.errorCount,
        featureFlagEnabled: BrowserFeatureFlags.CONSOLE_DAEMON_ENABLED,
        capturedMethods: Object.keys(this.originalConsole)
      },
      timestamp: new Date().toISOString()
    };
  }
}