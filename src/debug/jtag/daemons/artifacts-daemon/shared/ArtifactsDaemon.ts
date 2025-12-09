/**
 * Artifacts Daemon - Filesystem Access Orchestration
 * 
 * Centralized filesystem access layer that enforces .continuum directory structure
 * and manages all file operations across the JTAG system.
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { type UUID } from '../../../system/core/types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';
import { createBaseResponse, type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

// Storage location types that ArtifactsDaemon manages
export type StorageType = 'database' | 'session' | 'system' | 'cache' | 'logs' | 'config' | 'persona';

// Storage base paths
export const STORAGE_PATHS = {
  DATABASE: '.continuum/database',
  SYSTEM: '.continuum/jtag/system',
  CACHE: '.continuum/cache',
  LOGS: '.continuum/logs',
  CONFIG: (homeDir: string) => `${homeDir}/.continuum`,
  SESSION: (sessionId: string) => `.continuum/jtag/sessions/user/${sessionId}`,
  PERSONA: (personaId: string) => `${process.env.HOME}/.continuum/personas/${personaId}`
} as const;

// Artifacts operation types
export interface ArtifactsPayload extends JTAGPayload {
  readonly operation: 'read' | 'write' | 'append' | 'mkdir' | 'list' | 'stat' | 'delete' | 'loadEnvironment';
  readonly relativePath: string;
  readonly storageType?: StorageType;  // Where to store/read from
  readonly personaId?: UUID;  // Required for persona storage type
  readonly content?: string | Buffer;
  readonly options?: {
    createDirectories?: boolean;
    atomicWrite?: boolean;
    encoding?: BufferEncoding;
  };
}

export const createArtifactsPayload = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ArtifactsPayload>, 'context' | 'sessionId'>
): ArtifactsPayload => createPayload(context, sessionId, {
  operation: data.operation ?? 'read',
  relativePath: data.relativePath ?? '',
  storageType: data.storageType ?? 'system',
  content: data.content,
  options: data.options ?? {},
  ...data
});

/**
 * Artifacts operation result
 */
export interface ArtifactsResult {
  readonly success: boolean;
  readonly data?: any;
  readonly fullPath?: string;
  readonly bytesProcessed?: number;
  readonly error?: string;
}

/**
 * Artifacts response type
 */
export interface ArtifactsResponse extends BaseResponsePayload {
  readonly operation: string;
  readonly result: ArtifactsResult;
}

export const createArtifactsResponse = (
  operation: string,
  result: ArtifactsResult,
  context: JTAGContext,
  sessionId: UUID
): ArtifactsResponse => createPayload(context, sessionId, {
  success: result.success,
  timestamp: new Date().toISOString(),
  operation,
  result
});

export const createArtifactsErrorResponse = (
  operation: string,
  error: string,
  context: JTAGContext,
  sessionId: UUID
): ArtifactsResponse => createPayload(context, sessionId, {
  success: false,
  timestamp: new Date().toISOString(),
  operation,
  result: { success: false, error }
});

/**
 * Universal Artifacts Handler - Symmetric daemon for filesystem orchestration
 */
export abstract class ArtifactsDaemon extends DaemonBase {
  public readonly subpath: string = 'artifacts';
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('artifacts-daemon', context, router);
  }

  /**
   * Initialize artifacts daemon
   */
  protected async initialize(): Promise<void> {
    this.log.info(`📁 ${this.toString()}: Artifacts daemon initialized`);
  }

  /**
   * Handle incoming artifacts messages (MessageSubscriber interface)
   */
  async handleMessage(message: JTAGMessage): Promise<ArtifactsResponse> {
    const artifactsPayload = message.payload as ArtifactsPayload;
    
    try {
      let result: ArtifactsResult;
      
      switch (artifactsPayload.operation) {
        case 'read':
          result = await this.handleRead(artifactsPayload);
          break;

        case 'write':
          result = await this.handleWrite(artifactsPayload);
          break;

        case 'append':
          result = await this.handleAppend(artifactsPayload);
          break;

        case 'mkdir':
          result = await this.handleMkdir(artifactsPayload);
          break;

        case 'list':
          result = await this.handleList(artifactsPayload);
          break;

        case 'stat':
          result = await this.handleStat(artifactsPayload);
          break;

        case 'delete':
          result = await this.handleDelete(artifactsPayload);
          break;

        case 'loadEnvironment':
          result = await this.handleLoadEnvironment(artifactsPayload);
          break;

        default:
          result = {
            success: false,
            error: `Unknown artifacts operation: ${artifactsPayload.operation}`
          };
      }
      
      return createArtifactsResponse(artifactsPayload.operation, result, artifactsPayload.context, artifactsPayload.sessionId || 'system');

    } catch (error: any) {
      this.log.error(`❌ ${this.toString()}: Operation failed:`, error.message);
      return createArtifactsErrorResponse(artifactsPayload.operation, error.message, artifactsPayload.context, artifactsPayload.sessionId || 'system');
    }
  }

  /**
   * Validate and resolve path within .continuum structure based on storage type
   */
  protected validateAndResolvePath(
    relativePath: string,
    storageType: StorageType = 'system',
    sessionId?: UUID,
    personaId?: UUID
  ): string {
    // Remove leading slash if present
    const cleanPath = relativePath.startsWith('/') ? relativePath.slice(1) : relativePath;

    // Build full path based on storage type
    let basePath: string;

    switch (storageType) {
      case 'database':
        basePath = STORAGE_PATHS.DATABASE;
        break;
      case 'session':
        if (!sessionId) throw new Error('Session storage requires sessionId');
        basePath = STORAGE_PATHS.SESSION(sessionId);
        break;
      case 'system':
        basePath = STORAGE_PATHS.SYSTEM;
        break;
      case 'cache':
        basePath = STORAGE_PATHS.CACHE;
        break;
      case 'logs':
        basePath = STORAGE_PATHS.LOGS;
        break;
      case 'config':
        if (!process.env.HOME) throw new Error('HOME environment variable not set');
        basePath = STORAGE_PATHS.CONFIG(process.env.HOME);
        break;
      case 'persona':
        if (!personaId) throw new Error('Persona storage requires personaId');
        basePath = STORAGE_PATHS.PERSONA(personaId);
        break;
      default:
        throw new Error(`Unknown storage type: ${storageType}`);
    }

    return `${basePath}/${cleanPath}`;
  }

  // Abstract methods for environment-specific implementations
  protected abstract handleRead(payload: ArtifactsPayload): Promise<ArtifactsResult>;
  protected abstract handleWrite(payload: ArtifactsPayload): Promise<ArtifactsResult>;
  protected abstract handleAppend(payload: ArtifactsPayload): Promise<ArtifactsResult>;
  protected abstract handleMkdir(payload: ArtifactsPayload): Promise<ArtifactsResult>;
  protected abstract handleList(payload: ArtifactsPayload): Promise<ArtifactsResult>;
  protected abstract handleStat(payload: ArtifactsPayload): Promise<ArtifactsResult>;
  protected abstract handleDelete(payload: ArtifactsPayload): Promise<ArtifactsResult>;
  protected abstract handleLoadEnvironment(payload: ArtifactsPayload): Promise<ArtifactsResult>;

  toString(): string {
    return `ArtifactsDaemon[${this.context.environment}]`;
  }
}