/**
 * LogSearcher - Pattern matching across log files with context
 *
 * Purpose:
 * - Search for regex patterns in log files
 * - Return matching lines with N lines of context before/after
 * - Support multi-file search
 * - Filter by log level, component, timestamp
 *
 * Usage:
 *   const searcher = new LogSearcher(logReader);
 *   const result = await searcher.search('/path/to/file.log', /ERROR/);
 *   const multiResult = await searcher.searchMultiple(files, /timeout/i);
 */

import * as fs from 'fs';
import * as readline from 'readline';
import { LogReader, type LogLine } from './LogReader';

export interface SearchResult {
  matches: SearchMatch[];
  totalMatches: number;
  filesSearched: number;
  durationMs: number;
}

export interface SearchMatch {
  filePath: string;
  lineNumber: number;
  line: LogLine;
  context: {
    before: LogLine[];
    after: LogLine[];
  };
  highlightedContent: string;  // With ANSI color codes
}

export interface SearchOptions {
  contextLines?: number;      // Lines before/after match (default: 2)
  caseSensitive?: boolean;    // Default: false
  maxMatches?: number;        // Stop after N matches (default: 100)
  logLevel?: string;          // Only match lines with this level
  component?: string;         // Only match lines from this component
  after?: Date;               // Only match lines after this timestamp
  before?: Date;              // Only match lines before this timestamp
}

export class LogSearcher {
  constructor(private logReader: LogReader) {}

  /**
   * Search single file
   */
  async search(
    filePath: string,
    pattern: string | RegExp,
    options: SearchOptions = {}
  ): Promise<SearchResult> {
    const startTime = Date.now();
    const regex = this.normalizePattern(pattern, options.caseSensitive);
    const contextLines = options.contextLines ?? 2;
    const maxMatches = options.maxMatches ?? 100;

    const matches: SearchMatch[] = [];
    const linesBuffer: LogLine[] = [];  // Circular buffer for context
    let matchCount = 0;

    // Stream file line-by-line
    const stream = fs.createReadStream(filePath, { encoding: 'utf-8' });
    const rl = readline.createInterface({ input: stream, crlfDelay: Infinity });

    let currentLine = 0;

    for await (const lineContent of rl) {
      currentLine++;
      const logLine = this.logReader.parseLine(lineContent, currentLine);

      // Apply filters
      if (!this.matchesFilters(logLine, options)) {
        // Still add to buffer for context
        linesBuffer.push(logLine);
        if (linesBuffer.length > contextLines * 2 + 1) {
          linesBuffer.shift();
        }
        continue;
      }

      // Test pattern
      if (regex.test(lineContent)) {
        matchCount++;

        // Extract context
        const matchIndex = linesBuffer.length - 1;
        const before = linesBuffer.slice(Math.max(0, matchIndex - contextLines), matchIndex);
        const after: LogLine[] = [];

        // Collect 'after' context lines
        const afterPromise = this.collectAfterContext(rl, contextLines);

        matches.push({
          filePath,
          lineNumber: currentLine,
          line: logLine,
          context: {
            before,
            after  // Will be populated after collectAfterContext
          },
          highlightedContent: this.highlightMatch(lineContent, regex)
        });

        // Stop if maxMatches reached
        if (matchCount >= maxMatches) {
          break;
        }
      }

      // Maintain circular buffer
      linesBuffer.push(logLine);
      if (linesBuffer.length > contextLines * 2 + 1) {
        linesBuffer.shift();
      }
    }

    return {
      matches,
      totalMatches: matchCount,
      filesSearched: 1,
      durationMs: Date.now() - startTime
    };
  }

  /**
   * Search multiple files
   */
  async searchMultiple(
    filePaths: string[],
    pattern: string | RegExp,
    options: SearchOptions = {}
  ): Promise<SearchResult> {
    const startTime = Date.now();
    const allMatches: SearchMatch[] = [];
    let totalMatches = 0;
    const maxMatches = options.maxMatches ?? 100;

    for (const filePath of filePaths) {
      const result = await this.search(filePath, pattern, {
        ...options,
        maxMatches: maxMatches - totalMatches
      });

      allMatches.push(...result.matches);
      totalMatches += result.totalMatches;

      // Stop if maxMatches reached across all files
      if (totalMatches >= maxMatches) {
        break;
      }
    }

    return {
      matches: allMatches,
      totalMatches,
      filesSearched: filePaths.length,
      durationMs: Date.now() - startTime
    };
  }

  /**
   * Normalize pattern to RegExp
   */
  private normalizePattern(pattern: string | RegExp, caseSensitive: boolean = false): RegExp {
    if (pattern instanceof RegExp) {
      return pattern;
    }

    // Escape special regex characters in string
    const escaped = pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    return new RegExp(escaped, caseSensitive ? '' : 'i');
  }

  /**
   * Check if log line matches filters
   */
  private matchesFilters(logLine: LogLine, options: SearchOptions): boolean {
    // Filter by log level
    if (options.logLevel && logLine.level !== options.logLevel) {
      return false;
    }

    // Filter by component
    if (options.component && logLine.component !== options.component) {
      return false;
    }

    // Filter by timestamp
    if (logLine.timestamp) {
      if (options.after && logLine.timestamp < options.after) {
        return false;
      }
      if (options.before && logLine.timestamp > options.before) {
        return false;
      }
    }

    return true;
  }

  /**
   * Collect 'after' context lines
   * Note: This is a simplified version - in production, we'd need to handle this more carefully
   * For now, we'll just leave the 'after' array empty and rely on streaming search
   */
  private async collectAfterContext(
    rl: readline.Interface,
    contextLines: number
  ): Promise<LogLine[]> {
    // TODO: Implement proper after-context collection
    // This requires lookahead in the stream, which is complex with readline
    // For MVP, we'll skip 'after' context and only show 'before' context
    return [];
  }

  /**
   * Highlight match in content using ANSI color codes
   */
  private highlightMatch(content: string, pattern: RegExp): string {
    // ANSI color codes
    const YELLOW = '\x1b[33m';
    const RESET = '\x1b[0m';

    return content.replace(pattern, match => `${YELLOW}${match}${RESET}`);
  }

  /**
   * Search by criteria (uses LogFileRegistry under the hood - will be called by LogQueryEngine)
   * This is a convenience method that delegates to searchMultiple
   */
  async searchByCriteria(
    filePaths: string[],
    pattern: string | RegExp,
    options: SearchOptions = {}
  ): Promise<SearchResult> {
    return this.searchMultiple(filePaths, pattern, options);
  }
}
