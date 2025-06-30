/**
 * BaseFileCommand - Shared functionality for all file operations
 * 
 * Provides common methods for file path resolution, session integration,
 * and ContinuumDirectoryDaemon communication
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import * as path from 'path';

// Strongly typed enums for daemon operations
export enum FileSystemOperation {
  READ_DIRECTORY = 'read_directory',
  CREATE_DIRECTORY = 'create_directory', 
  WRITE_FILE = 'write_file',
  READ_FILE = 'read_file',
  APPEND_FILE = 'append_file',
  DELETE_FILE = 'delete_file',
  GET_FILE_STATS = 'get_file_stats',
  CHECK_FILE_ACCESS = 'check_file_access'
}

export enum DirectoryOperation {
  GET_ROOT_DIRECTORY = 'get_root_directory',
  GET_SESSION_DIRECTORY = 'get_session_directory',
  CREATE_SESSION_STRUCTURE = 'create_session_structure',
  GET_ARTIFACT_LOCATION = 'get_artifact_location'
}

export abstract class BaseFileCommand extends BaseCommand {
  
  /**
   * Get .continuum root directory from ContinuumDirectoryDaemon
   */
  protected static async getContinuumRoot(): Promise<string> {
    try {
      const response = await this.delegateToContinuumDirectoryDaemon(DirectoryOperation.GET_ROOT_DIRECTORY, {});
      return response.rootPath;
    } catch (error) {
      // Fallback during development
      console.warn('ContinuumDirectoryDaemon delegation failed, using fallback:', error);
      const homeDir = process.env.HOME || process.env.USERPROFILE || process.cwd();
      return path.join(homeDir, '.continuum');
    }
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
    const items = await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.READ_DIRECTORY, { path: typeDir, withFileTypes: true });
    
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
          const subItems = await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.READ_DIRECTORY, { path: subDir, withFileTypes: true });
          
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
   * Ensure directory exists using ContinuumFileSystemDaemon
   */
  protected static async ensureDirectoryExists(dirPath: string): Promise<void> {
    try {
      await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.CREATE_DIRECTORY, { 
        path: dirPath, 
        recursive: true 
      });
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
      
      await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.APPEND_FILE, {
        path: logFile,
        content: logLine,
        encoding: 'utf8'
      });
    } catch {
      // Don't fail the main operation if logging fails
    }
  }

  /**
   * Get file stats safely
   */
  protected static async getFileStats(filePath: string): Promise<any | null> {
    try {
      return await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.GET_FILE_STATS, { path: filePath });
    } catch {
      return null;
    }
  }

  /**
   * Check if file exists
   */
  protected static async fileExists(filePath: string): Promise<boolean> {
    try {
      await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.CHECK_FILE_ACCESS, { path: filePath });
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Delegate filesystem operations to ContinuumFileSystemDaemon
   * All file commands use this pattern for actual filesystem operations
   */
  protected static async delegateToContinuumFileSystemDaemon(operation: FileSystemOperation, params: any): Promise<any> {
    try {
      const daemonMessage = {
        type: 'daemon_request',
        target: 'continuum-filesystem',
        operation,
        params,
        requestId: `file_${operation}_${Date.now()}`,
        timestamp: Date.now()
      };
      
      const response = await this.sendDaemonMessage(daemonMessage);
      
      if (response.success) {
        return response.data;
      } else {
        throw new Error(response.error || `Filesystem ${operation} failed`);
      }
      
    } catch (error) {
      // For critical file operations, provide fallback implementations
      console.warn(`ContinuumFileSystemDaemon ${operation} failed, using fallback:`, error);
      return this.fallbackFileOperation(operation, params);
    }
  }

  /**
   * Delegate directory operations to ContinuumDirectoryDaemon
   */
  protected static async delegateToContinuumDirectoryDaemon(operation: DirectoryOperation, params: any): Promise<any> {
    try {
      const daemonMessage = {
        type: 'daemon_request',
        target: 'continuum-directory',
        operation,
        params,
        requestId: `dir_${operation}_${Date.now()}`,
        timestamp: Date.now()
      };
      
      const response = await this.sendDaemonMessage(daemonMessage);
      
      if (response.success) {
        return response.data;
      } else {
        throw new Error(response.error || `Directory ${operation} failed`);
      }
      
    } catch (error) {
      console.warn(`ContinuumDirectoryDaemon ${operation} failed, using fallback:`, error);
      return this.fallbackDirectoryOperation(operation, params);
    }
  }

  /**
   * Send message via internal daemon message bus (not WebSocket ports)
   * Commands communicate with daemons through the CommandProcessorDaemon bus
   */
  protected static async sendDaemonMessage(_message: any): Promise<any> {
    // TODO: Use internal daemon message bus via CommandProcessorDaemon
    // Daemons communicate via internal bus, NOT across ports
    return new Promise((resolve) => {
      setTimeout(() => {
        resolve({
          success: false,
          error: 'Internal daemon bus communication not yet implemented',
          mockMode: true
        });
      }, 10); // Quick fallback for file operations
    });
  }

  /**
   * Fallback file operations for development mode
   */
  protected static async fallbackFileOperation(operation: FileSystemOperation, params: any): Promise<any> {
    const fs = await import('fs/promises');
    
    switch (operation) {
      case FileSystemOperation.READ_DIRECTORY:
        return await fs.readdir(params.path, { withFileTypes: params.withFileTypes });
      case FileSystemOperation.CREATE_DIRECTORY:
        await fs.mkdir(params.path, { recursive: params.recursive });
        return { success: true };
      case FileSystemOperation.WRITE_FILE:
        await fs.writeFile(params.path, params.content, params.encoding ? { encoding: params.encoding } : undefined);
        return { success: true };
      case FileSystemOperation.APPEND_FILE:
        await fs.appendFile(params.path, params.content, { encoding: params.encoding });
        return { success: true };
      case FileSystemOperation.GET_FILE_STATS:
        return await fs.stat(params.path);
      case FileSystemOperation.CHECK_FILE_ACCESS:
        await fs.access(params.path);
        return { success: true };
      default:
        throw new Error(`Unknown file operation: ${operation}`);
    }
  }

  /**
   * Fallback directory operations for development mode
   */
  protected static async fallbackDirectoryOperation(operation: DirectoryOperation, params: any): Promise<any> {
    const fs = await import('fs/promises');
    
    switch (operation) {
      case DirectoryOperation.GET_ROOT_DIRECTORY:
        const homeDir = process.env.HOME || process.env.USERPROFILE || process.cwd();
        const rootPath = path.join(homeDir, '.continuum');
        await fs.mkdir(rootPath, { recursive: true });
        return { rootPath };
      default:
        throw new Error(`Unknown directory operation: ${operation}`);
    }
  }
}