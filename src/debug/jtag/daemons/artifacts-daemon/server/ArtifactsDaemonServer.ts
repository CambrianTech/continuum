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
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';

/**
 * Server Artifacts Daemon - Real filesystem operations
 */
export class ArtifactsDaemonServer extends ArtifactsDaemon {

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Set up file-based logging using class name automatically
    // Logs go to .continuum/jtag/logs/system/daemons/{ClassName}.log
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Read file content
   */
  protected async handleRead(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId);
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
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
        };
      }

      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
      };
    }
  }

  /**
   * Write file content with atomic operations
   */
  protected async handleWrite(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId);
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
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
      };
    }
  }

  /**
   * Append to file
   */
  protected async handleAppend(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId);
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
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
      };
    }
  }

  /**
   * Create directory
   */
  protected async handleMkdir(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId);
      
      await fs.mkdir(fullPath, { recursive: payload.options?.createDirectories !== false });
      
      return {
        success: true,
        fullPath
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
      };
    }
  }

  /**
   * List directory contents
   */
  protected async handleList(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId);
      
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
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
        };
      }
      
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
      };
    }
  }

  /**
   * Get file/directory statistics
   */
  protected async handleStat(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId);
      
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
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
        };
      }
      
      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
      };
    }
  }

  /**
   * Delete file or directory
   */
  protected async handleDelete(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const fullPath = this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId);

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
          fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
        };
      }

      return {
        success: false,
        error: error.message,
        fullPath: this.validateAndResolvePath(payload.relativePath, payload.storageType, payload.sessionId, payload.personaId)
      };
    }
  }

  /**
   * Load environment variables from config.env
   */
  protected async handleLoadEnvironment(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      const configPath = this.validateAndResolvePath('config.env', 'config');

      let content: string;
      try {
        content = await fs.readFile(configPath, 'utf-8');
      } catch (error: any) {
        if (error.code === 'ENOENT') {
          return {
            success: true,
            data: { loaded: 0, message: 'No config.env found', path: configPath }
          };
        }
        throw error;
      }

      const lines = content.split('\n');
      let loaded = 0;
      const variables: Record<string, string> = {};

      for (const line of lines) {
        const trimmed = line.trim();

        // Skip empty lines and comments
        if (!trimmed || trimmed.startsWith('#')) continue;

        // Parse KEY=value
        const match = trimmed.match(/^([A-Z_][A-Z0-9_]*)=(.*)$/);
        if (match) {
          const [, key, value] = match;
          process.env[key] = value;
          variables[key] = value;
          loaded++;
          this.log.info(`🔑 Loaded env var: ${key}`);
        }
      }

      return {
        success: true,
        data: { loaded, variables, path: configPath }
      };

    } catch (error: any) {
      return {
        success: false,
        error: `Failed to load environment: ${error.message}`
      };
    }
  }

  toString(): string {
    return `ArtifactsDaemon[${this.context.environment}]`;
  }
}