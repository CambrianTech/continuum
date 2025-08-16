/**
 * Artifacts Daemon Server - Filesystem Operations Implementation
 * 
 * Server-side implementation of ArtifactsDaemon that handles actual filesystem operations.
 * Enforces .continuum directory structure and provides atomic file operations.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { ArtifactsDaemon, type ArtifactsPayload, type ArtifactsResult } from '../shared/ArtifactsDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

/**
 * Server Artifacts Daemon - Real filesystem operations
 */
export class ArtifactsDaemonServer extends ArtifactsDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Read file content
   */
  protected async handleRead(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.sessionId);
      const encoding = payload.options?.encoding || 'utf8';
      
      const content = await fs.readFile(fullPath, encoding);
      
      return {
        success: true,
        data: content,
        fullPath,
        bytesProcessed: Buffer.byteLength(content.toString(), encoding)
      };
      
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return {
          success: true,
          data: undefined,
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
        };
      }
      
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
      };
    }
  }

  /**
   * Write file content with atomic operations
   */
  protected async handleWrite(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.sessionId);
      const content = payload.content || '';
      
      // Ensure directory exists
      if (payload.options?.createDirectories !== false) {
        await fs.mkdir(path.dirname(fullPath), { recursive: true });
      }
      
      let bytesWritten = 0;
      
      if (payload.options?.atomicWrite !== false) {
        // Atomic write via temp file
        const tempPath = `${fullPath}.tmp`;
        if (Buffer.isBuffer(content)) {
          await fs.writeFile(tempPath, content);
          bytesWritten = content.length;
        } else {
          const encoding = payload.options?.encoding || 'utf8';
          await fs.writeFile(tempPath, content, encoding);
          bytesWritten = Buffer.byteLength(content, encoding);
        }
        await fs.rename(tempPath, fullPath);
      } else {
        // Direct write
        if (Buffer.isBuffer(content)) {
          await fs.writeFile(fullPath, content);
          bytesWritten = content.length;
        } else {
          const encoding = payload.options?.encoding || 'utf8';
          await fs.writeFile(fullPath, content, encoding);
          bytesWritten = Buffer.byteLength(content, encoding);
        }
      }
      
      return {
        success: true,
        fullPath,
        bytesProcessed: bytesWritten
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
      };
    }
  }

  /**
   * Append to file
   */
  protected async handleAppend(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.sessionId);
      const content = payload.content || '';
      
      // Ensure directory exists
      if (payload.options?.createDirectories !== false) {
        await fs.mkdir(path.dirname(fullPath), { recursive: true });
      }
      
      let bytesWritten = 0;
      if (Buffer.isBuffer(content)) {
        await fs.appendFile(fullPath, content);
        bytesWritten = content.length;
      } else {
        const encoding = payload.options?.encoding || 'utf8';
        await fs.appendFile(fullPath, content, encoding);
        bytesWritten = Buffer.byteLength(content, encoding);
      }
      
      return {
        success: true,
        fullPath,
        bytesProcessed: bytesWritten
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
      };
    }
  }

  /**
   * Create directory
   */
  protected async handleMkdir(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.sessionId);
      
      await fs.mkdir(fullPath, { recursive: payload.options?.createDirectories !== false });
      
      return {
        success: true,
        fullPath
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
      };
    }
  }

  /**
   * List directory contents
   */
  protected async handleList(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.sessionId);
      
      const entries = await fs.readdir(fullPath, { withFileTypes: true });
      const files = entries.map(entry => ({
        name: entry.name,
        isDirectory: entry.isDirectory(),
        isFile: entry.isFile()
      }));
      
      return {
        success: true,
        data: files,
        fullPath
      };
      
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return {
          success: true,
          data: [],
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
        };
      }
      
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
      };
    }
  }

  /**
   * Get file/directory statistics
   */
  protected async handleStat(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.sessionId);
      
      const stats = await fs.stat(fullPath);
      
      return {
        success: true,
        data: {
          isFile: stats.isFile(),
          isDirectory: stats.isDirectory(),
          size: stats.size,
          createdAt: stats.birthtime.toISOString(),
          modifiedAt: stats.mtime.toISOString()
        },
        fullPath
      };
      
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return {
          success: true,
          data: undefined,
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
        };
      }
      
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
      };
    }
  }

  /**
   * Delete file or directory
   */
  protected async handleDelete(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.sessionId);
      
      const stats = await fs.stat(fullPath);
      
      if (stats.isDirectory()) {
        await fs.rmdir(fullPath, { recursive: true });
      } else {
        await fs.unlink(fullPath);
      }
      
      return {
        success: true,
        fullPath
      };
      
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return {
          success: true,
          data: false, // File didn't exist
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
        };
      }
      
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.sessionId)
      };
    }
  }

  toString(): string {
    return `ArtifactsDaemon[${this.context.environment}]`;
  }
}