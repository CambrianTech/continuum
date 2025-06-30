/**
 * Continuum Directory Daemon - Intelligent .continuum directory management
 * 
 * RESPONSIBILITIES:
 * - Maintain organized .continuum directory structure
 * - Provide intelligent save locations for artifacts
 * - Handle directory lifecycle and cleanup policies
 * - Ensure consistent organization across all sessions
 * - Provide directory analytics and health monitoring
 * 
 * FUTURE: AI/Persona will be tasked with this - learning optimal organization patterns
 * from usage, predicting storage needs, and automatically organizing old artifacts
 */

import { BaseDaemon } from '../base/BaseDaemon.js';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol.js';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface DirectoryRequest {
  type: 'session' | 'artifact' | 'config' | 'cache' | 'logs' | 'temp';
  context: {
    sessionType?: 'portal' | 'validation' | 'user' | 'persona';
    owner?: string;
    sessionId?: string;
    artifactType?: 'screenshot' | 'log' | 'recording' | 'file' | 'devtools';
    timestamp?: Date;
    metadata?: Record<string, any>;
  };
}

export interface DirectoryResponse {
  path: string;
  created: boolean;
  structure: string[];
  metadata: {
    type: string;
    permissions: string;
    estimatedSize: string;
    retentionPolicy: string;
  };
}

export interface DirectoryStats {
  totalSize: number;
  sessionCount: number;
  artifactCount: number;
  oldestSession: Date | null;
  newestSession: Date | null;
  byType: Record<string, number>;
  healthStatus: 'healthy' | 'warning' | 'critical';
}

export class ContinuumDirectoryDaemon extends BaseDaemon {
  public readonly name = 'continuum-directory';
  public readonly version = '1.0.0';
  
  private continuumRoot: string;
  private directoryPolicies: Map<string, any> = new Map();
  private statsCache: DirectoryStats | null = null;
  private statsCacheExpiry: Date | null = null;

  constructor(continuumRoot: string = '.continuum') {
    super();
    this.continuumRoot = path.resolve(continuumRoot);
    this.initializePolicies();
  }

