/**
 * LogReader - Read specific line ranges from log files
 *
 * Purpose:
 * - Read lines N-M from any log file
 * - Stream large files (don't load entire file into memory)
 * - Parse log lines into structured format
 * - Provide tail functionality (last N lines)
 *
 * Usage:
 *   const reader = new LogReader();
 *   const result = await reader.read('/path/to/file.log', 1, 100);
 *   const tailResult = await reader.tail('/path/to/file.log', 50);
 *   const lineCount = await reader.countLines('/path/to/file.log');
 */

import * as fs from 'fs';
import * as readline from 'readline';

export interface ReadResult {
  lines: LogLine[];
  totalLines: number;
  hasMore: boolean;
  nextOffset: number;
}

export interface LogLine {
  lineNumber: number;
  content: string;
  timestamp?: Date;    // Parsed from log line if available
  level?: string;      // e.g., 'INFO', 'ERROR'
  component?: string;  // e.g., 'AIProviderDaemon'
}

export class LogReader {
  private lineCountCache: Map<string, { count: number; timestamp: number }> = new Map();
  private readonly LINE_COUNT_CACHE_TTL_MS = 60000;  // 60 seconds

  /**
   * Read specific line range
   * @param filePath - Path to log file
   * @param startLine - 1-indexed line number
   * @param endLine - 1-indexed line number (inclusive)
   */
  async read(filePath: string, startLine: number, endLine: number): Promise<ReadResult> {
    // Validate input
    if (startLine < 1) {
      throw new Error(`startLine must be >= 1, got ${startLine}`);
    }
    if (endLine < startLine) {
      throw new Error(`endLine must be >= startLine, got ${startLine}-${endLine}`);
    }

    const lines: LogLine[] = [];
    let currentLine = 0;

    // Stream file line-by-line
    const stream = fs.createReadStream(filePath, { encoding: 'utf-8' });
    const rl = readline.createInterface({ input: stream, crlfDelay: Infinity });

    for await (const line of rl) {
      currentLine++;

      // Skip lines before startLine
      if (currentLine < startLine) continue;

      // Stop after endLine
      if (currentLine > endLine) break;

      // Parse and collect line
      lines.push(this.parseLine(line, currentLine));
    }

    // Get total line count
    const totalLines = await this.countLines(filePath);

    return {
      lines,
      totalLines,
      hasMore: endLine < totalLines,
      nextOffset: endLine + 1
    };
  }

  /**
   * Read last N lines (tail)
   * Optimized: reads file backwards from end
   */
  async tail(filePath: string, lineCount: number): Promise<ReadResult> {
    const totalLines = await this.countLines(filePath);
    const startLine = Math.max(1, totalLines - lineCount + 1);
    return this.read(filePath, startLine, totalLines);
  }

  /**
   * Count total lines in file (async, uses streaming)
   * Caches result for 60 seconds
   */
  async countLines(filePath: string): Promise<number> {
    // Check cache
    const cached = this.lineCountCache.get(filePath);
    if (cached && Date.now() - cached.timestamp < this.LINE_COUNT_CACHE_TTL_MS) {
      return cached.count;
    }

    // Count lines by streaming
    let count = 0;
    const stream = fs.createReadStream(filePath, { encoding: 'utf-8' });
    const rl = readline.createInterface({ input: stream, crlfDelay: Infinity });

    for await (const _ of rl) {
      count++;
    }

    // Update cache
    this.lineCountCache.set(filePath, { count, timestamp: Date.now() });

    return count;
  }

  /**
   * Parse log line into structured format
   * Extracts timestamp, level, and component from common log formats
   */
  parseLine(line: string, lineNumber: number): LogLine {
    const logLine: LogLine = {
      lineNumber,
      content: line
    };

    // Parse timestamp (ISO 8601 format: [2025-12-02T19:33:09.123Z])
    const timestampMatch = line.match(/^\[(\d{4}-\d{2}-\d{2}T[\d:.]+Z)\]/);
    if (timestampMatch) {
      try {
        logLine.timestamp = new Date(timestampMatch[1]);
      } catch (error) {
        // Invalid timestamp format
      }
    }

    // Parse log level ([DEBUG], [INFO], [WARN], [ERROR])
    const levelMatch = line.match(/\[(DEBUG|INFO|WARN|ERROR)\]/);
    if (levelMatch) {
      logLine.level = levelMatch[1];
    }

    // Parse component (after level: "ComponentName:")
    // Example: [2025-12-02T19:33:09.123Z] [INFO] AIProviderDaemon: Starting...
    const componentMatch = line.match(/\[.*?\]\s*\[.*?\]\s*(\w+):/);
    if (componentMatch) {
      logLine.component = componentMatch[1];
    }

    return logLine;
  }

  /**
   * Clear line count cache (for testing or manual invalidation)
   */
  clearCache(): void {
    this.lineCountCache.clear();
  }

  /**
   * Read entire file (use with caution on large files)
   */
  async readAll(filePath: string): Promise<ReadResult> {
    const totalLines = await this.countLines(filePath);
    return this.read(filePath, 1, totalLines);
  }
}
