/**
 * Client Console Manager - Unified console management for browser-side logging
 * Part of the symmetric logger daemon architecture
 * Handles console interception, message queuing, and server forwarding
 */

import { Console } from '../../../types/shared/ConsoleTypes';
import type { ContinuumState } from '../../../ui/continuum-browser-client/types/BrowserClientTypes';
import { ContinuumContext, continuumContextFactory } from '../../../types/shared/core/ContinuumTypes';
import { clientLoggerClient } from './ClientLoggerClient';
import { LogLevel, OriginalConsole, ProbeData, ConsoleUtils } from '../shared/LoggerMessageTypes';

export class ClientConsoleManager {
  private consoleForwarding = false;
  private consoleMessageQueue: Console.LogEntry[] = [];
  private originalConsole: OriginalConsole = {} as OriginalConsole;
  private executeCallback?: (command: string, params: Record<string, unknown>) => Promise<unknown>;
  private context: ContinuumContext;

  constructor(private getState: () => ContinuumState, private getSessionId: () => string) {
    // Force context to be non-null - session ID is required
    const sessionId = this.getSessionId();
    if (!sessionId) {
      throw new Error('ClientConsoleManager requires a valid session ID');
    }
    
    this.context = continuumContextFactory.create({ sessionId: sessionId as any });
    this.initializeLoggerDaemon();
    this.enableConsoleForwarding();
  }

  private initializeLoggerDaemon(): void {
    clientLoggerClient.initialize(this.context);
  }

  setExecuteCallback(callback: (command: string, params: Record<string, unknown>) => Promise<unknown>): void {
    this.executeCallback = callback;
  }

  setWebSocketTransport(transport: any): void {
    clientLoggerClient.setWebSocketTransport(transport);
  }

  private enableConsoleForwarding(): void {
    if (this.consoleForwarding) {
      console.warn('⚠️ Console forwarding is already enabled, called twice?');
      return;
    }

    // Store original console methods
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console),
      debug: console.debug ? console.debug.bind(console) : console.log.bind(console),
      trace: console.trace.bind(console)
    };

    // Override console methods
    console.log = (...args: unknown[]): void => {
      this.originalConsole.log(...args);
      this.forwardConsole('log', args);
    };

    console.log('🔄 Enabling console forwarding...');

    console.warn = (...args: unknown[]): void => {
      this.originalConsole.warn(...args);
      this.forwardConsole('warn', args);
    };

    console.error = (...args: unknown[]): void => {
      this.originalConsole.error(...args);
      this.forwardConsole('error', args);
    };

    console.info = (...args: unknown[]): void => {
      this.originalConsole.info(...args);
      this.forwardConsole('info', args);
    };

    console.trace = (...args: unknown[]): void => {
      this.originalConsole.trace(...args);
      this.forwardConsole('trace', args);
    };

    // Add AI-friendly probe method for diagnostic logging with optional JS execution
    (console as any).probe = (messageOrProbeData: string | ProbeData, data?: Record<string, unknown>): void => {
      let probeData: ProbeData;
      
      // Handle both simple string usage and full ProbeData interface
      if (typeof messageOrProbeData === 'string') {
        probeData = {
          message: messageOrProbeData,
          category: 'ai-diagnostic'
        };
        if (data) {
          probeData.data = data;
        }
      } else {
        probeData = messageOrProbeData;
      }

      // Execute optional JavaScript and capture result
      if (probeData.executeJS) {
        try {
          // ESLint disable: eval needed for dynamic probe execution in browser context
          // This is intentional for debugging/testing - probe commands need dynamic JS execution
          // eslint-disable-next-line no-eval
          // @ts-ignore - eval needed for dynamic probe execution in browser context
          const result = eval(probeData.executeJS);
          probeData.data = {
            ...probeData.data,
            executeJSResult: result,
            jsCode: probeData.executeJS  // Keep original for local logging
          };
        } catch (error) {
          probeData.data = {
            ...probeData.data,
            executeJSError: error instanceof Error ? error.message : String(error),
            jsCode: probeData.executeJS  // Keep original for local logging
          };
        }
        
        // Base64 encode for secure wire transmission
        probeData.executeJSBase64 = btoa(probeData.executeJS);
        // Remove plain text version for wire transmission
        delete probeData.executeJS;
      }

      this.originalConsole.log('🔬 PROBE:', probeData.message, probeData.data || '');
      this.forwardConsole('probe', [probeData]);
    };

    this.consoleForwarding = true;
    console.log('✅ Console forwarding enabled');
    console.log('🔬 console.probe() method added for AI diagnostics');
  }

  private forwardConsole(type: string, args: unknown[]): void {
    try {
      // Convert type string to proper Console.Level enum
      const level = type as Console.Level;
      
      // Create properly formatted log entry using shared types
      const logEntry = Console.MessageUtils.createLogEntry(level, args);

      // Add session ID to the log entry (use context)
      const consoleLogEntry: Console.LogEntry = {
        ...logEntry,
        sessionId: this.context.sessionId
      };

      // Forward to logger daemon using unified architecture
      if (clientLoggerClient.isInitialized()) {
        const message = ConsoleUtils.serializeArgs(args);
        clientLoggerClient.log(level as LogLevel, message, { 
          originalArgs: args,
          consoleLogEntry 
        }).catch(error => {
          this.originalConsole.error('Failed to forward to logger daemon:', error);
        });
      }

      // Keep existing behavior for backward compatibility
      // Only execute if we're in ready state, otherwise queue for later
      if (this.getState() === 'ready' && this.executeCallback) {
        this.executeConsoleCommand(consoleLogEntry);
      } else {
        // Queue message for when we reach ready state
        this.queueConsoleMessage(consoleLogEntry);
      }
    } catch (error) {
      // Fail silently to avoid console loops
      this.originalConsole.error('❌ Error forwarding console message:', error);
    }
  }

  // Serialization methods removed - now using unified ConsoleUtils

  private queueConsoleMessage(logEntry: Console.LogEntry): void {
    this.consoleMessageQueue.push(logEntry);
  }

  private executeConsoleCommand(logEntry: Console.LogEntry): void {
    if (this.executeCallback) {
      // Pass the logEntry directly - the console command expects the LogEntry format
      this.executeCallback('console', logEntry as unknown as Record<string, unknown>).catch(error => {
        this.originalConsole.error('❌ Error executing console command:', error);
      });
    }
  }

  /**
   * Process queued messages when ready
   */
  processQueuedMessages(): void {
    if (this.consoleMessageQueue.length === 0) return;

    const messages = [...this.consoleMessageQueue];
    this.consoleMessageQueue = [];

    for (const message of messages) {
      this.executeConsoleCommand(message);
    }
  }

  /**
   * Get current context
   */
  getContext(): ContinuumContext {
    return this.context;
  }

  /**
   * Update context (e.g., when session changes)
   */
  updateContext(newContext: ContinuumContext): void {
    this.context = newContext;
    clientLoggerClient.destroy();
    clientLoggerClient.initialize(this.context);
  }

  /**
   * Clean up console forwarding
   */
  destroy(): void {
    if (this.consoleForwarding) {
      console.log = this.originalConsole.log;
      console.warn = this.originalConsole.warn;
      console.error = this.originalConsole.error;
      console.info = this.originalConsole.info;
      console.trace = this.originalConsole.trace;
      delete (console as any).probe;
      this.consoleForwarding = false;
    }
    
    clientLoggerClient.destroy();
  }
}