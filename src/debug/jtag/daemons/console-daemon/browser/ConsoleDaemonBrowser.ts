/**
 * Console Daemon - Browser Implementation
 *
 * Browser-specific console daemon that handles browser console interception.
 * Uses batched writes to avoid blocking main thread on every log.
 */

import { ConsoleDaemon } from '../shared/ConsoleDaemon';
import type { ConsolePayload } from '../shared/ConsoleDaemon';

// Declare window for TypeScript if not already in the global scope
declare const window: Window & typeof globalThis;

export class ConsoleDaemonBrowser extends ConsoleDaemon {
  // In-memory buffer for batched writes
  private pendingLogs: ConsolePayload[] = [];
  private flushScheduled = false;
  private static readonly FLUSH_INTERVAL_MS = 1000; // Flush every 1s max
  private static readonly MAX_STORED_LOGS = 100;

  /**
   * Process console payload - browser implementation
   */
  protected async processConsolePayload(consolePayload: ConsolePayload): Promise<void> {
    // Add to in-memory buffer (fast, non-blocking)
    this.pendingLogs.push(consolePayload);

    // Schedule batched flush (doesn't block main thread)
    this.scheduleFlush();

    // Messages are automatically added to buffer in parent class
    // and will be drained to server when JTAG system is ready
  }

  /**
   * Schedule a batched flush during idle time
   */
  private scheduleFlush(): void {
    if (this.flushScheduled) return;
    this.flushScheduled = true;

    // Use requestIdleCallback for non-blocking localStorage write
    if ('requestIdleCallback' in window) {
      (window as any).requestIdleCallback(() => this.flushToStorage(), {
        timeout: ConsoleDaemonBrowser.FLUSH_INTERVAL_MS
      });
    } else {
      // Fallback: setTimeout to at least batch
      setTimeout(() => this.flushToStorage(), ConsoleDaemonBrowser.FLUSH_INTERVAL_MS);
    }
  }

  /**
   * Flush pending logs to localStorage (called during idle time)
   */
  private flushToStorage(): void {
    this.flushScheduled = false;

    if (this.pendingLogs.length === 0) return;

    try {
      if (window?.localStorage) {
        // Read existing logs
        const existingLogs = JSON.parse(
          window.localStorage.getItem('jtag-console-logs') ?? '[]'
        );

        // Append pending logs
        const allLogs = [...existingLogs, ...this.pendingLogs];

        // Keep only last N entries
        const trimmedLogs = allLogs.length > ConsoleDaemonBrowser.MAX_STORED_LOGS
          ? allLogs.slice(-ConsoleDaemonBrowser.MAX_STORED_LOGS)
          : allLogs;

        // Single write for entire batch
        window.localStorage.setItem('jtag-console-logs', JSON.stringify(trimmedLogs));

        // Clear pending buffer
        this.pendingLogs = [];
      }
    } catch {
      // Silently ignore localStorage errors - logs are still in parent buffer
      this.pendingLogs = [];
    }
  }
}