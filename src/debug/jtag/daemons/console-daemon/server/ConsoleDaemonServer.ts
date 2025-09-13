/**
 * Console Daemon - Server Implementation
 * 
 * Server-specific console daemon that handles server console logging and file writes.
 */

import { ConsoleDaemon } from '../shared/ConsoleDaemon';
import type { ConsolePayload } from '../shared/ConsoleDaemon';
import { SYSTEM_SCOPES, shouldDualScope } from '../../../system/core/types/SystemScopes';
import { WorkingDirConfig } from '../../../system/core/config/WorkingDirConfig';
import * as fs from 'fs/promises';
import * as path from 'path';

export class ConsoleDaemonServer extends ConsoleDaemon {
  private symlinkWarningShown = false;
  private currentUserSymlinkWarningShown = false;
  private lastCurrentUserTarget = '';
  private currentUserUpdateInProgress = false;
  
  
  // setupConsoleInterception() is now handled by the base class

  /**
   * Process console payload - server implementation with dual-scope logging
   */
  protected async processConsolePayload(consolePayload: ConsolePayload): Promise<void> {
    // Always write to system logs
    await this.writeToSystemLogs(consolePayload);
    
    // Don't create session directories for server context UUIDs
    if (consolePayload.sessionId === this.context.uuid) {
      return;
    }
    
    // Write to session logs for real client sessions
    if (shouldDualScope(consolePayload.sessionId)) {
      await this.writeToSessionLogs(consolePayload);
    }
    
    // Error monitoring
    if (consolePayload.level === 'error') {
      await this.notifyErrorMonitoring(consolePayload);
    }
  }

  private async writeToSystemLogs(consolePayload: ConsolePayload): Promise<void> {
    try {
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const logDir = path.join(continuumPath, 'jtag', 'sessions', 'system', SYSTEM_SCOPES.SYSTEM, 'logs');
      await fs.mkdir(logDir, { recursive: true });
      
      await this.ensureSystemSymlink(continuumPath);
      await this.writeLogFiles(logDir, consolePayload);
      
    } catch (error) {
      this.originalConsole.error('ConsoleDaemon: Failed to write system logs:', error);
    }
  }

  private async writeToSessionLogs(consolePayload: ConsolePayload): Promise<void> {
    try {
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const category = 'user';
      const logDir = path.join(continuumPath, 'jtag', 'sessions', category, consolePayload.sessionId, 'logs');
      await fs.mkdir(logDir, { recursive: true });
      
      await this.ensureCurrentUserSymlink(continuumPath, category, consolePayload.sessionId);
      await this.writeLogFiles(logDir, consolePayload);
      
    } catch (error) {
      this.originalConsole.error('ConsoleDaemon: Failed to write session logs:', error);
    }
  }

  private async ensureSystemSymlink(continuumPath: string): Promise<void> {
    try {
      const symlinkPath = path.join(continuumPath, 'jtag', 'system');
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
      } catch (checkError: unknown) {
        // Path doesn't exist or other error - that's fine, we'll create it
        const errorCode = (checkError as { code?: string })?.code;
        if (checkError instanceof Error && errorCode && errorCode !== 'ENOENT') {
          this.originalConsole.log(`🔗 ${this.toString()}: Symlink check warning (continuing): ${checkError.message}`);
        }
      }
      
      // Create the symlink (path should be clear now)
      await fs.symlink(targetPath, symlinkPath);
      this.originalConsole.log(`🔗 ${this.toString()}: Created system symlink: ${symlinkPath} -> ${targetPath}`);
      
    } catch (error: unknown) {
      // Don't fail the whole operation if symlink creation fails - this is non-critical
      // Special handling for EEXIST - this is normal and expected
      if (error instanceof Error && error.message.includes('EEXIST: file already exists')) {
        // Symlink already exists - this is fine, no need to report
        return;
      }
      
      // Only warn once for other errors to avoid log spam
      if (!this.symlinkWarningShown) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.originalConsole.warn(`ConsoleDaemon: Symlink creation failed (non-critical):`, errorMessage);
        this.symlinkWarningShown = true;
      }
    }
  }

  private async ensureCurrentUserSymlink(continuumPath: string, category: string, sessionId: string): Promise<void> {
    try {
      const symlinkPath = path.join(continuumPath, 'jtag', 'currentUser');
      const targetPath = `sessions/${category}/${sessionId}`;
      
      // 🔧 CLAUDE-FIX-1757736695: Symlink validation + UNKNOWN_SESSION exclusion
      // CRITICAL FIX: Always check current symlink target FIRST, regardless of cache
      // This prevents unnecessary work when symlink already points to correct target
      try {
        const stats = await fs.lstat(symlinkPath);
        if (stats.isSymbolicLink()) {
          const currentTarget = await fs.readlink(symlinkPath);
          if (currentTarget === targetPath) {
            // Symlink already exists and points to correct target - NO ACTION NEEDED
            this.lastCurrentUserTarget = targetPath;
            return;
          }
        }
      } catch (checkError: unknown) {
        // Path doesn't exist - that's fine, we'll create it below
        const errorCode = (checkError as { code?: string })?.code;
        if (checkError instanceof Error && errorCode !== 'ENOENT') {
          this.originalConsole.log(`🔗 ${this.toString()}: Symlink check warning: ${checkError.message}`);
        }
      }
      
      // Prevent concurrent updates that can cause race conditions
      if (this.currentUserUpdateInProgress) {
        return;
      }
      
      this.currentUserUpdateInProgress = true;
      
      try {
        // Only now do we need to update the symlink
        try {
          const stats = await fs.lstat(symlinkPath);
          if (stats.isSymbolicLink()) {
            const linkTarget = await fs.readlink(symlinkPath);
            // Remove existing symlink (we know it's wrong from check above)
            await fs.unlink(symlinkPath);
            this.originalConsole.log(`🔗 ${this.toString()}: Removed outdated currentUser symlink (was: ${linkTarget})`);
          } else {
            // Path exists but is not a symlink - remove it
            await fs.rm(symlinkPath, { recursive: true, force: true });
            this.originalConsole.log(`🔗 ${this.toString()}: Removed non-symlink at currentUser path`);
          }
        } catch (removeError: unknown) {
          // Path doesn't exist - that's fine
          const errorCode = (removeError as { code?: string })?.code;
          if (removeError instanceof Error && errorCode !== 'ENOENT') {
            this.originalConsole.log(`🔗 ${this.toString()}: Symlink removal warning: ${removeError.message}`);
          }
        }
        
        // Create the symlink
        await fs.symlink(targetPath, symlinkPath);
        this.lastCurrentUserTarget = targetPath;
        this.originalConsole.log(`🔗 ${this.toString()}: Updated currentUser symlink: ${symlinkPath} -> ${targetPath}`);
        
      } finally {
        this.currentUserUpdateInProgress = false;
      }
      
    } catch (error: unknown) {
      this.currentUserUpdateInProgress = false;
      
      // Don't fail the whole operation if symlink creation fails - this is non-critical
      if (error instanceof Error && error.message.includes('EEXIST: file already exists')) {
        // Symlink already exists - this is fine, no need to report
        return;
      }
      
      // Only warn once for other errors to avoid log spam
      if (!this.currentUserSymlinkWarningShown) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.originalConsole.warn(`ConsoleDaemon: CurrentUser symlink creation failed (non-critical):`, errorMessage);
        this.currentUserSymlinkWarningShown = true;
      }
    }
  }

  private async writeLogFiles(logDir: string, consolePayload: ConsolePayload): Promise<void> {
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