/**
 * CodeDaemon Server - JTAG Integration
 *
 * Server-side implementation that replaces CodeDaemon static methods
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { CodeDaemon } from '../shared/CodeDaemon';
import type {
  CodeDaemonConfig,
  CodeReadOptions,
  CodeReadResult,
  CodeSearchOptions,
  CodeSearchResult,
  GitLogOptions,
  GitLogResult,
  CodeFileReadEvent,
  CodeSearchEvent,
  CodeGitLogEvent,
  WorkspaceEditMode,
  WorkspaceWriteResult,
  WorkspaceReadResult,
  WorkspaceSearchResult,
  WorkspaceTreeResult,
  WorkspaceUndoResult,
  WorkspaceHistoryResult,
  WorkspaceGitStatusInfo,
} from '../shared/CodeDaemonTypes';
import { Events } from '../../../system/core/shared/Events';
import { PathValidator } from './modules/PathValidator';
import { FileReader } from './modules/FileReader';
import { Logger } from '../../../system/core/logging/Logger';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import * as path from 'path';

/**
 * Server-side implementation of CodeDaemon
 */
class CodeDaemonImpl {
  private pathValidator: PathValidator;
  private fileReader: FileReader;
  private config: CodeDaemonConfig;
  private jtagContext: JTAGContext;
  private isInitialized: boolean = false;

  constructor(jtagContext: JTAGContext, config: CodeDaemonConfig) {
    this.jtagContext = jtagContext;
    this.config = config;
    this.pathValidator = new PathValidator(config.repositoryRoot);
    this.fileReader = new FileReader(
      this.pathValidator,
      config.maxFileSize,
      config.enableCache,
      config.cacheTTL
    );
    this.isInitialized = true;
  }

  async readFile(filePath: string, options?: CodeReadOptions): Promise<CodeReadResult> {
    const result = await this.fileReader.read(filePath, options);

    // Emit event
    if (result.success) {
      await Events.emit<CodeFileReadEvent>(this.jtagContext, 'code:file:read', {
        path: filePath,
        size: result.metadata.size,
        cached: result.cached || false,
        timestamp: Date.now()
      });
    }

    return result;
  }

  async searchCode(pattern: string, options?: CodeSearchOptions): Promise<CodeSearchResult> {
    // TODO: Implement search
    return {
      success: false,
      pattern,
      matches: [],
      totalMatches: 0,
      filesSearched: 0,
      error: 'Search not yet implemented'
    };
  }

  async getGitLog(options?: GitLogOptions): Promise<GitLogResult> {
    // TODO: Implement git log
    return {
      success: false,
      commits: [],
      error: 'Git log not yet implemented'
    };
  }

  clearCache(): void {
    this.fileReader.clearCache();
  }

  getCacheStats(): { entries: number; size: number } {
    return this.fileReader.getCacheStats();
  }

  getRepositoryRoot(): string {
    return this.config.repositoryRoot;
  }

  getIsInitialized(): boolean {
    return this.isInitialized;
  }
}

// Singleton instance
let codeDaemonInstance: CodeDaemonImpl | undefined;

/**
 * Initialize CodeDaemon for server usage
 */
