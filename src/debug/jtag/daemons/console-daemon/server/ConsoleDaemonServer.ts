/**
 * Console Daemon - Server Implementation
 * 
 * Server-specific console daemon that handles server console logging and file writes.
 */

import { ConsoleDaemon } from '../shared/ConsoleDaemon';
import type { ConsolePayload } from '../shared/ConsoleDaemon';
import { SYSTEM_SCOPES, shouldDualScope } from '../../../system/core/types/SystemScopes';

export class ConsoleDaemonServer extends ConsoleDaemon {
  private symlinkWarningShown = false;
  private currentUserSymlinkWarningShown = false;
  
  // setupConsoleInterception() is now handled by the base class

  /**
   * Process console payload - server implementation with dual-scope logging
   */
  protected async processConsolePayload(consolePayload: ConsolePayload): Promise<void> {
    // Always write to system logs
    await this.writeToSystemLogs(consolePayload);
    
    // Also write to session logs if this is a real session
    if (shouldDualScope(consolePayload.sessionId)) {
      await this.writeToSessionLogs(consolePayload);
    }
    
    // Error monitoring
    if (consolePayload.level === 'error') {
      await this.notifyErrorMonitoring(consolePayload);
    }
  }

  private async writeToSystemLogs(consolePayload: ConsolePayload): Promise<void> {
    // Write to system logs: .continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/
    try {
      const fs = await eval('import("fs/promises")');
      const path = await eval('import("path")');
      
      const logDir = `.continuum/jtag/sessions/system/${SYSTEM_SCOPES.SYSTEM}/logs`;
      await fs.mkdir(logDir, { recursive: true });
      
      // Create symlink for convenient access: .continuum/jtag/system -> sessions/system/00000000-0000-0000-0000-000000000000/
      await this.ensureSystemSymlink(fs, path);
      
      await this.writeLogFiles(fs, path, logDir, consolePayload);
      
    } catch (error) {
      this.originalConsole.error('ConsoleDaemon: Failed to write system logs:', error);
    }
  }

  private async writeToSessionLogs(consolePayload: ConsolePayload): Promise<void> {
    // Write to session logs: .continuum/jtag/sessions/{category}/sessionId/logs/
    // TODO: Get category from session metadata - for now use 'user' as default
    try {
      const fs = await eval('import("fs/promises")');
      const path = await eval('import("path")');
      
      const category = 'user'; // TODO: Get from SessionDaemon
      const logDir = `.continuum/jtag/sessions/${category}/${consolePayload.sessionId}/logs`;
      await fs.mkdir(logDir, { recursive: true });
      
      // Create/update currentUser symlink for easy access to current session
      await this.ensureCurrentUserSymlink(fs, path, category, consolePayload.sessionId);
      
      await this.writeLogFiles(fs, path, logDir, consolePayload);
      
    } catch (error) {
      this.originalConsole.error('ConsoleDaemon: Failed to write session logs:', error);
    }
  }

