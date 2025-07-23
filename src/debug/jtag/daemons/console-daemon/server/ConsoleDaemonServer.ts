/**
 * Console Daemon - Server Implementation
 * 
 * Server-specific console daemon that handles server console logging and file writes.
 */

import { JTAGContext } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { ConsoleDaemon, ConsolePayload } from '../shared/ConsoleDaemon';

export class ConsoleDaemonServer extends ConsoleDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Setup server-specific console interception
   */
  protected setupConsoleInterception(): void {
    // Server console interception is simpler - just wrap the methods
    this.originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug
    };

    ['log', 'info', 'warn', 'error', 'debug'].forEach(level => {
      (console as any)[level] = (...args: any[]) => {
        // Call original first
        (this.originalConsole as any)[level](...args);
        
        // Process through daemon
        if (!this.intercepting) {
          this.intercepting = true;
          this.processConsoleCall(level as any, args);
          this.intercepting = false;
        }
      };
    });

    console.log('🎧 ConsoleDaemon[server]: Console interception enabled');
  }

  /**
   * Process console payload - server implementation
   */
  protected async processConsolePayload(consolePayload: ConsolePayload): Promise<void> {
    // Server-specific console processing
    await this.writeToFile(consolePayload);
    
    // Could also forward to monitoring systems
    if (consolePayload.level === 'error') {
      await this.notifyErrorMonitoring(consolePayload);
    }
  }

  private async writeToFile(consolePayload: ConsolePayload): Promise<void> {
    // Write to JTAG log files (server only)
    try {
      const fs = await eval('import("fs/promises")');
      const path = await eval('import("path")');
      
      // Get log directory
      const logDir = '.continuum/jtag/logs';
      await fs.mkdir(logDir, { recursive: true });
      
      // Write to both text and JSON files
      const baseName = `${consolePayload.context}-console-${consolePayload.level}`;
      const txtFile = path.join(logDir, `${baseName}.log`);
      const jsonFile = path.join(logDir, `${baseName}.json`);
      
      // Append to text log
      const logLine = `${consolePayload.timestamp} [${consolePayload.component}] ${consolePayload.message}\n`;
      await fs.appendFile(txtFile, logLine);
      
      // Append to JSON log
      const jsonEntry = JSON.stringify(consolePayload) + '\n';
      await fs.appendFile(jsonFile, jsonEntry);
      
    } catch (error) {
      // Fallback to original console to avoid loops
      this.originalConsole.error('ConsoleDaemon: Failed to write log files:', error);
    }
  }

  private async notifyErrorMonitoring(consolePayload: ConsolePayload): Promise<void> {
    // Send errors to monitoring systems
    console.log(`🚨 Error monitoring notification:`, consolePayload.message);
  }
}