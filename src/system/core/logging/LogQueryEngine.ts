/**
 * LogQueryEngine - High-level query orchestration
 *
 * Purpose:
 * - Compose LogFileRegistry, LogReader, LogSearcher, LogIterator for complex queries
 * - Implement common query patterns (e.g., "show me all errors in last hour")
 * - Provide simple API for commands to use
 *
 * Usage:
 *   const engine = new LogQueryEngine(registry, reader, searcher, iterator);
 *   const logs = await engine.listLogs({ category: 'persona' });
 *   const result = await engine.searchLogs({ pattern: /ERROR/, after: oneHourAgo });
 *   const stats = await engine.getStats();
 */

import { LogFileRegistry, type LogMetadata, type FilterCriteria } from './LogFileRegistry';
import { LogReader, type ReadResult, type LogLine } from './LogReader';
import { LogSearcher, type SearchResult, type SearchMatch, type SearchOptions } from './LogSearcher';
import { LogIterator, type IteratorHandle } from './LogIterator';

export interface QueryResult {
  logs: LogMetadata[];      // Matching log files
  lines: LogLine[];         // Matching lines
  matches: SearchMatch[];   // Search matches with context
  summary: QuerySummary;
}

export interface QuerySummary {
  totalFiles: number;
  totalLines: number;
  totalMatches: number;
  durationMs: number;
}

export interface LogQuery {
  pattern: string | RegExp;
  files?: string[];          // Specific files (or all if omitted)
  category?: string;         // Filter by category
  component?: string;        // Filter by component
  personaId?: string;        // Filter by persona
  logType?: string;          // Filter by log type (e.g., 'cognition', 'tools')
  logLevel?: string;         // Filter by log level
  after?: Date;              // Only lines after timestamp
  before?: Date;             // Only lines before timestamp
  contextLines?: number;     // Context around matches
  maxMatches?: number;       // Stop after N matches
}

export interface LogStats {
  totalFiles: number;
  totalSizeMB: number;
  byCategory: Record<string, number>;
  byComponent: Record<string, number>;
  largestFiles: Array<{ filePath: string; sizeMB: number }>;
  oldestFiles: Array<{ filePath: string; lastModified: Date; ageHours: number }>;
}

export class LogQueryEngine {
  constructor(
    private registry: LogFileRegistry,
    private reader: LogReader,
    private searcher: LogSearcher,
    private iterator: LogIterator
  ) {}

  /**
   * List all available logs with optional filtering
   */
  async listLogs(filter?: FilterCriteria): Promise<LogMetadata[]> {
    if (!filter) {
      return this.registry.discover();
    }
    return this.registry.filter(filter);
  }

  /**
   * Read from specific log
   */
  async readLog(
    filePath: string,
    startLine?: number,
    endLine?: number
  ): Promise<ReadResult> {
    if (startLine === undefined || endLine === undefined) {
      return this.reader.readAll(filePath);
    }
    return this.reader.read(filePath, startLine, endLine);
  }

  /**
   * Tail log (last N lines)
   */
  async tailLog(filePath: string, lineCount: number = 50): Promise<ReadResult> {
    return this.reader.tail(filePath, lineCount);
  }

  /**
   * Search logs with complex query
   */
  async searchLogs(query: LogQuery): Promise<QueryResult> {
    const startTime = Date.now();

    // Determine which files to search
    let filePaths: string[];
    if (query.files) {
      filePaths = query.files;
    } else {
      // Filter files by criteria
      const criteria: FilterCriteria = {
        category: query.category,
        component: query.component,
        personaId: query.personaId,
        logType: query.logType
      };
      const logs = await this.registry.filter(criteria);
      filePaths = logs.map(log => log.filePath);
    }

    // Search options
    const searchOptions: SearchOptions = {
      contextLines: query.contextLines,
      maxMatches: query.maxMatches,
      logLevel: query.logLevel,
      component: query.component,
      after: query.after,
      before: query.before
    };

    // Execute search
    const searchResult = await this.searcher.searchMultiple(
      filePaths,
      query.pattern,
      searchOptions
    );

    // Get log metadata
    const logs = await this.registry.discover();
    const matchingLogs = logs.filter(log => filePaths.includes(log.filePath));

    return {
      logs: matchingLogs,
      lines: searchResult.matches.map(m => m.line),
      matches: searchResult.matches,
      summary: {
        totalFiles: matchingLogs.length,
        totalLines: searchResult.matches.length,
        totalMatches: searchResult.totalMatches,
        durationMs: Date.now() - startTime
      }
    };
  }

  /**
   * Get summary statistics
   */
  async getStats(): Promise<LogStats> {
    const logs = await this.registry.discover();

    // Calculate totals
    const totalFiles = logs.length;
    const totalSizeBytes = logs.reduce((sum, log) => sum + log.sizeBytes, 0);
    const totalSizeMB = totalSizeBytes / (1024 * 1024);

    // Group by category
    const byCategory: Record<string, number> = {};
    for (const log of logs) {
      byCategory[log.category] = (byCategory[log.category] || 0) + 1;
    }

    // Group by component
    const byComponent: Record<string, number> = {};
    for (const log of logs) {
      if (log.component) {
        byComponent[log.component] = (byComponent[log.component] || 0) + 1;
      }
    }

    // Largest files (top 10)
    const largestFiles = logs
      .map(log => ({
        filePath: log.filePath,
        sizeMB: log.sizeBytes / (1024 * 1024)
      }))
      .sort((a, b) => b.sizeMB - a.sizeMB)
      .slice(0, 10);

    // Oldest files (top 10)
    const now = Date.now();
    const oldestFiles = logs
      .map(log => ({
        filePath: log.filePath,
        lastModified: log.lastModified,
        ageHours: (now - log.lastModified.getTime()) / (1000 * 60 * 60)
      }))
      .sort((a, b) => a.lastModified.getTime() - b.lastModified.getTime())
      .slice(0, 10);

    return {
      totalFiles,
      totalSizeMB,
      byCategory,
      byComponent,
      largestFiles,
      oldestFiles
    };
  }

  /**
   * Open iterator for large file processing
   */
  async openIterator(filePath: string): Promise<IteratorHandle> {
    return this.iterator.open(filePath);
  }

  /**
   * Read from iterator
   */
  async readIterator(handleId: string, lineCount: number): Promise<ReadResult> {
    return this.iterator.read(handleId, lineCount);
  }

  /**
   * Seek iterator to specific line
   */
  async seekIterator(handleId: string, lineNumber: number): Promise<void> {
    return this.iterator.seek(handleId, lineNumber);
  }

  /**
   * Close iterator
   */
  async closeIterator(handleId: string): Promise<void> {
    return this.iterator.close(handleId);
  }

  /**
   * List active iterators
   */
  listIterators(): IteratorHandle[] {
    return this.iterator.listHandles();
  }

  /**
   * Shutdown engine (cleanup resources)
   */
  async shutdown(): Promise<void> {
    await this.iterator.shutdown();
    this.reader.clearCache();
    this.registry.invalidateCache();
  }
}
