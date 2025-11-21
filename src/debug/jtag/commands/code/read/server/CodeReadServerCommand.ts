/**
 * code/read server command - Read source code files
 */

import * as fs from 'fs';
import * as path from 'path';
import { promisify } from 'util';
import minimatch from 'minimatch';

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { CodeDaemon } from '../../../../daemons/code-daemon/shared/CodeDaemon';
import type { CodeReadParams, CodeReadResult } from '../shared/CodeReadTypes';
import { createCodeReadResultFromParams } from '../shared/CodeReadTypes';
import { CodeReadCommand } from '../shared/CodeReadCommand';

const stat = promisify(fs.stat);
const readdir = promisify(fs.readdir);

export class CodeReadServerCommand extends CodeReadCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code-read', context, subpath, commander);
  }

  /**
   * Execute code/read command
   *
   * Delegates to CodeDaemon.readFile() static method
   * If exact path fails, tries fuzzy matching to find similar files
   */
  protected async executeCommand(params: CodeReadParams): Promise<CodeReadResult> {
    // Validate params
    if (!params.path) {
      return createCodeReadResultFromParams(params, {
        success: false,
        error: 'Missing required parameter: path'
      });
    }

    console.log(`📂 CODE SERVER: Reading file ${params.path} via CodeDaemon`);

    try {
      // Try exact path first
      const result = await CodeDaemon.readFile(params.path, {
        startLine: params.startLine,
        endLine: params.endLine,
        includeMetadata: params.includeMetadata,
        forceRefresh: params.forceRefresh
      });

      if (result.success) {
        console.log(`✅ CODE SERVER: Read ${params.path} (${result.metadata.linesReturned} lines)`);
        return createCodeReadResultFromParams(params, result);
      }

      // If exact path failed, try fuzzy matching
      console.log(`🔍 CODE SERVER: Exact path failed, trying fuzzy match for ${params.path}`);
      const matches = await this.findSimilarFiles(params.path);

      if (matches.length === 0) {
        console.log(`❌ CODE SERVER: No similar files found for ${params.path}`);
        return createCodeReadResultFromParams(params, {
          success: false,
          error: `File not found: ${params.path}. No similar files found.`
        });
      }

      if (matches.length === 1) {
        // Exactly one match - read it automatically
        console.log(`✅ CODE SERVER: Found exact fuzzy match: ${matches[0]}`);
        const fuzzyResult = await CodeDaemon.readFile(matches[0], {
          startLine: params.startLine,
          endLine: params.endLine,
          includeMetadata: params.includeMetadata,
          forceRefresh: params.forceRefresh
        });

        if (fuzzyResult.success) {
          console.log(`✅ CODE SERVER: Read fuzzy match ${matches[0]} (${fuzzyResult.metadata.linesReturned} lines)`);
        }

        return createCodeReadResultFromParams(params, fuzzyResult);
      }

      // Multiple matches - return suggestions
      console.log(`❓ CODE SERVER: Found ${matches.length} similar files for ${params.path}`);
      const suggestionsList = matches.slice(0, 10).map((m, i) => `${i + 1}. ${m}`).join('\n');
      return createCodeReadResultFromParams(params, {
        success: false,
        error: `File not found: ${params.path}.\n\nDid you mean one of these?\n${suggestionsList}\n\nPlease try again with the full path.`
      });
    } catch (error) {
      console.error(`❌ CODE SERVER: Exception reading ${params.path}:`, error);

      return createCodeReadResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  /**
   * Find files with similar names using fuzzy matching
   * Searches for files that contain the given filename pattern
   */
  private async findSimilarFiles(partialPath: string): Promise<string[]> {
    try {
      const repositoryRoot = CodeDaemon.getRepositoryRoot();

      // Extract the filename from the partial path
      const basename = path.basename(partialPath);
      const dirname = path.dirname(partialPath);

      // Create a case-insensitive glob pattern
      const pattern = `*${basename}*`;

      const matches: string[] = [];
      const startTime = Date.now();
      const TIMEOUT_MS = 5000; // 5 second timeout
      const MAX_DEPTH = 10; // Maximum directory depth

      // If a directory was specified, search only in that directory
      if (dirname && dirname !== '.' && dirname !== '/') {
        const searchPath = path.join(repositoryRoot, dirname);
        try {
          await stat(searchPath);
          await this.searchDirectoryForPattern(searchPath, repositoryRoot, pattern, matches, 50, 0, MAX_DEPTH, startTime, TIMEOUT_MS);
        } catch {
          // Directory doesn't exist, fall through to repo-wide search
        }
      }

      // If no matches in specified directory (or no directory specified), search entire repo
      if (matches.length === 0) {
        await this.searchDirectoryForPattern(repositoryRoot, repositoryRoot, pattern, matches, 50, 0, MAX_DEPTH, startTime, TIMEOUT_MS);
      }

      return matches;
    } catch (error) {
      console.warn(`⚠️ CODE SERVER: Error in fuzzy file search:`, error);
      return [];
    }
  }

  /**
   * Recursively search directory for files matching pattern
   * @param depth Current depth in directory tree
   * @param maxDepth Maximum depth to search (prevents deep recursion)
   * @param startTime Start time of search (for timeout check)
   * @param timeoutMs Maximum time to search in milliseconds
   */
  private async searchDirectoryForPattern(
    dirPath: string,
    repoRoot: string,
    pattern: string,
    matches: string[],
    maxResults: number,
    depth: number = 0,
    maxDepth: number = 10,
    startTime: number = Date.now(),
    timeoutMs: number = 5000
  ): Promise<void> {
    // Performance limits
    if (matches.length >= maxResults) return;
    if (depth > maxDepth) return;
    if (Date.now() - startTime > timeoutMs) {
      console.warn(`⚠️ CODE SERVER: Fuzzy search timeout after ${timeoutMs}ms at depth ${depth}`);
      return;
    }

    try {
      const entries = await readdir(dirPath, { withFileTypes: true });

      for (const entry of entries) {
        if (matches.length >= maxResults) break;
        if (Date.now() - startTime > timeoutMs) break;

        // Skip hidden files/directories and node_modules
        if (entry.name.startsWith('.') || entry.name === 'node_modules') continue;

        const fullPath = path.join(dirPath, entry.name);
        const relativePath = path.relative(repoRoot, fullPath);

        // Check if filename matches pattern (case-insensitive)
        if (entry.isFile() && minimatch(entry.name.toLowerCase(), pattern.toLowerCase())) {
          matches.push(relativePath);
        }

        // Recursively search subdirectories (with updated depth)
        if (entry.isDirectory() && matches.length < maxResults) {
          await this.searchDirectoryForPattern(fullPath, repoRoot, pattern, matches, maxResults, depth + 1, maxDepth, startTime, timeoutMs);
        }
      }
    } catch {
      // Silently skip directories we can't read
    }
  }
}
