/**
 * FileScanner - File pattern matching and glob expansion
 *
 * Extracted from generate-structure.ts for Phase 2 refactoring.
 * Handles glob patterns like "commands/**/shared/*Types.ts"
 */

import { existsSync, readdirSync, statSync } from 'fs';
import { join, relative } from 'path';

export class FileScanner {
  private rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Find files matching patterns, excluding those matching exclude patterns
   */
  findFiles(patterns: string[], excludePatterns: string[] = []): string[] {
    return patterns
      .flatMap(pattern => this.expandPattern(pattern.split('/'), this.rootPath))
      .filter(path => existsSync(path) && !this.isExcluded(path, excludePatterns))
      .sort();
  }

  /**
   * Check if a file path matches any exclude pattern
   */
  private isExcluded(filePath: string, excludePatterns: string[]): boolean {
    const relativePath = relative(this.rootPath, filePath).replace(/\\/g, '/');
    return excludePatterns.some(pattern => this.matchesGlobPattern(relativePath, pattern));
  }

  /**
   * Efficient glob pattern matching
   * Converts glob syntax to regex: * matches within segment, ** matches any segments
   */
  private matchesGlobPattern(filePath: string, pattern: string): boolean {
    // Convert glob pattern to regex - be more careful about order
    const regexPattern = pattern
      .replace(/\./g, '\\.')            // Escape dots first
      .replace(/\*\*/g, '§DOUBLESTAR§')  // Temporarily replace ** to avoid conflicts
      .replace(/\*/g, '[^/]*')          // * matches within a segment
      .replace(/§DOUBLESTAR§/g, '.*');  // ** matches any path segments

    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(filePath);
  }

  /**
   * Recursively expand glob pattern parts into concrete file paths
   */
  private expandPattern(patternParts: string[], basePath: string, index = 0): string[] {
    if (index >= patternParts.length) return [basePath];

    const part = patternParts[index];

    const safeReadDir = (path: string): string[] => {
      try { return readdirSync(path); } catch { return []; }
    };

    const safeIsDirectory = (path: string): boolean => {
      try { return statSync(path).isDirectory(); } catch { return false; }
    };

    const safeIsFile = (path: string): boolean => {
      try { return statSync(path).isFile(); } catch { return false; }
    };

    // Handle different pattern types
    if (part === '**') {
      return [
        // Zero directories matched - continue from current directory
        ...this.expandPattern(patternParts, basePath, index + 1),
        // Recursively descend into subdirectories
        ...safeReadDir(basePath)
          .map(entry => join(basePath, entry))
          .filter(safeIsDirectory)
          .flatMap(fullPath => this.expandPattern(patternParts, fullPath, index))
      ];
    }

    if (part === '*') {
      return safeReadDir(basePath)
        .map(entry => join(basePath, entry))
        .filter(safeIsDirectory)
        .flatMap(fullPath => this.expandPattern(patternParts, fullPath, index + 1));
    }

    if (part.includes('*')) {
      return safeReadDir(basePath)
        .filter(entry => this.matchesPattern(entry, part))
        .map(entry => join(basePath, entry))
        .filter(safeIsFile);
    }

    // Literal path component
    return this.expandPattern(patternParts, join(basePath, part), index + 1);
  }

  /**
   * Simple pattern matching for filename wildcards
   */
  private matchesPattern(filename: string, pattern: string): boolean {
    // Simple pattern matching - convert * to regex
    const regexPattern = pattern.replace(/\*/g, '.*');
    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(filename);
  }
}