export async function initializeCodeDaemon(jtagContext: JTAGContext): Promise<void> {
  const log = Logger.create('CodeDaemonServer', 'daemons/CodeDaemonServer');
  log.info('Initializing CodeDaemon...');

  // Determine repository root (go up from daemons/code-daemon/server to jtag root)
  const repositoryRoot = path.resolve(__dirname, '../../..');

  const config: CodeDaemonConfig = {
    repositoryRoot,
    maxFileSize: 10 * 1024 * 1024, // 10MB
    enableCache: true,
    cacheTTL: 60000, // 1 minute
    rateLimit: 100, // 100 ops/minute
    enableAudit: true
  };

  // Create implementation instance
  codeDaemonInstance = new CodeDaemonImpl(jtagContext, config);

  // Replace static methods on CodeDaemon class
  CodeDaemon.readFile = async (filePath: string, options?: CodeReadOptions) => {
    if (!codeDaemonInstance) throw new Error('CodeDaemon not initialized');
    return await codeDaemonInstance.readFile(filePath, options);
  };

  CodeDaemon.searchCode = async (pattern: string, options?: CodeSearchOptions) => {
    if (!codeDaemonInstance) throw new Error('CodeDaemon not initialized');
    return await codeDaemonInstance.searchCode(pattern, options);
  };

  CodeDaemon.getGitLog = async (options?: GitLogOptions) => {
    if (!codeDaemonInstance) throw new Error('CodeDaemon not initialized');
    return await codeDaemonInstance.getGitLog(options);
  };

  CodeDaemon.clearCache = () => {
    if (!codeDaemonInstance) throw new Error('CodeDaemon not initialized');
    codeDaemonInstance.clearCache();
  };

  CodeDaemon.getCacheStats = () => {
    if (!codeDaemonInstance) throw new Error('CodeDaemon not initialized');
    return codeDaemonInstance.getCacheStats();
  };

  CodeDaemon.getRepositoryRoot = () => {
    if (!codeDaemonInstance) throw new Error('CodeDaemon not initialized');
    return codeDaemonInstance.getRepositoryRoot();
  };

  CodeDaemon.isInitialized = () => {
    return codeDaemonInstance?.getIsInitialized() || false;
  };

  // ========================================================================
  // Workspace-Scoped Operations (Rust IPC backed)
  // ========================================================================

  const rustClient = new RustCoreIPCClient('/tmp/continuum-core.sock');
  try {
    await rustClient.connect();
    log.info('Connected to continuum-core for workspace operations');
  } catch (err) {
    log.warn('continuum-core not available — workspace operations will fail until Rust server starts');
  }

  CodeDaemon.createWorkspace = async (personaId: string, workspaceRoot: string, readRoots?: string[]) => {
    await rustClient.codeCreateWorkspace(personaId, workspaceRoot, readRoots);
  };

  CodeDaemon.workspaceRead = async (personaId: string, filePath: string, startLine?: number, endLine?: number) => {
    return await rustClient.codeRead(personaId, filePath, startLine, endLine);
  };

  CodeDaemon.workspaceWrite = async (personaId: string, filePath: string, content: string, description?: string) => {
    return await rustClient.codeWrite(personaId, filePath, content, description);
  };

  CodeDaemon.workspaceEdit = async (personaId: string, filePath: string, editMode: WorkspaceEditMode, description?: string) => {
    return await rustClient.codeEdit(personaId, filePath, editMode, description);
  };

  CodeDaemon.workspaceDelete = async (personaId: string, filePath: string, description?: string) => {
    return await rustClient.codeDelete(personaId, filePath, description);
  };

  CodeDaemon.workspaceDiff = async (personaId: string, filePath: string, editMode: WorkspaceEditMode) => {
    return await rustClient.codeDiff(personaId, filePath, editMode);
  };

  CodeDaemon.workspaceUndo = async (personaId: string, changeId?: string, count?: number) => {
    return await rustClient.codeUndo(personaId, changeId, count);
  };

  CodeDaemon.workspaceHistory = async (personaId: string, filePath?: string, limit?: number) => {
    return await rustClient.codeHistory(personaId, filePath, limit);
  };

  CodeDaemon.workspaceSearch = async (personaId: string, pattern: string, fileGlob?: string, maxResults?: number) => {
    return await rustClient.codeSearch(personaId, pattern, fileGlob, maxResults);
  };

  CodeDaemon.workspaceTree = async (personaId: string, treePath?: string, maxDepth?: number, includeHidden?: boolean) => {
    return await rustClient.codeTree(personaId, treePath, maxDepth, includeHidden);
  };

  CodeDaemon.workspaceGitStatus = async (personaId: string) => {
    return await rustClient.codeGitStatus(personaId);
  };

  CodeDaemon.workspaceGitDiff = async (personaId: string, staged?: boolean) => {
    return await rustClient.codeGitDiff(personaId, staged);
  };

  log.info(`Initialized successfully (repository root: ${repositoryRoot})`);
}