  private async ensureSystemSymlink(fs: any, path: any): Promise<void> {
    try {
      const symlinkPath = '.continuum/jtag/system';
      const targetPath = `sessions/system/${SYSTEM_SCOPES.SYSTEM}`;
      
      // Check if symlink already exists and points to the correct target
      try {
        const stats = await fs.lstat(symlinkPath);
        if (stats.isSymbolicLink()) {
          const linkTarget = await fs.readlink(symlinkPath);
          if (linkTarget === targetPath) {
            // Symlink already exists and points to correct target - no action needed
            return;
          }
          // Symlink exists but points to wrong target - remove it
          await fs.unlink(symlinkPath);
          this.originalConsole.log(`🔗 ${this.toString()}: Removed outdated system symlink (was: ${linkTarget})`);
        } else {
          // Path exists but is not a symlink - remove it
          await fs.rm(symlinkPath, { recursive: true, force: true });
          this.originalConsole.log(`🔗 ${this.toString()}: Removed non-symlink at system path`);
        }
      } catch (checkError: any) {
        // Path doesn't exist or other error - that's fine, we'll create it
        if (checkError.code !== 'ENOENT') {
          this.originalConsole.log(`🔗 ${this.toString()}: Symlink check warning (continuing): ${checkError.message}`);
        }
      }
      
      // Create the symlink (path should be clear now)
      await fs.symlink(targetPath, symlinkPath);
      this.originalConsole.log(`🔗 ${this.toString()}: Created system symlink: ${symlinkPath} -> ${targetPath}`);
      
    } catch (error: unknown) {
      // Don't fail the whole operation if symlink creation fails - this is non-critical
      // Only warn once to avoid log spam
      if (!this.symlinkWarningShown) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.originalConsole.warn(`ConsoleDaemon: Symlink creation failed (non-critical):`, errorMessage);
        this.symlinkWarningShown = true;
      }
    }
  }

  private async ensureCurrentUserSymlink(fs: any, path: any, category: string, sessionId: string): Promise<void> {
    try {
      const symlinkPath = '.continuum/jtag/currentUser';
      const targetPath = `sessions/${category}/${sessionId}`;
      
      // Check if symlink already exists and points to the correct target
      try {
        const stats = await fs.lstat(symlinkPath);
        if (stats.isSymbolicLink()) {
          const linkTarget = await fs.readlink(symlinkPath);
          if (linkTarget === targetPath) {
            // Symlink already exists and points to correct target - no action needed
            return;
          }
          // Symlink exists but points to wrong target - remove it
          await fs.unlink(symlinkPath);
          this.originalConsole.log(`🔗 ${this.toString()}: Removed outdated currentUser symlink (was: ${linkTarget})`);
        } else {
          // Path exists but is not a symlink - remove it
          await fs.rm(symlinkPath, { recursive: true, force: true });
          this.originalConsole.log(`🔗 ${this.toString()}: Removed non-symlink at currentUser path`);
        }
      } catch (checkError: any) {
        // Path doesn't exist or other error - that's fine, we'll create it
        if (checkError.code !== 'ENOENT') {
          this.originalConsole.log(`🔗 ${this.toString()}: Symlink check warning (continuing): ${checkError.message}`);
        }
      }
      
      // Create the symlink (path should be clear now)
      await fs.symlink(targetPath, symlinkPath);
      this.originalConsole.log(`🔗 ${this.toString()}: Updated currentUser symlink: ${symlinkPath} -> ${targetPath}`);
      
    } catch (error: unknown) {
      // Don't fail the whole operation if symlink creation fails - this is non-critical
      // Only warn once to avoid log spam
      if (!this.currentUserSymlinkWarningShown) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.originalConsole.warn(`ConsoleDaemon: CurrentUser symlink creation failed (non-critical):`, errorMessage);
        this.currentUserSymlinkWarningShown = true;
      }
    }
  }

  private async writeLogFiles(fs: any, path: any, logDir: string, consolePayload: ConsolePayload): Promise<void> {
    // Write to both text and JSON files
    const baseName = `${consolePayload.context.environment}-console-${consolePayload.level}`;
    const txtFile = path.join(logDir, `${baseName}.log`);
    const jsonFile = path.join(logDir, `${baseName}.json`);
    
    // Append to text log
    const logLine = `${consolePayload.timestamp} [${consolePayload.component}] ${consolePayload.message}\n`;
    await fs.appendFile(txtFile, logLine);
    
    // Append to JSON log
    const jsonEntry = JSON.stringify(consolePayload) + '\n';
    await fs.appendFile(jsonFile, jsonEntry);
  }

  private async notifyErrorMonitoring(consolePayload: ConsolePayload): Promise<void> {
    // Send errors to monitoring systems - use original console to avoid recursion
    this.originalConsole.log(`🚨 Error monitoring notification:`, consolePayload.message);
  }
}