/**
 * File Daemon - Centralized file operations with proper path resolution
 */

import * as fs from 'fs/promises';
import * as path from 'path';

export class FileDaemon {
  
  /**
   * Resolve file path - handles both project-relative and session-relative paths
   */
  private resolvePath(filepath: string, sessionId?: string): string {
    // If path is absolute, use as-is
    if (path.isAbsolute(filepath)) {
      return filepath;
    }
    
    // Try project-relative path first (for widgets, etc.)
    const projectPath = path.resolve(process.cwd(), filepath);
    if (this.fileExistsSync(projectPath)) {
      return projectPath;
    }
    
    // Fall back to session-relative path if session ID provided
    if (sessionId) {
      const sessionPath = path.resolve(`.continuum/jtag/sessions/user/${sessionId}`, filepath);
      return sessionPath;
    }
    
    // Default to project-relative
    return projectPath;
  }
  
  /**
   * Check if file exists synchronously
   */
  private fileExistsSync(filepath: string): boolean {
    try {
      require('fs').accessSync(filepath);
      return true;
    } catch {
      return false;
    }
  }
  
  /**
   * Load file content
   */
  async loadFile(filepath: string, sessionId?: string, encoding: BufferEncoding = 'utf8'): Promise<{
    success: boolean;
    content: string;
    bytesRead: number;
    resolvedPath: string;
    exists: boolean;
    error?: Error;
  }> {
    try {
      const resolvedPath = this.resolvePath(filepath, sessionId);
      // console.log(`📂 FileDaemon: Loading ${filepath} → ${resolvedPath}`);
      
      // Check if file exists
      try {
        await fs.access(resolvedPath);
      } catch {
        return {
          success: false,
          content: '',
          bytesRead: 0,
          resolvedPath,
          exists: false,
          error: new Error(`File not found: ${resolvedPath}`)
        };
      }
      
      // Read file
      const content = await fs.readFile(resolvedPath, { encoding });
      const stats = await fs.stat(resolvedPath);

      // console.log(`✅ FileDaemon: Loaded ${stats.size} bytes from ${resolvedPath}`);
      
      return {
        success: true,
        content,
        bytesRead: stats.size,
        resolvedPath,
        exists: true
      };
      
    } catch (error: any) {
      console.error(`❌ FileDaemon: Load failed:`, error.message);
      return {
        success: false,
        content: '',
        bytesRead: 0,
        resolvedPath: filepath,
        exists: false,
        error: error instanceof Error ? error : new Error(String(error))
      };
    }
  }
}