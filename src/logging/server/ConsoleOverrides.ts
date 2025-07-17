/**
 * Server Console Overrides - Server-side console interception
 * Integrates with async logger daemon for non-blocking console operations
 */

import { ContinuumContext, continuumContextFactory } from '../../types/shared/core/ContinuumTypes';
import { SessionContext } from '../../context/SessionContext';
import { ServerAsyncLogger } from './AsyncLogger';
import { LogLevel } from '../shared/LoggingTypes';

export class ServerConsoleOverrides {
  private static instance: ServerConsoleOverrides;
  private logger: ServerAsyncLogger;
  private overridden = false;
  private originalConsole: {
    log: typeof console.log;
    info: typeof console.info;
    warn: typeof console.warn;
    error: typeof console.error;
    debug: typeof console.debug;
  } | null = null;

  private constructor() {
    this.logger = new ServerAsyncLogger();
  }

  static getInstance(): ServerConsoleOverrides {
    if (!ServerConsoleOverrides.instance) {
      ServerConsoleOverrides.instance = new ServerConsoleOverrides();
    }
    return ServerConsoleOverrides.instance;
  }

  async initialize(): Promise<void> {
    await this.logger.initialize();
    this.overrideConsole();
  }

  private overrideConsole(): void {
    if (this.overridden) return;

    // Store original methods
    this.originalConsole = {
      log: console.log.bind(console),
      info: console.info.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      debug: console.debug.bind(console)
    };

    // Override with async versions
    console.log = (...args: any[]) => {
      this.originalConsole!.log(...args);
      this.logAsync('info', args);
    };

    console.info = (...args: any[]) => {
      this.originalConsole!.info(...args);
      this.logAsync('info', args);
    };

    console.warn = (...args: any[]) => {
      this.originalConsole!.warn(...args);
      this.logAsync('warn', args);
    };

    console.error = (...args: any[]) => {
      this.originalConsole!.error(...args);
      this.logAsync('error', args);
    };

    console.debug = (...args: any[]) => {
      this.originalConsole!.debug(...args);
      this.logAsync('debug', args);
    };

    this.overridden = true;
  }

  private logAsync(level: LogLevel, args: any[]): void {
    try {
      const context = this.getContextFromStack();
      const message = this.formatArgs(args);
      const source = this.getSourceFromStack();

      // Fire and forget - don't block console calls
      this.logger.log(context, level, message, source).catch(() => {
        // Silently handle errors to avoid infinite loops
      });
    } catch (error) {
      // Silently handle errors to avoid infinite loops
    }
  }

  private getContextFromStack(): ContinuumContext {
    try {
      const sessionId = SessionContext.getCurrentSessionSync();
      if (sessionId) {
        return continuumContextFactory.create({
          sessionId: sessionId as any, // TODO: Fix UUID type casting
          environment: 'server'
        });
      }
    } catch (error) {
      // Ignore errors
    }

    return continuumContextFactory.create({
      environment: 'server'
    });
  }

  private getSourceFromStack(): string {
    try {
      const context = this.getContextFromStack();
      
      if (context.executionStack && context.executionStack.length > 0) {
        const currentFrame = context.executionStack[context.executionStack.length - 1];
        return currentFrame.location || 'unknown';
      }
    } catch (error) {
      // Ignore errors
    }

    return 'console';
  }

  private formatArgs(args: any[]): string {
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

  restore(): void {
    if (this.originalConsole && this.overridden) {
      console.log = this.originalConsole.log;
      console.info = this.originalConsole.info;
      console.warn = this.originalConsole.warn;
      console.error = this.originalConsole.error;
      console.debug = this.originalConsole.debug;
      this.overridden = false;
    }
  }

  async shutdown(): Promise<void> {
    this.restore();
    await this.logger.shutdown();
  }
}