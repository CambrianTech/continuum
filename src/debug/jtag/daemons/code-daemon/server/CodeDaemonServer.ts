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
  CodeGitLogEvent
} from '../shared/CodeDaemonTypes';
import { Events } from '../../../system/core/shared/Events';
import { PathValidator } from './modules/PathValidator';
import { FileReader } from './modules/FileReader';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
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

  log.info(`Initialized successfully (repository root: ${repositoryRoot})`);
}
