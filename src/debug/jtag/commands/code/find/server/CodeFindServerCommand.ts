/**
 * code/find server command - Find files by name pattern
 */

import * as fs from 'fs';
import * as path from 'path';
import { promisify } from 'util';
import minimatch from 'minimatch';

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { CodeDaemon } from '../../../../daemons/code-daemon/shared/CodeDaemon';
import type { CodeFindParams, CodeFindResult, FileMatch } from '../shared/CodeFindTypes';
import { createCodeFindResultFromParams } from '../shared/CodeFindTypes';
import { CodeFindCommand } from '../shared/CodeFindCommand';

const stat = promisify(fs.stat);
const readdir = promisify(fs.readdir);

export class CodeFindServerCommand extends CodeFindCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code-find', context, subpath, commander);
  }

  /**
   * Execute code/find command
   *
   * Searches for files by name pattern using minimatch (supports wildcards)
   */
  protected async executeCommand(params: CodeFindParams): Promise<CodeFindResult> {
    // Validate params
    if (!params.pattern || params.pattern.trim() === '') {
      return createCodeFindResultFromParams(params, {
        success: false,
        error: 'Missing required parameter: pattern'
      });
    }

    console.log(`🔍 CODE FIND SERVER: Searching for pattern "${params.pattern}"`);

    try {
      const repositoryRoot = CodeDaemon.getRepositoryRoot();
      const baseDir = params.baseDir ?? '.';
      const searchPath = path.join(repositoryRoot, baseDir);

      // Validate base directory exists
      try {
        const searchStat = await stat(searchPath);
        if (!searchStat.isDirectory()) {
          return createCodeFindResultFromParams(params, {
            success: false,
            error: `Base directory is not a directory: ${baseDir}`
          });
        }
      } catch {
        return createCodeFindResultFromParams(params, {
          success: false,
          error: `Base directory not found: ${baseDir}`
        });
      }

      const maxResults = params.maxResults ?? 50;
      const caseInsensitive = params.caseInsensitive !== false; // Default true
      const includeHidden = params.includeHidden === true; // Default false
      const excludeDirs = params.excludeDirs ?? ['node_modules', 'dist', '.continuum', '.git', 'examples/dist', 'coverage'];

      // Prepare pattern for minimatch
      const pattern = caseInsensitive ? params.pattern.toLowerCase() : params.pattern;

      // Find matching files
      const matches: FileMatch[] = [];
      let totalMatches = 0;

      await this.searchDirectory(
        searchPath,
        repositoryRoot,
        pattern,
        caseInsensitive,
        includeHidden,
        excludeDirs,
        matches,
        maxResults,
        () => totalMatches++
      );

      console.log(`✅ CODE FIND SERVER: Found ${totalMatches} matches for "${params.pattern}" (returning ${matches.length})`);

      return createCodeFindResultFromParams(params, {
        success: true,
        pattern: params.pattern,
        matches,
        totalMatches,
        baseDir
      });
    } catch (error) {
      console.error(`❌ CODE FIND SERVER: Exception searching for ${params.pattern}:`, error);

      return createCodeFindResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  /**
   * Recursively search directory for matching files
   */
  private async searchDirectory(
    dirPath: string,
    repoRoot: string,
    pattern: string,
    caseInsensitive: boolean,
    includeHidden: boolean,
    excludeDirs: string[],
    matches: FileMatch[],
    maxResults: number,
    onMatch: () => void
  ): Promise<void> {
    // Stop if we've reached max results
    if (matches.length >= maxResults) return;

    try {
      const entries = await readdir(dirPath, { withFileTypes: true });

      for (const entry of entries) {
        // Stop if we've reached max results
        if (matches.length >= maxResults) break;

        // Skip hidden files/directories if not requested
        if (!includeHidden && entry.name.startsWith('.')) continue;

        // Skip excluded directories (configurable, defaults to massive dirs that cause timeouts)
        if (excludeDirs.includes(entry.name)) {
          continue;
        }

        const fullPath = path.join(dirPath, entry.name);
        const relativePath = path.relative(repoRoot, fullPath);

        // Get file stats
        let fileStat;
        let fileType: 'file' | 'directory' | 'symlink' = 'file';
        try {
          fileStat = await stat(fullPath);
          if (fileStat.isDirectory()) fileType = 'directory';
          else if (fileStat.isSymbolicLink()) fileType = 'symlink';
        } catch {
          // Skip files we can't stat
          continue;
        }

        // Check if filename matches pattern
        const filename = caseInsensitive ? entry.name.toLowerCase() : entry.name;
        if (minimatch(filename, pattern)) {
          onMatch();

          if (matches.length < maxResults) {
            matches.push({
              path: relativePath,
              size: fileStat.size,
              modified: fileStat.mtime.toISOString(),
              type: fileType
            });
          }
        }

        // Recursively search subdirectories
        if (entry.isDirectory()) {
          await this.searchDirectory(
            fullPath,
            repoRoot,
            pattern,
            caseInsensitive,
            includeHidden,
            excludeDirs,
            matches,
            maxResults,
            onMatch
          );
        }
      }
    } catch (error) {
      // Silently skip directories we can't read (permissions, etc.)
      console.warn(`⚠️ CODE FIND SERVER: Cannot read directory ${dirPath}:`, error);
    }
  }
}
