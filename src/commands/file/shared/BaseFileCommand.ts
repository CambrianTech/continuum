// ISSUES: 0 open, last updated 2025-07-14 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * BaseFileCommand - Shared functionality for all file operations
 * 
 * Provides common methods for file path resolution, session integration,
 * and ContinuumDirectoryDaemon communication
 * 
 * Now uses shared types and simplified session-based path resolution
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { ArtifactType } from './FileTypes';

// Browser-compatible path utilities
const pathUtils = {
  join: (...parts: string[]) => {
    return parts.join('/').replace(/\/+/g, '/');
  },
  dirname: (filePath: string) => {
    const parts = filePath.split('/');
    return parts.slice(0, -1).join('/') || '/';
  }
};

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
      const homeDir = typeof process !== 'undefined' ? (process.env.HOME || process.env.USERPROFILE || process.cwd()) : '';
      return pathUtils.join(homeDir, '.continuum');
    }
  }

  /**
   * DEPRECATED: Complex findSessionPath logic replaced with simple session structure
   * Use getTargetPath() instead which uses predictable session paths
   */
  protected static async findSessionPath(sessionId: string): Promise<string | null> {
    console.warn('findSessionPath is deprecated. Use getTargetPath() for predictable session structure.');
    const continuumRoot = await this.getContinuumRoot();
    return pathUtils.join(continuumRoot, 'sessions', 'user', 'shared', sessionId);
  }

  /**
   * Get artifact subdirectory name following daemon conventions
   * Now uses shared ArtifactType enum for consistency
   */
  protected static getArtifactSubdirectory(artifactType?: ArtifactType | string): string {
    switch (artifactType) {
      case ArtifactType.SCREENSHOT:
      case 'screenshot': return 'screenshots';
      case ArtifactType.LOG:
      case 'log': return 'logs';
      case ArtifactType.RECORDING:
      case 'recording': return 'recordings';
      case ArtifactType.FILE:
      case 'file': return 'files';
      case ArtifactType.DEVTOOLS:
      case 'devtools': return 'devtools';
      case ArtifactType.METADATA:
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
   * Get target path for file operation using simple session context-based path resolution
   * Replaces complex findSessionPath logic with predictable session structure
   */
  protected static async getTargetPath(params: {
    filename: string;
    sessionId: string | undefined;
    artifactType: ArtifactType | string | undefined;
    directory?: string | undefined;
  }): Promise<string> {
    // If directory is explicitly provided, use it
    if (params.directory) {
      return pathUtils.join(params.directory, params.filename);
    }

    const continuumRoot = await this.getContinuumRoot();
    
    // If no session ID, write to general location
    if (!params.sessionId) {
      const defaultDir = params.artifactType ? 
        pathUtils.join(continuumRoot, this.getArtifactSubdirectory(params.artifactType)) : 
        pathUtils.join(continuumRoot, 'files');
      return pathUtils.join(defaultDir, params.filename);
    }

    // Use predictable session structure: .continuum/sessions/user/shared/{sessionId}
    const sessionPath = pathUtils.join(continuumRoot, 'sessions', 'user', 'shared', params.sessionId);
    
    // Add artifact subdirectory if specified
    if (params.artifactType) {
      const artifactSubdir = this.getArtifactSubdirectory(params.artifactType);
      return pathUtils.join(sessionPath, artifactSubdir, params.filename);
    }
    
    return pathUtils.join(sessionPath, params.filename);
  }

  /**
   * Log file operation for debugging and forensics
   */
  protected static async logFileOperation(operation: string, targetPath: string, metadata: any = {}): Promise<void> {
    try {
      const continuumRoot = await this.getContinuumRoot();
      const logDir = pathUtils.join(continuumRoot, 'logs');
      await this.ensureDirectoryExists(logDir);

      const logEntry = {
        timestamp: new Date().toISOString(),
        operation,
        path: targetPath,
        ...metadata
      };

      const logFile = pathUtils.join(logDir, 'file-operations.log');
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
    // Only available in Node.js environment
    if (typeof window !== 'undefined') {
      throw new Error('File operations not available in browser environment');
    }
    
    // Use eval to avoid bundling issues
    const fs = await eval('import("fs/promises")');
    
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
  protected static async fallbackDirectoryOperation(operation: DirectoryOperation, _params: any): Promise<any> {
    // Only available in Node.js environment
    if (typeof window !== 'undefined') {
      throw new Error('Directory operations not available in browser environment');
    }
    
    // Use eval to avoid bundling issues
    const fs = await eval('import("fs/promises")');
    
    switch (operation) {
      case DirectoryOperation.GET_ROOT_DIRECTORY:
        const homeDir = process.env.HOME || process.env.USERPROFILE || process.cwd();
        const rootPath = pathUtils.join(homeDir, '.continuum');
        await fs.mkdir(rootPath, { recursive: true });
        return { rootPath };
      default:
        throw new Error(`Unknown directory operation: ${operation}`);
    }
  }
}