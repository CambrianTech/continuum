/**
 * Console Daemon - Browser Implementation
 * 
 * Browser-specific console daemon that handles browser console interception.
 */

import { ConsoleDaemon } from '@daemonsConsoleDaemon/shared/ConsoleDaemon';
import type { ConsolePayload } from '@daemonsConsoleDaemon/shared/ConsoleDaemon';

// Declare window for TypeScript if not already in the global scope
declare const window: Window & typeof globalThis;

export class ConsoleDaemonBrowser extends ConsoleDaemon {

  // setupConsoleInterception() is now handled by the base class

  /**
   * Process console payload - browser implementation
   */
  protected async processConsolePayload(consolePayload: ConsolePayload): Promise<void> {
    // Browser-specific console processing
    this.storeInBrowser(consolePayload);
    
    // Messages are automatically added to buffer in parent class
    // and will be drained to server when JTAG system is ready
    // No immediate sending - let the queue drain handle transport
  }

  private storeInBrowser(consolePayload: ConsolePayload): void {
    // Store in browser localStorage
    try {
      if (window?.localStorage) {
        const logs = JSON.parse(window.localStorage.getItem('jtag-console-logs') ?? '[]');
        logs.push(consolePayload);

        // Keep only last 100 entries
        if (logs.length > 100) {
          logs.splice(0, logs.length - 100);
        }

        window.localStorage.setItem('jtag-console-logs', JSON.stringify(logs));
      }
    } catch (error) {
      // Ignore localStorage errors
      console.error('ConsoleDaemon: Failed to store logs in localStorage:', error);
    }
  }
}