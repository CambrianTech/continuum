/**
 * ArtifactsAPI - Elegant Filesystem Access Layer with Generic Typing
 *
 * Provides type-safe filesystem operations through ArtifactsDaemon with <T> generics.
 * Works in ANY environment (browser/server/Grid) via transparent daemon routing.
 *
 * Usage:
 * ```typescript
 * const artifacts = ArtifactsAPI.getInstance();
 *
 * // Type-safe JSON operations
 * const config = await artifacts.readJSON<Config>('config.json', 'config');
 * await artifacts.writeJSON('config.json', configData, 'config');
 *
 * // Raw file operations
 * const content = await artifacts.read('data.txt', 'system');
 * await artifacts.write('output.txt', content, 'system');
 *
 * // Environment loading
 * const env = await artifacts.loadEnvironment();
 * ```
 */

import { JTAGRouter } from '../router/shared/JTAGRouter';
import type { JTAGContext } from '../types/JTAGTypes';
import { JTAGMessageFactory } from '../types/JTAGTypes';
import type { StorageType } from '../../../daemons/artifacts-daemon/shared/ArtifactsDaemon';
import type { ArtifactsPayload, ArtifactsResult, ArtifactsResponse } from '../../../daemons/artifacts-daemon/shared/ArtifactsDaemon';
import { createArtifactsPayload } from '../../../daemons/artifacts-daemon/shared/ArtifactsDaemon';
import type { UUID } from '../types/CrossPlatformUUID';

/**
 * Environment loading result
 */
export interface EnvironmentLoadResult {
  readonly loaded: number;
  readonly variables?: Record<string, string>;
  readonly message?: string;
  readonly path?: string;
}

/**
 * File stat information
 */
export interface FileStats {
  readonly isFile: boolean;
  readonly isDirectory: boolean;
  readonly size: number;
  readonly createdAt: string;
  readonly modifiedAt: string;
}

/**
 * Directory entry information
 */
export interface DirectoryEntry {
  readonly name: string;
  readonly isDirectory: boolean;
  readonly isFile: boolean;
}

/**
 * Elegant filesystem API with generic typing support
 */
export class ArtifactsAPI {
  private static instance: ArtifactsAPI | null = null;

  private constructor(
    private readonly router: JTAGRouter,
    private readonly context: JTAGContext,
    private readonly sessionId: UUID
  ) {}

  /**
   * Get singleton instance (requires JTAGRouter context)
   */
  public static getInstance(router?: JTAGRouter, context?: JTAGContext, sessionId?: UUID): ArtifactsAPI {
    if (!ArtifactsAPI.instance) {
      if (!router || !context || !sessionId) {
        throw new Error('ArtifactsAPI requires router, context, and sessionId for first initialization');
      }
      ArtifactsAPI.instance = new ArtifactsAPI(router, context, sessionId);
    }
    return ArtifactsAPI.instance;
  }

  /**
   * Read file content as string
   */
  public async read(relativePath: string, storageType: StorageType = 'system'): Promise<string> {
    const result = await this.execute({
      operation: 'read',
      relativePath,
      storageType,
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to read ${relativePath}: ${result.error}`);
    }

    return result.data as string;
  }

  /**
   * Read and parse JSON file with type safety
   */
  public async readJSON<T>(relativePath: string, storageType: StorageType = 'system'): Promise<T> {
    const content = await this.read(relativePath, storageType);
    return JSON.parse(content) as T;
  }

  /**
   * Write file content
   */
  public async write(
    relativePath: string,
    content: string | Buffer,
    storageType: StorageType = 'system',
    options?: { createDirectories?: boolean; atomicWrite?: boolean; encoding?: BufferEncoding }
  ): Promise<void> {
    const result = await this.execute({
      operation: 'write',
      relativePath,
      content,
      storageType,
      options,
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to write ${relativePath}: ${result.error}`);
    }
  }

  /**
   * Write JSON data with type safety
   */
  public async writeJSON<T>(
    relativePath: string,
    data: T,
    storageType: StorageType = 'system',
    options?: { createDirectories?: boolean; atomicWrite?: boolean }
  ): Promise<void> {
    const content = JSON.stringify(data, null, 2);
    await this.write(relativePath, content, storageType, options);
  }

  /**
   * Append to file
   */
  public async append(
    relativePath: string,
    content: string | Buffer,
    storageType: StorageType = 'system',
    options?: { createDirectories?: boolean; encoding?: BufferEncoding }
  ): Promise<void> {
    const result = await this.execute({
      operation: 'append',
      relativePath,
      content,
      storageType,
      options,
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to append ${relativePath}: ${result.error}`);
    }
  }

  /**
   * Create directory
   */
  public async mkdir(relativePath: string, storageType: StorageType = 'system'): Promise<void> {
    const result = await this.execute({
      operation: 'mkdir',
      relativePath,
      storageType,
      options: { createDirectories: true },
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to mkdir ${relativePath}: ${result.error}`);
    }
  }

  /**
   * List directory contents
   */
  public async list(relativePath: string, storageType: StorageType = 'system'): Promise<DirectoryEntry[]> {
    const result = await this.execute({
      operation: 'list',
      relativePath,
      storageType,
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to list ${relativePath}: ${result.error}`);
    }

    return result.data as DirectoryEntry[];
  }

  /**
   * Get file/directory stats
   */
  public async stat(relativePath: string, storageType: StorageType = 'system'): Promise<FileStats | undefined> {
    const result = await this.execute({
      operation: 'stat',
      relativePath,
      storageType,
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to stat ${relativePath}: ${result.error}`);
    }

    return result.data as FileStats | undefined;
  }

  /**
   * Check if file/directory exists
   */
  public async exists(relativePath: string, storageType: StorageType = 'system'): Promise<boolean> {
    try {
      const stats = await this.stat(relativePath, storageType);
      return stats !== undefined;
    } catch {
      return false;
    }
  }

  /**
   * Delete file or directory
   */
  public async delete(relativePath: string, storageType: StorageType = 'system'): Promise<void> {
    const result = await this.execute({
      operation: 'delete',
      relativePath,
      storageType,
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to delete ${relativePath}: ${result.error}`);
    }
  }

  /**
   * Load environment variables from config.env
   */
  public async loadEnvironment(): Promise<EnvironmentLoadResult> {
    const result = await this.execute({
      operation: 'loadEnvironment',
      relativePath: 'config.env',
      storageType: 'config',
      context: this.context,
      sessionId: this.sessionId
    });

    if (!result.success) {
      throw new Error(`Failed to load environment: ${result.error}`);
    }

    return result.data as EnvironmentLoadResult;
  }

  /**
   * Execute artifacts operation through daemon
   */
  private async execute(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      this.context.environment,
      'artifacts',
      payload,
      `artifacts_${payload.operation}_${Date.now()}`
    );

    const response = await this.router.postMessage(message) as ArtifactsResponse;
    return response.result;
  }
}

/**
 * Convenience function to get ArtifactsAPI instance
 */
export function getArtifactsAPI(router?: JTAGRouter, context?: JTAGContext, sessionId?: UUID): ArtifactsAPI {
  return ArtifactsAPI.getInstance(router, context, sessionId);
}
