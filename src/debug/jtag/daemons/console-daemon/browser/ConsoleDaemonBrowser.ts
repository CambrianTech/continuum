/**
 * Console Daemon - Browser Implementation
 * 
 * Browser-specific console daemon that handles browser console interception.
 */

import { JTAGContext } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { ConsoleDaemon, ConsolePayload } from '../shared/ConsoleDaemon';

export class ConsoleDaemonBrowser extends ConsoleDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Setup browser-specific console interception
   */
  protected setupConsoleInterception(): void {
    // Save original console methods
    this.originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug
    };

    // Intercept console calls
    (['log', 'info', 'warn', 'error', 'debug'] as const).forEach(level => {
      const originalMethod = this.originalConsole[level];
      (console as any)[level] = (...args: any[]) => {
        // Call original first
        originalMethod(...args);
        
        // Then process through daemon (avoid infinite loops)
        if (!this.intercepting) {
          this.intercepting = true;
          this.processConsoleCall(level, args);
          this.intercepting = false;
        }
      };
    });

    console.log('🎧 ConsoleDaemon[browser]: Console interception enabled');
  }

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
      const logs = JSON.parse(localStorage.getItem('jtag-console-logs') || '[]');
      logs.push(consolePayload);
      
      // Keep only last 100 entries
      if (logs.length > 100) {
        logs.splice(0, logs.length - 100);
      }
      
      localStorage.setItem('jtag-console-logs', JSON.stringify(logs));
    } catch (error) {
      // Ignore localStorage errors
    }
  }
}