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

    // STEP 2: Query analysis - detect conceptual/semantic searches
    const queryAnalysis = this.analyzeQuery(params.pattern);
    if (queryAnalysis.isConceptual) {
      const suggestions = [
        `Your query "${params.pattern}" appears to be a semantic/conceptual search.`,
        '',
        'This tool (code/find) matches filename patterns, not code concepts or logic.',
        '',
        'For semantic code understanding, try:',
        '• Use ai/rag/query-open: Ask questions about code functionality',
        '  Example: "How does authentication flow work?"',
        '• Use code/read after finding relevant files',
        '',
        'Detection reasons:',
        ...queryAnalysis.reasons.map(r => `  • ${r}`),
        '',
        'If you meant to search for a filename pattern, try:',
        '• Use wildcards: "**/*.ts" for TypeScript files',
        '• Use exact names: "auth.ts" or "AuthService.ts"',
        '• Use simpler patterns: "*.test.ts" for test files'
      ];

      return createCodeFindResultFromParams(params, {
        success: true,
        pattern: params.pattern,
        matches: [],
        totalMatches: 0,
        baseDir: params.baseDir ?? '.',
        message: suggestions.join('\n')
      });
    }

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

      // If no matches found, provide helpful guidance
      if (totalMatches === 0) {
        const suggestions = [
          `No files found matching pattern "${params.pattern}".`,
          '',
          'Tips for better results:',
          '• Use simpler patterns: "*.ts" instead of "typescript files"',
          '• Try wildcards: "**/*.test.ts" for test files',
          '• Use exact filenames: "package.json"',
          '• Check your baseDir parameter (currently searching: ' + (baseDir ?? '.') + ')',
          '',
          'Note: This tool matches filename patterns, not file contents.',
          'To search code contents, use the code/read command after finding the file.'
        ];

        return createCodeFindResultFromParams(params, {
          success: true,
          pattern: params.pattern,
          matches: [],
          totalMatches: 0,
          baseDir,
          message: suggestions.join('\n')
        });
      }

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

  /**
   * Analyze query to detect if it's conceptual/semantic vs literal pattern matching
   * Based on AI team testing feedback and detection patterns
   */
  private analyzeQuery(pattern: string): { isConceptual: boolean; reasons: string[] } {
    const reasons: string[] = [];

    // Detect multi-word conceptual phrases
    const words = pattern.trim().split(/\s+/);
    if (words.length >= 2 && !pattern.includes('*') && !pattern.includes('?')) {
      // Check if it looks like a semantic query vs a filename pattern
      const hasCodeIndicators = /[A-Z][a-z]+|[a-z]+[A-Z]|[._-]|\.ts$|\.js$|\.py$/.test(pattern);
      if (!hasCodeIndicators) {
        reasons.push(`Multi-word phrase without file indicators: "${pattern}"`);
      }
    }

    // Detect question structures
    if (/^(how|what|where|why|when|who|which)\b/i.test(pattern)) {
      reasons.push(`Question word detected: ${pattern.split(/\s+/)[0].toLowerCase()}`);
    }

    // Detect abstract/conceptual terms (common semantic search patterns)
    const conceptualTerms = [
      'flow', 'logic', 'process', 'pattern', 'approach', 'mechanism',
      'system', 'strategy', 'implementation', 'algorithm', 'architecture',
      'structure', 'design', 'method', 'technique', 'concept', 'principle',
      'handling', 'management', 'processing', 'validation', 'authentication'
    ];

    const lowerPattern = pattern.toLowerCase();
    const matchedTerms = conceptualTerms.filter(term =>
      lowerPattern.includes(term) && !pattern.includes('*')
    );

    if (matchedTerms.length > 0) {
      reasons.push(`Conceptual terms found: ${matchedTerms.join(', ')}`);
    }

    // Detect descriptive phrases (adjective + noun patterns)
    if (words.length >= 2 && !/[*?[\]]/.test(pattern)) {
      const descriptivePatterns = /\b(user|error|data|file|auth|api|request|response|message|event|state|config|service|component|module|handler|manager|controller|model|view)\s+(handling|management|processing|validation|creation|deletion|update|retrieval|storage|flow|pattern|logic)\b/i;
      if (descriptivePatterns.test(pattern)) {
        reasons.push('Descriptive phrase detected (noun + verb pattern)');
      }
    }

    // If pattern has wildcards or file extensions, it's likely literal
    if (/[*?[\]]|\.(?:ts|js|py|java|go|rs|cpp|h)$/.test(pattern)) {
      return { isConceptual: false, reasons: [] };
    }

    // If pattern is PascalCase or camelCase, it's likely a filename
    if (/^[A-Z][a-z]+[A-Z]|^[a-z]+[A-Z]/.test(pattern)) {
      return { isConceptual: false, reasons: [] };
    }

    // Decision: conceptual if we have 2+ reasons
    return {
      isConceptual: reasons.length >= 2,
      reasons
    };
  }
}
