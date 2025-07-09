/**
 * Console Forwarder - Handles console message forwarding to server
 * Provides semaphore protection and message queuing
 */

import { ConsoleCommand } from '../types/ConsoleTypes';
import { ContinuumState } from '../types/BrowserClientTypes';

interface OriginalConsole {
  log: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
  info: (...args: unknown[]) => void;
  trace: (...args: unknown[]) => void;
}

export class ConsoleForwarder {
  private consoleForwarding = false;
  private consoleMessageQueue: ConsoleCommand[] = [];
  private originalConsole: OriginalConsole = {} as OriginalConsole;
  private executeCallback?: (command: string, params: Record<string, unknown>) => Promise<unknown>;

  constructor(private getState: () => ContinuumState, private getSessionId: () => string | null) {}

  setExecuteCallback(callback: (command: string, params: Record<string, unknown>) => Promise<unknown>): void {
    this.executeCallback = callback;
  }

  enableConsoleForwarding(): void {
    if (this.consoleForwarding) return;
    
    // Store original console methods
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console),
      trace: console.trace.bind(console)
    };

    // Override console methods
    console.log = (...args: unknown[]) => {
      this.originalConsole.log(...args);
      this.forwardConsole('log', args);
    };

    console.warn = (...args: unknown[]) => {
      this.originalConsole.warn(...args);
      this.forwardConsole('warn', args);
    };

    console.error = (...args: unknown[]) => {
      this.originalConsole.error(...args);
      this.forwardConsole('error', args);
    };

    console.info = (...args: unknown[]) => {
      this.originalConsole.info(...args);
      this.forwardConsole('info', args);
    };

    console.trace = (...args: unknown[]) => {
      this.originalConsole.trace(...args);
      this.forwardConsole('trace', args);
    };

    this.consoleForwarding = true;
    console.log('✅ Console forwarding enabled');
  }

  private forwardConsole(type: string, args: unknown[]): void {
    // Forward as soon as console forwarding is enabled (connected state)
    if (this.getState() !== 'connected' && this.getState() !== 'ready') return;

    try {
      const consoleCommand: ConsoleCommand = {
        action: type,
        message: args.map(arg => String(arg)).join(' '),
        timestamp: new Date().toISOString(),
        sessionId: this.getSessionId()
      };

      // Only execute if we're in ready state, otherwise queue for later
      if (this.getState() === 'ready' && this.executeCallback) {
        this.executeCallback('console', consoleCommand).catch(() => {
          // Fail silently to avoid console loops
        });
      } else {
        // Queue message for when we reach ready state
        this.queueConsoleMessage(consoleCommand);
      }
    } catch (error) {
      // Fail silently to avoid console loops
    }
  }

  private queueConsoleMessage(consoleCommand: ConsoleCommand): void {
    this.consoleMessageQueue.push(consoleCommand);
  }

  flushConsoleMessageQueue(): void {
    if (this.consoleMessageQueue.length > 0 && this.executeCallback) {
      console.log(`🔄 Flushing ${this.consoleMessageQueue.length} queued console messages`);
      
      // Send all queued messages
      this.consoleMessageQueue.forEach(command => {
        this.executeCallback!(command.action, command).catch(() => {
          // Fail silently to avoid console loops
        });
      });
      
      // Clear the queue
      this.consoleMessageQueue = [];
    }
  }

  performHealthCheck(): void {
    console.log('🏥 Performing console forwarding health check...');
    console.error('❌ HEALTH CHECK: Error message test');
    console.warn('⚠️ HEALTH CHECK: Warning message test');
    console.trace('🔍 HEALTH CHECK: Trace message test');
    console.log('✅ HEALTH CHECK: Health check complete');
  }
}