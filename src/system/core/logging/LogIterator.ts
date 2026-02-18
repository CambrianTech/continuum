/**
 * LogIterator - Handle-based streaming for large file processing
 *
 * Purpose:
 * - Maintain file position across multiple reads
 * - Provide "open, read, read, close" pattern
 * - Track active handles (prevent leaks)
 * - Auto-close on timeout
 *
 * Usage:
 *   const iterator = new LogIterator(logReader);
 *   const handle = await iterator.open('/path/to/large.log');
 *   const batch1 = await iterator.read(handle.id, 100);
 *   const batch2 = await iterator.read(handle.id, 100);
 *   await iterator.close(handle.id);
 */

import { v4 as uuidv4 } from 'uuid';
import { LogReader, type ReadResult } from './LogReader';

export interface IteratorHandle {
  id: string;              // UUID for handle
  filePath: string;
  position: number;        // Current line number (1-indexed)
  lineNumber: number;      // Alias for position
  createdAt: Date;
  lastAccessedAt: Date;
  expiresAt: Date;         // Auto-close after 5 minutes idle
  isEOF: boolean;
  totalLines: number;      // Cached from LogReader
}

export class LogIterator {
  private handles: Map<string, IteratorHandle> = new Map();
  private readonly MAX_CONCURRENT_HANDLES = 100;
  private readonly HANDLE_TIMEOUT_MS = 5 * 60 * 1000;  // 5 minutes
  private cleanupTimer: NodeJS.Timeout | null = null;

  constructor(private logReader: LogReader) {
    // Start cleanup timer (runs every 60 seconds)
    this.startCleanupTimer();
  }

  /**
   * Open file for iterative reading
   */
  async open(filePath: string): Promise<IteratorHandle> {
    // Check handle limit
    if (this.handles.size >= this.MAX_CONCURRENT_HANDLES) {
      throw new Error(
        `Maximum concurrent handles (${this.MAX_CONCURRENT_HANDLES}) reached. Close some handles first.`
      );
    }

    // Count total lines (cached by LogReader)
    const totalLines = await this.logReader.countLines(filePath);

    const now = new Date();
    const handle: IteratorHandle = {
      id: uuidv4(),
      filePath,
      position: 1,
      lineNumber: 1,
      createdAt: now,
      lastAccessedAt: now,
      expiresAt: new Date(now.getTime() + this.HANDLE_TIMEOUT_MS),
      isEOF: false,
      totalLines
    };

    this.handles.set(handle.id, handle);
    return handle;
  }

  /**
   * Read next N lines from handle
   */
  async read(handleId: string, lineCount: number): Promise<ReadResult> {
    const handle = this.handles.get(handleId);
    if (!handle) {
      throw new Error(`Invalid handle ID: ${handleId}`);
    }

    // Check if EOF
    if (handle.isEOF) {
      return {
        lines: [],
        totalLines: handle.totalLines,
        hasMore: false,
        nextOffset: handle.position
      };
    }

    // Read lines
    const startLine = handle.position;
    const endLine = Math.min(startLine + lineCount - 1, handle.totalLines);

    const result = await this.logReader.read(handle.filePath, startLine, endLine);

    // Update handle position
    handle.position = endLine + 1;
    handle.lineNumber = handle.position;
    handle.lastAccessedAt = new Date();
    handle.expiresAt = new Date(Date.now() + this.HANDLE_TIMEOUT_MS);

    // Check EOF
    if (handle.position > handle.totalLines) {
      handle.isEOF = true;
    }

    return result;
  }

  /**
   * Seek to specific line
   */
  async seek(handleId: string, lineNumber: number): Promise<void> {
    const handle = this.handles.get(handleId);
    if (!handle) {
      throw new Error(`Invalid handle ID: ${handleId}`);
    }

    if (lineNumber < 1 || lineNumber > handle.totalLines) {
      throw new Error(
        `Invalid line number ${lineNumber}. File has ${handle.totalLines} lines.`
      );
    }

    handle.position = lineNumber;
    handle.lineNumber = lineNumber;
    handle.isEOF = false;
    handle.lastAccessedAt = new Date();
    handle.expiresAt = new Date(Date.now() + this.HANDLE_TIMEOUT_MS);
  }

  /**
   * Close handle and free resources
   */
  async close(handleId: string): Promise<void> {
    this.handles.delete(handleId);
  }

  /**
   * Get handle metadata
   */
  getHandle(handleId: string): IteratorHandle | null {
    return this.handles.get(handleId) || null;
  }

  /**
   * List all active handles (for debugging)
   */
  listHandles(): IteratorHandle[] {
    return Array.from(this.handles.values());
  }

  /**
   * Close expired handles (called periodically)
   */
  async cleanupExpired(): Promise<number> {
    const now = new Date();
    let closedCount = 0;

    for (const [handleId, handle] of this.handles) {
      if (now > handle.expiresAt) {
        this.handles.delete(handleId);
        closedCount++;
      }
    }

    return closedCount;
  }

  /**
   * Start periodic cleanup timer
   */
  private startCleanupTimer(): void {
    if (this.cleanupTimer) return;

    this.cleanupTimer = setInterval(() => {
      this.cleanupExpired().catch(error => {
        console.error('LogIterator cleanup failed:', error);
      });
    }, 60000);  // Every 60 seconds
  }

  /**
   * Stop cleanup timer (for testing or shutdown)
   */
  stopCleanupTimer(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
  }

  /**
   * Shutdown iterator (close all handles)
   */
  async shutdown(): Promise<void> {
    this.stopCleanupTimer();
    this.handles.clear();
  }
}
