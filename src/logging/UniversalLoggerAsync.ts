/**
 * Universal Logger Async - Drop-in replacement for UniversalLogger using async queue
 * Maintains same API but uses ProcessBasedDaemon for performance
 * Uses stack-based context for routing at every process level
 */

import { ContinuumContext, ContinuumEnvironment, continuumContextFactory } from '../types/shared/core/ContinuumTypes';
import { SessionContext } from '../context/SessionContext';
import { AsyncLogger } from './AsyncLogger';
import { LogLevel } from '../daemons/logger/shared/LoggerMessageTypes';

export class UniversalLoggerAsync {
  private static initialized = false;
  private static consoleOverridden = false;
  static originalConsole: {
    log: typeof console.log;
    info: typeof console.info;
    warn: typeof console.warn;
    error: typeof console.error;
    debug: typeof console.debug;
  } | null = null;

  static async init() {
    if (!this.initialized) {
      try {
        // Initialize the async logger daemon
        await AsyncLogger.initialize();
        this.initialized = true;
      } catch (error) {
        console.error('Failed to initialize UniversalLoggerAsync:', error);
      }
    }
  }

  /**
   * Override console methods to route to async logger
   * Uses stack-based context from execution stack
   */
  static overrideConsole() {
    if (this.consoleOverridden) {
      return;
    }

    // Store original console methods
    this.originalConsole = {
      log: console.log.bind(console),
      info: console.info.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      debug: console.debug.bind(console)
    };

    // Override console methods - call original + route through async logger
    console.log = (...args: any[]) => {
      this.originalConsole!.log(...args);
      this.writeConsoleLogAsync('info', args);
    };

    console.info = (...args: any[]) => {
      this.originalConsole!.info(...args);
      this.writeConsoleLogAsync('info', args);
    };

    console.warn = (...args: any[]) => {
      this.originalConsole!.warn(...args);
      this.writeConsoleLogAsync('warn', args);
    };

    console.error = (...args: any[]) => {
      this.originalConsole!.error(...args);
      this.writeConsoleLogAsync('error', args);
    };

    console.debug = (...args: any[]) => {
      this.originalConsole!.debug(...args);
      this.writeConsoleLogAsync('debug', args);
    };

    this.consoleOverridden = true;
  }

  /**
   * Write console log through async logger using stack-based context
   */
  private static writeConsoleLogAsync(level: LogLevel, args: any[]) {
    // Get context from stack or create default
    const context = this.getContextFromStack();
    const message = this.formatConsoleArgs(args);
    const source = this.getSourceFromStack();

    // Fire and forget - don't wait for async logging
    AsyncLogger.log(context, level, message, source).catch(error => {
      // Fallback to original console if async fails
      console.error('Async console logging failed:', error);
    });
  }

  /**
   * Main logging method - now uses async logger
   */
  static async log(
    _name: ContinuumEnvironment, 
    source: string, 
    message: string, 
    level: 'info' | 'warn' | 'error' | 'debug' = 'info', 
    context: ContinuumContext
  ) {
    await this.init();
    
    // Use the provided context with stack-based routing
    const enrichedContext = this.enrichContextWithStack(context);
    
    // Route through async logger
    await AsyncLogger.log(enrichedContext, level as LogLevel, message, source);
  }

  /**
   * Get context from execution stack or create default
   */
  private static getContextFromStack(): ContinuumContext {
    try {
      // Try to get current context from SessionContext
      const sessionId = SessionContext.getCurrentSessionSync();
      if (sessionId) {
        return continuumContextFactory.create({
          sessionId: sessionId as any, // TODO: Fix UUID type casting
          environment: 'server' // Default environment
        });
      }
    } catch (error) {
      // Ignore errors getting context
    }

    // Create default context
    return continuumContextFactory.create({
      environment: 'server'
    });
  }

  /**
   * Get source from execution stack
   */
  private static getSourceFromStack(): string {
    try {
      const context = this.getContextFromStack();
      
      // Use execution stack to determine source
      if (context.executionStack && context.executionStack.length > 0) {
        const currentFrame = context.executionStack[context.executionStack.length - 1];
        return currentFrame.location || 'unknown';
      }
    } catch (error) {
      // Ignore errors getting source
    }

    return 'console';
  }

  /**
   * Enrich context with current execution stack information
   */
  private static enrichContextWithStack(context: ContinuumContext): ContinuumContext {
    try {
      // Get current execution stack and merge with provided context
      const stackContext = this.getContextFromStack();
      
      return {
        ...context,
        executionStack: [
          ...(context.executionStack || []),
          ...(stackContext.executionStack || [])
        ]
      };
    } catch (error) {
      // Return original context if enrichment fails
      return context;
    }
  }

  /**
   * Format console arguments to string
   */
  private static formatConsoleArgs(args: any[]): string {
    return args.map(arg => {
      if (typeof arg === 'string') {
        return arg;
      } else if (typeof arg === 'object' && arg !== null) {
        try {
          return JSON.stringify(arg);
        } catch (error) {
          return String(arg);
        }
      } else {
        return String(arg);
      }
    }).join(' ');
  }

  /**
   * Restore original console methods
   */
  static restoreConsole() {
    if (this.originalConsole && this.consoleOverridden) {
      console.log = this.originalConsole.log;
      console.info = this.originalConsole.info;
      console.warn = this.originalConsole.warn;
      console.error = this.originalConsole.error;
      console.debug = this.originalConsole.debug;
      this.consoleOverridden = false;
    }
  }

  /**
   * Shutdown the async logger
   */
  static async shutdown() {
    await AsyncLogger.shutdown();
    this.restoreConsole();
    this.initialized = false;
  }

  /**
   * Force flush all pending logs
   */
  static async flush(sessionId?: string) {
    await AsyncLogger.flush(sessionId);
  }
}

// Initialize on module load
UniversalLoggerAsync.init().catch(console.error);