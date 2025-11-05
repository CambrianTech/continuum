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

    // Store original console methods for internal use
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console),
      debug: console.debug ? console.debug.bind(console) : console.log.bind(console),
      trace: console.trace.bind(console)
    };

    // CONSOLE OVERRIDE REMOVED: LoggerDaemon is now "The Console Daemon"
    // ClientConsoleManager only handles message forwarding, not console override
    // Console override is handled by LoggerDaemon with semaphore protection

    console.log('🔄 Console forwarding initialized (no console override)');

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
          
          // If result is a JSON string, parse it back to an object for proper logging
          let processedResult = result;
          if (typeof result === 'string') {
            try {
              processedResult = JSON.parse(result);
            } catch (error) {
              // If it's not valid JSON, keep as string
              processedResult = result;
            }
          }
          
          probeData.data = {
            ...probeData.data,
            executeJSResult: processedResult,
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
    console.log('✅ Console forwarding enabled (without console override)');
    console.log('🔬 console.probe() method added for AI diagnostics');
  }

  /**
   * Forward console messages to server (called by probe or manual forwarding)
   */
  private forwardConsole(type: string, args: unknown[]): void {
    try {
      // Convert type string to proper Console.Level enum
      const level = type as Console.Level;
      
      // Capture stack trace BEFORE going deeper into the call chain
      const stackTrace = this.captureCallerStackTrace();
      
      // Create properly formatted log entry using shared types
      const logEntry = Console.MessageUtils.createLogEntry(level, args, {
        stackTrace // Pass the captured stack trace
      });

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

  /**
   * Capture stack trace from the actual calling location (not our console forwarding methods)
   */
  private captureCallerStackTrace(): string {
    try {
      const error = new Error();
      if (error.stack) {
        const lines = error.stack.split('\n');
        const filteredLines = lines.filter(line => 
          // Filter out our own console management methods
          !line.includes('ClientConsoleManager.forwardConsole') &&
          !line.includes('ClientConsoleManager.captureCallerStackTrace') &&
          !line.includes('ClientConsoleManager.') &&
          !line.includes('ClientLoggerDaemon.') &&
          !line.includes('console.log') &&
          !line.includes('console.warn') &&
          !line.includes('console.error') &&
          !line.includes('console.info') &&
          !line.includes('console.debug') &&
          !line.includes('console.trace') &&
          !line.includes('console.probe') &&
          // Filter out the Error constructor and other internal methods
          !line.includes('Error.captureStackTrace') &&
          !line.includes('at new Error') &&
          !line.includes('at Object.') && // Common for arrow functions
          line.trim().length > 0
        );
        
        // Return the first few meaningful lines to show the actual call location
        return filteredLines.slice(0, 5).join('\n');
      }
    } catch (e) {
      // Fallback if stack capture fails
    }
    return '';
  }


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
      // Remove probe method only (no console override to restore)
      delete (console as any).probe;
      this.consoleForwarding = false;
    }
    
    clientLoggerClient.destroy();
  }
}