  protected async onStart(): Promise<void> {
    this.log('📁 Starting Continuum Directory Daemon...');
    await this.initializeDirectoryStructure();
    await this.validateDirectoryHealth();
    this.log('✅ Continuum directory management ready');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Continuum Directory Daemon...');
    // Ensure any pending writes complete
    await this.flushPendingOperations();
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    try {
      switch (message.type) {
        case 'get_directory':
          return await this.handleGetDirectory(message.data);
          
        case 'create_session_directory':
          return await this.handleCreateSessionDirectory(message.data);
          
        case 'get_artifact_location':
          return await this.handleGetArtifactLocation(message.data);
          
        case 'cleanup_old_sessions':
          return await this.handleCleanupOldSessions(message.data);
          
        case 'get_directory_stats':
          return await this.handleGetDirectoryStats();
          
        case 'validate_directory_health':
          return await this.handleValidateDirectoryHealth();
          
        case 'organize_artifacts':
          return await this.handleOrganizeArtifacts(message.data);
          
        default:
          return {
            success: false,
            error: `Unknown message type: ${message.type}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Directory management failed: ${errorMessage}`
      };
    }
  }

  /**
   * Handle directory request
   */
  private async handleGetDirectory(data: DirectoryRequest): Promise<DaemonResponse> {
    try {
      const response = await this.getIntelligentDirectory(data);
      
      return {
        success: true,
        data: response
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle session directory creation
   */
  private async handleCreateSessionDirectory(data: any): Promise<DaemonResponse> {
    const { sessionType, owner, sessionId, context } = data;
    
    if (!sessionType || !owner || !sessionId) {
      return {
        success: false,
        error: 'sessionType, owner, and sessionId are required'
      };
    }

    try {
      const sessionPath = await this.createSessionDirectory(sessionType, owner, sessionId, context);
      
      return {
        success: true,
        data: {
          sessionPath,
          artifactPaths: await this.getSessionArtifactPaths(sessionPath),
          metadata: await this.getSessionMetadata(sessionPath)
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle artifact location request
   */
  private async handleGetArtifactLocation(data: any): Promise<DaemonResponse> {
    const { sessionId, artifactType, filename, metadata } = data;
    
    try {
      const location = await this.getArtifactLocation(sessionId, artifactType, filename, metadata);
      
      return {
        success: true,
        data: {
          path: location.path,
          directory: path.dirname(location.path),
          filename: path.basename(location.path),
          created: location.created
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle old session cleanup
   */
  private async handleCleanupOldSessions(data: any): Promise<DaemonResponse> {
    const { maxAge = 7 * 24 * 60 * 60 * 1000, dryRun = false } = data; // Default 7 days
    
    try {
      const cleanup = await this.cleanupOldSessions(maxAge, dryRun);
      
      return {
        success: true,
        data: {
          sessionsFound: cleanup.sessionsFound,
          sessionsRemoved: cleanup.sessionsRemoved,
          spaceFreed: cleanup.spaceFreed,
          dryRun,
          removedSessions: cleanup.removedSessions
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle directory stats request
   */
  private async handleGetDirectoryStats(): Promise<DaemonResponse> {
    try {
      const stats = await this.getDirectoryStats();
      
      return {
        success: true,
        data: stats
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle directory health validation
   */
  private async handleValidateDirectoryHealth(): Promise<DaemonResponse> {
    try {
      const health = await this.validateDirectoryHealth();
      
      return {
        success: true,
        data: health
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle artifact organization
   */
  private async handleOrganizeArtifacts(data: any): Promise<DaemonResponse> {
    const { strategy = 'auto', sessionId } = data;
    
    try {
      const organization = await this.organizeArtifacts(strategy, sessionId);
      
      return {
        success: true,
        data: organization
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  // Core implementation methods

  /**
   * Get intelligent directory location based on request context
   */
  private async getIntelligentDirectory(request: DirectoryRequest): Promise<DirectoryResponse> {
    let directoryPath: string;
    
    switch (request.type) {
      case 'session':
        directoryPath = await this.getSessionDirectoryPath(request.context);
        break;
      case 'artifact':
        directoryPath = await this.getArtifactDirectoryPath(request.context);
        break;
      case 'config':
        directoryPath = path.join(this.continuumRoot, 'config');
        break;
      case 'cache':
        directoryPath = path.join(this.continuumRoot, 'cache');
        break;
      case 'logs':
        directoryPath = path.join(this.continuumRoot, 'logs');
        break;
      case 'temp':
        directoryPath = path.join(this.continuumRoot, 'temp');
        break;
      default:
        throw new Error(`Unknown directory type: ${request.type}`);
    }

    // Ensure directory exists
    const created = await this.ensureDirectoryExists(directoryPath);
    
    // Get directory structure info
    const structure = await this.getDirectoryStructure(directoryPath);
    
    return {
      path: directoryPath,
      created,
      structure,
      metadata: {
        type: request.type,
        permissions: '755',
        estimatedSize: await this.estimateDirectorySize(directoryPath),
        retentionPolicy: this.getRetentionPolicy(request.type)
      }
    };
  }

  /**
   * Create session directory with intelligent organization
   */
  private async createSessionDirectory(
    sessionType: string,
    owner: string,
    sessionId: string,
    context?: any
  ): Promise<string> {
    let sessionPath: string;
    
    switch (sessionType) {
      case 'portal':
        sessionPath = path.join(this.continuumRoot, 'sessions', 'portal', sessionId);
        break;
      case 'validation':
        // Git hooks with branch context
        const branchContext = context?.branch ? `-${context.branch}` : '';
        sessionPath = path.join(this.continuumRoot, 'sessions', 'validation', `${sessionId}${branchContext}`);
        break;
      case 'user':
        sessionPath = path.join(this.continuumRoot, 'sessions', 'user', owner, sessionId);
        break;
      case 'persona':
        sessionPath = path.join(this.continuumRoot, 'sessions', 'personas', owner, sessionId);
        break;
      default:
        sessionPath = path.join(this.continuumRoot, 'sessions', 'misc', sessionId);
    }

    // Create directory structure
    await fs.mkdir(sessionPath, { recursive: true });
    
    // Create standard subdirectories
    const subdirs = ['artifacts', 'logs', 'screenshots', 'recordings', 'files', 'devtools', 'metadata'];
    for (const subdir of subdirs) {
      await fs.mkdir(path.join(sessionPath, subdir), { recursive: true });
    }

    // Create session metadata
    const metadata = {
      sessionId,
      sessionType,
      owner,
      context,
      created: new Date().toISOString(),
      structure: subdirs,
      policies: this.getSessionPolicies(sessionType)
    };

    await fs.writeFile(
      path.join(sessionPath, 'session.json'),
      JSON.stringify(metadata, null, 2)
    );

    this.log(`📁 Created session directory: ${sessionPath}`);
    return sessionPath;
  }

  /**
   * Get artifact location with intelligent naming
   */
  private async getArtifactLocation(
    sessionId: string,
    artifactType: string,
    filename?: string,
    metadata?: any
  ): Promise<{ path: string; created: boolean }> {
    // Find session directory
    const sessionPath = await this.findSessionPath(sessionId);
    if (!sessionPath) {
      throw new Error(`Session ${sessionId} not found`);
    }

    // Determine artifact subdirectory
    const artifactDir = path.join(sessionPath, this.getArtifactSubdirectory(artifactType));
    
    // Ensure artifact directory exists
    const created = await this.ensureDirectoryExists(artifactDir);
    
    // Generate intelligent filename if not provided
    if (!filename) {
      filename = this.generateArtifactFilename(artifactType, metadata);
    }

    const fullPath = path.join(artifactDir, filename);
    
    return { path: fullPath, created };
  }

  /**
   * Initialize directory structure
   */
  private async initializeDirectoryStructure(): Promise<void> {
    const baseDirs = [
      'sessions/portal',
      'sessions/validation', 
      'sessions/user',
      'sessions/personas',
      'config',
      'cache',
      'logs',
      'temp',
      'backups'
    ];

    for (const dir of baseDirs) {
      await fs.mkdir(path.join(this.continuumRoot, dir), { recursive: true });
    }

    // Create directory metadata
    const metadata = {
      version: this.version,
      created: new Date().toISOString(),
      structure: baseDirs,
      policies: Object.fromEntries(this.directoryPolicies)
    };

    await fs.writeFile(
      path.join(this.continuumRoot, 'directory.json'),
      JSON.stringify(metadata, null, 2)
    );

    this.log('📁 Continuum directory structure initialized');
  }

  /**
   * Initialize directory policies
   */
  private initializePolicies(): void {
    this.directoryPolicies.set('portal', {
      retention: '7 days',
      autoCleanup: true,
      compression: false
    });
    
    this.directoryPolicies.set('validation', {
      retention: '30 days',
      autoCleanup: true,
      compression: true
    });
    
    this.directoryPolicies.set('user', {
      retention: 'manual',
      autoCleanup: false,
      compression: false
    });
    
    this.directoryPolicies.set('persona', {
      retention: '14 days',
      autoCleanup: true,
      compression: true
    });
  }

  // Helper methods

  private async getSessionDirectoryPath(context: any): Promise<string> {
    const { sessionType, owner, sessionId } = context;
    return path.join(this.continuumRoot, 'sessions', sessionType, owner || '', sessionId || '');
  }

  private async getArtifactDirectoryPath(context: any): Promise<string> {
    const { sessionId, artifactType } = context;
    const sessionPath = await this.findSessionPath(sessionId);
    return path.join(sessionPath, this.getArtifactSubdirectory(artifactType));
  }

  private getArtifactSubdirectory(artifactType: string): string {
    switch (artifactType) {
      case 'screenshot': return 'screenshots';
      case 'log': return 'logs';
      case 'recording': return 'recordings';
      case 'file': return 'files';
      case 'devtools': return 'devtools';
      default: return 'artifacts';
    }
  }

  private generateArtifactFilename(artifactType: string, metadata?: any): string {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const source = metadata?.source || 'unknown';
    const extension = this.getArtifactExtension(artifactType);
    return `${timestamp}-${source}-${artifactType}.${extension}`;
  }

  private getArtifactExtension(artifactType: string): string {
    switch (artifactType) {
      case 'screenshot': return 'png';
      case 'log': return 'log';
      case 'recording': return 'webm';
      case 'devtools': return 'json';
      default: return 'txt';
    }
  }

  private async ensureDirectoryExists(dirPath: string): Promise<boolean> {
    try {
      await fs.access(dirPath);
      return false; // Already existed
    } catch {
      await fs.mkdir(dirPath, { recursive: true });
      return true; // Created new
    }
  }

  private async findSessionPath(sessionId: string): Promise<string | null> {
    // Search for session in all session directories recursively
    const sessionTypes = ['portal', 'validation', 'user', 'personas'];
    
    for (const type of sessionTypes) {
      const typeDir = path.join(this.continuumRoot, 'sessions', type);
      const found = await this.searchSessionRecursively(typeDir, sessionId);
      if (found) {
        return found;
      }
    }
    
    return null;
  }

  /**
   * Recursively search for session directory by sessionId
   */
  private async searchSessionRecursively(dir: string, sessionId: string): Promise<string | null> {
    try {
      const items = await fs.readdir(dir, { withFileTypes: true });
      
      for (const item of items) {
        if (item.isDirectory()) {
          const itemPath = path.join(dir, item.name);
          
          // Check if this directory matches the sessionId
          if (item.name === sessionId || item.name.includes(sessionId)) {
            // Verify it's actually a session directory by checking for session.json
            const sessionJsonPath = path.join(itemPath, 'session.json');
            try {
              await fs.access(sessionJsonPath);
              return itemPath;
            } catch {
              // Not a session directory, continue searching
            }
          }
          
          // Recursively search subdirectories
          const found = await this.searchSessionRecursively(itemPath, sessionId);
          if (found) {
            return found;
          }
        }
      }
    } catch {
      // Directory doesn't exist or can't read
    }
    
    return null;
  }

  private async getDirectoryStats(): Promise<DirectoryStats> {
    // Implementation would scan directories and calculate stats
    return {
      totalSize: 0,
      sessionCount: 0,
      artifactCount: 0,
      oldestSession: null,
      newestSession: null,
      byType: {},
      healthStatus: 'healthy'
    };
  }

  private async validateDirectoryHealth(): Promise<any> {
    // Check directory permissions, disk space, corruption, etc.
    return {
      status: 'healthy',
      issues: [],
      recommendations: []
    };
  }

  private async cleanupOldSessions(maxAge: number, dryRun: boolean): Promise<any> {
    // Implementation would find and clean old sessions
    return {
      sessionsFound: 0,
      sessionsRemoved: 0,
      spaceFreed: 0,
      removedSessions: []
    };
  }

  private async organizeArtifacts(strategy: string, sessionId?: string): Promise<any> {
    // Future: AI will learn optimal organization patterns
    return {
      strategy,
      organized: 0,
      moved: 0,
      compressed: 0
    };
  }

  private getRetentionPolicy(type: string): string {
    return this.directoryPolicies.get(type)?.retention || '30 days';
  }

  private getSessionPolicies(sessionType: string): any {
    return this.directoryPolicies.get(sessionType) || {};
  }

  private async getDirectoryStructure(dirPath: string): Promise<string[]> {
    try {
      const items = await fs.readdir(dirPath);
      return items;
    } catch {
      return [];
    }
  }

  private async estimateDirectorySize(dirPath: string): Promise<string> {
    // Simplified - real implementation would calculate actual size
    return '< 1MB';
  }

  private async getSessionArtifactPaths(sessionPath: string): Promise<any> {
    const subdirs = ['artifacts', 'logs', 'screenshots', 'recordings', 'files', 'devtools'];
    const paths: any = {};
    
    for (const subdir of subdirs) {
      paths[subdir] = path.join(sessionPath, subdir);
    }
    
    return paths;
  }

  private async getSessionMetadata(sessionPath: string): Promise<any> {
    try {
      const metadataPath = path.join(sessionPath, 'session.json');
      const content = await fs.readFile(metadataPath, 'utf-8');
      return JSON.parse(content);
    } catch {
      return null;
    }
  }

  private async flushPendingOperations(): Promise<void> {
    // Ensure any pending filesystem operations complete
    // Implementation would track and complete pending operations
  }
}