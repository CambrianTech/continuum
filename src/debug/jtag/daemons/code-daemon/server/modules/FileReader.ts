/**
 * FileReader - Read files with caching and validation
 */

import * as fs from 'fs';
import * as path from 'path';
import { PathValidator } from './PathValidator';
import type { CodeReadOptions, CodeReadResult } from '../../shared/CodeDaemonTypes';

interface CacheEntry {
  content: string;
  metadata: CodeReadResult['metadata'];
  timestamp: number;
}

export class FileReader {
  private pathValidator: PathValidator;
  private cache: Map<string, CacheEntry> = new Map();
  private maxFileSize: number;
  private enableCache: boolean;
  private cacheTTL: number;

  constructor(
    pathValidator: PathValidator,
    maxFileSize: number = 10 * 1024 * 1024, // 10MB default
    enableCache: boolean = true,
    cacheTTL: number = 60000 // 1 minute default
  ) {
    this.pathValidator = pathValidator;
    this.maxFileSize = maxFileSize;
    this.enableCache = enableCache;
    this.cacheTTL = cacheTTL;
  }

  /**
   * Read a file with optional line range
   */
  async read(filePath: string, options: CodeReadOptions = {}): Promise<CodeReadResult> {
    // Validate path
    const validation = this.pathValidator.validate(filePath);
    if (!validation.valid || !validation.absolutePath) {
      return {
        success: false,
        metadata: {
          path: filePath,
          size: 0,
          lines: 0,
          linesReturned: 0,
          modified: ''
        },
        error: validation.error
      };
    }

    const absolutePath = validation.absolutePath;

    try {
      // Check cache if enabled and not force refresh
      if (this.enableCache && !options.forceRefresh) {
        const cached = this.getCachedFile(absolutePath);
        if (cached) {
          return this.extractLines(cached.content, cached.metadata, options, true);
        }
      }

      // Check file size
      const stats = fs.statSync(absolutePath);
      if (stats.size > this.maxFileSize) {
        return {
          success: false,
          metadata: {
            path: absolutePath,
            size: stats.size,
            lines: 0,
            linesReturned: 0,
            modified: stats.mtime.toISOString()
          },
          error: `File too large: ${stats.size} bytes (max: ${this.maxFileSize})`
        };
      }

      // Read file
      const content = fs.readFileSync(absolutePath, 'utf-8');
      const lines = content.split('\n');

      const metadata: CodeReadResult['metadata'] = {
        path: absolutePath,
        size: stats.size,
        lines: lines.length,
        linesReturned: lines.length,
        modified: stats.mtime.toISOString()
      };

      // Cache if enabled
      if (this.enableCache) {
        this.cacheFile(absolutePath, content, metadata);
      }

      return this.extractLines(content, metadata, options, false);
    } catch (error) {
      return {
        success: false,
        metadata: {
          path: absolutePath,
          size: 0,
          lines: 0,
          linesReturned: 0,
          modified: ''
        },
        error: `Failed to read file: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Extract specific line range from content
   */
  private extractLines(
    content: string,
    metadata: CodeReadResult['metadata'],
    options: CodeReadOptions,
    cached: boolean
  ): CodeReadResult {
    const lines = content.split('\n');

    // If no line range specified, return full content
    if (options.startLine === undefined && options.endLine === undefined) {
      return {
        success: true,
        content,
        metadata,
        cached
      };
    }

    // Extract line range (1-indexed)
    const startLine = Math.max(1, options.startLine || 1);
    const endLine = Math.min(lines.length, options.endLine || lines.length);

    if (startLine > endLine) {
      return {
        success: false,
        metadata,
        error: `Invalid line range: ${startLine}-${endLine}`
      };
    }

    const selectedLines = lines.slice(startLine - 1, endLine);
    const extractedContent = selectedLines.join('\n');

    return {
      success: true,
      content: extractedContent,
      metadata: {
        ...metadata,
        linesReturned: selectedLines.length
      },
      cached
    };
  }

  /**
   * Get cached file if valid
   */
  private getCachedFile(absolutePath: string): CacheEntry | null {
    const cached = this.cache.get(absolutePath);
    if (!cached) return null;

    // Check if cache expired
    const now = Date.now();
    if (now - cached.timestamp > this.cacheTTL) {
      this.cache.delete(absolutePath);
      return null;
    }

    return cached;
  }

  /**
   * Cache file content
   */
  private cacheFile(absolutePath: string, content: string, metadata: CodeReadResult['metadata']): void {
    this.cache.set(absolutePath, {
      content,
      metadata,
      timestamp: Date.now()
    });
  }

  /**
   * Clear cache
   */
  clearCache(): void {
    this.cache.clear();
  }

  /**
   * Get cache stats
   */
  getCacheStats(): { entries: number; size: number } {
    let totalSize = 0;
    for (const entry of this.cache.values()) {
      totalSize += entry.content.length;
    }
    return {
      entries: this.cache.size,
      size: totalSize
    };
  }
}
