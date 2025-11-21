/**
 * CodeDaemon - Static interface for code operations
 *
 * Environment-agnostic interface. All implementation is in server/.
 */

import type {
  CodeReadOptions,
  CodeReadResult,
  CodeSearchOptions,
  CodeSearchResult,
  GitLogOptions,
  GitLogResult
} from './CodeDaemonTypes';

/**
 * CodeDaemon - Static API for code operations
 *
 * All methods throw error if not initialized or called from wrong environment.
 * Implementation is in server/CodeDaemonImpl.ts
 */
export class CodeDaemon {
  /**
   * Read a file (STATIC METHOD - public API)
   */
  static async readFile(path: string, options?: CodeReadOptions): Promise<CodeReadResult> {
    throw new Error('CodeDaemon.readFile() must be implemented by server');
  }

  /**
   * Search code (STATIC METHOD - public API)
   */
  static async searchCode(pattern: string, options?: CodeSearchOptions): Promise<CodeSearchResult> {
    throw new Error('CodeDaemon.searchCode() must be implemented by server');
  }

  /**
   * Get git log (STATIC METHOD - public API)
   */
  static async getGitLog(options?: GitLogOptions): Promise<GitLogResult> {
    throw new Error('CodeDaemon.getGitLog() must be implemented by server');
  }

  /**
   * Clear file cache (STATIC METHOD)
   */
  static clearCache(): void {
    throw new Error('CodeDaemon.clearCache() must be implemented by server');
  }

  /**
   * Get cache stats (STATIC METHOD)
   */
  static getCacheStats(): { entries: number; size: number } {
    throw new Error('CodeDaemon.getCacheStats() must be implemented by server');
  }

  /**
   * Get repository root (STATIC METHOD)
   */
  static getRepositoryRoot(): string {
    throw new Error('CodeDaemon.getRepositoryRoot() must be implemented by server');
  }

  /**
   * Check if initialized (STATIC METHOD)
   */
  static isInitialized(): boolean {
    return false; // Overridden by server implementation
  }
}
