/**
 * Console Daemon - Server Implementation
 * 
 * Server-specific console daemon that handles server console logging and file writes.
 */

import { ConsoleDaemon } from '../shared/ConsoleDaemon';
import type { ConsolePayload } from '../shared/ConsoleDaemon';

export class ConsoleDaemonServer extends ConsoleDaemon {
  
  // setupConsoleInterception() is now handled by the base class

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
    // Send errors to monitoring systems - use original console to avoid recursion
    this.originalConsole.log(`🚨 Error monitoring notification:`, consolePayload.message);
  }
}