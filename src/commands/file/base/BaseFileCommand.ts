/**
 * BaseFileCommand - Shared functionality for all file operations
 * 
 * Provides common methods for file path resolution, session integration,
 * and ContinuumDirectoryDaemon communication
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import * as fs from 'fs/promises';
import * as path from 'path';

export abstract class BaseFileCommand extends BaseCommand {
  
  /**
   * Get .continuum root directory from ContinuumDirectoryDaemon
   */
  protected static async getContinuumRoot(): Promise<string> {
    // TODO: Call ContinuumDirectoryDaemon to get this configuration
    // For now, discover it programmatically
    const homeDir = process.env.HOME || process.env.USERPROFILE || process.cwd();
    const continuumRoot = path.join(homeDir, '.continuum');
    
    // Ensure it exists
    await fs.mkdir(continuumRoot, { recursive: true });
    return continuumRoot;
  }

  /**
   * Find session path by session ID using daemon organization
   */
  protected static async findSessionPath(sessionId: string): Promise<string | null> {
    const continuumRoot = await this.getContinuumRoot();
    const sessionTypes = ['portal', 'validation', 'user', 'personas'];
    
    for (const type of sessionTypes) {
      const typeDir = path.join(continuumRoot, 'sessions', type);
      try {
        const sessionPath = await this.searchSessionInDirectory(typeDir, sessionId, type);
        if (sessionPath) return sessionPath;
      } catch {
        // Skip if session type directory doesn't exist
      }
    }
    
    return null;
  }

  /**
   * Search for session in a specific directory type
   */
  private static async searchSessionInDirectory(typeDir: string, sessionId: string, type: string): Promise<string | null> {
    const items = await fs.readdir(typeDir, { withFileTypes: true });
    
    for (const item of items) {
      if (!item.isDirectory()) continue;
      
      // Direct match for portal/validation sessions
      if (item.name.includes(sessionId)) {
        return path.join(typeDir, item.name);
      }
      
      // For personas and user directories, look one level deeper
      if (type === 'personas' || type === 'user') {
        try {
          const subDir = path.join(typeDir, item.name);
          const subItems = await fs.readdir(subDir, { withFileTypes: true });
          
          for (const subItem of subItems) {
            if (subItem.isDirectory() && subItem.name.includes(sessionId)) {
              return path.join(subDir, subItem.name);
            }
          }
        } catch {
          // Skip if can't read subdirectory
        }
      }
    }
    
    return null;
  }

  /**
   * Get artifact subdirectory name following daemon conventions
   */
  protected static getArtifactSubdirectory(artifactType?: string): string {
    switch (artifactType) {
      case 'screenshot': return 'screenshots';
      case 'log': return 'logs';
      case 'recording': return 'recordings';
      case 'file': return 'files';
      case 'devtools': return 'devtools';
      case 'metadata': return 'metadata';
      default: return 'files';
    }
  }

  /**
   * Ensure directory exists using consistent error handling
   */
  protected static async ensureDirectoryExists(dirPath: string): Promise<void> {
    try {
      await fs.mkdir(dirPath, { recursive: true });
    } catch (error) {
      throw new Error(`Failed to create directory ${dirPath}: ${error}`);
    }
  }

  /**
   * Get target path for file operation using session management
   */
  protected static async getTargetPath(params: {
    filename: string;
    sessionId: string | undefined;
    artifactType: string | undefined;
    directory: string | undefined;
  }): Promise<string> {
    // If directory is explicitly provided, use it
    if (params.directory) {
      return path.join(params.directory, params.filename);
    }

    const continuumRoot = await this.getContinuumRoot();
    
    // If no session ID, write to general location
    if (!params.sessionId) {
      const defaultDir = params.artifactType ? 
        path.join(continuumRoot, params.artifactType + 's') : 
        path.join(continuumRoot, 'files');
      return path.join(defaultDir, params.filename);
    }

    // Find session and write to appropriate artifact subdirectory
    const sessionPath = await this.findSessionPath(params.sessionId);
    if (!sessionPath) {
      throw new Error(`Session ${params.sessionId} not found`);
    }

    const artifactSubdir = this.getArtifactSubdirectory(params.artifactType);
    return path.join(sessionPath, artifactSubdir, params.filename);
  }

  /**
   * Log file operation for debugging and forensics
   */
  protected static async logFileOperation(operation: string, targetPath: string, metadata: any = {}): Promise<void> {
    try {
      const continuumRoot = await this.getContinuumRoot();
      const logDir = path.join(continuumRoot, 'logs');
      await this.ensureDirectoryExists(logDir);

      const logEntry = {
        timestamp: new Date().toISOString(),
        operation,
        path: targetPath,
        ...metadata
      };

      const logFile = path.join(logDir, 'file-operations.log');
      const logLine = JSON.stringify(logEntry) + '\n';
      
      await fs.appendFile(logFile, logLine);
    } catch {
      // Don't fail the main operation if logging fails
    }
  }

  /**
   * Get file stats safely
   */
  protected static async getFileStats(filePath: string): Promise<Awaited<ReturnType<typeof fs.stat>> | null> {
    try {
      return await fs.stat(filePath);
    } catch {
      return null;
    }
  }

  /**
   * Check if file exists
   */
  protected static async fileExists(filePath: string): Promise<boolean> {
    try {
      await fs.access(filePath);
      return true;
    } catch {
      return false;
    }
  }
}