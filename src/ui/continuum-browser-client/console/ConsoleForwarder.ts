/**
 * Console Forwarder - Handles console message forwarding to server
 * Provides semaphore protection and message queuing
 */

import type { ConsoleCommand } from '../types/ConsoleTypes';
import type { ContinuumState } from '../types/BrowserClientTypes';

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

  constructor(private getState: () => ContinuumState, private getSessionId: () => string | null) {
    this.enableConsoleForwarding();
  }

  setExecuteCallback(callback: (command: string, params: Record<string, unknown>) => Promise<unknown>): void {
    this.executeCallback = callback;
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

    this.consoleForwarding = true;
    console.log('✅ Console forwarding enabled');
  }

  private forwardConsole(type: string, args: unknown[]): void {
    try {
      const consoleCommand: ConsoleCommand = {
        action: type,
        message: args.map(arg => String(arg)).join(' '),
        timestamp: new Date().toISOString(),
        sessionId: this.getSessionId()
      };

      // Only execute if we're in ready state, otherwise queue for later
      if (this.getState() === 'ready' && this.executeCallback) {
        this.executeConsoleCommand(consoleCommand);
      } else {
        // Queue message for when we reach ready state
        this.queueConsoleMessage(consoleCommand);
      }
    } catch (error) {
      // Fail silently to avoid console loops
      this.originalConsole.error('❌ Error forwarding console message:', error);
    }
  }

  private queueConsoleMessage(consoleCommand: ConsoleCommand): void {
    this.consoleMessageQueue.push(consoleCommand);
  }

  private executeConsoleCommand(consoleCommand: ConsoleCommand, prefix="🔄"): void {
    if (this.executeCallback) {
      this.originalConsole.log(`${prefix} Executing console command: ${consoleCommand.action}`, consoleCommand);
      this.executeCallback('console', consoleCommand).catch((e) => {
        this.originalConsole.error(`❌ Failed to execute console command: ${consoleCommand.action}`, consoleCommand, e);
      });
    }
  }

  executeAndFlushConsoleMessageQueue(): void {
    if (this.consoleMessageQueue.length > 0 && this.executeCallback) {
      // Send all queued messages
      this.consoleMessageQueue.sort((a, b) => a.timestamp.localeCompare(b.timestamp)).forEach(consoleCommand => this.executeConsoleCommand(consoleCommand, '🚽'));

      console.log(`🚽 Flushing ${this.consoleMessageQueue.length} queued console messages`);//occurred after everything else
      
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