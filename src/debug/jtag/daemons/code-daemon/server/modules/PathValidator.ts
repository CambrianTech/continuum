/**
 * PathValidator - Security validation for file system operations
 *
 * Prevents directory traversal, validates paths within repository
 */

import * as path from 'path';
import * as fs from 'fs';

export interface PathValidationResult {
  valid: boolean;
  absolutePath?: string;
  error?: string;
}

export class PathValidator {
  private repositoryRoot: string;

  constructor(repositoryRoot: string) {
    this.repositoryRoot = path.resolve(repositoryRoot);
  }

  /**
   * Validate a file path is safe and within repository
   */
  validate(filePath: string): PathValidationResult {
    try {
      // Resolve to absolute path
      const absolutePath = path.resolve(this.repositoryRoot, filePath);

      // Check if path is within repository (prevent directory traversal)
      if (!absolutePath.startsWith(this.repositoryRoot)) {
        return {
          valid: false,
          error: `Path outside repository: ${filePath}`
        };
      }

      // Check if path exists
      if (!fs.existsSync(absolutePath)) {
        return {
          valid: false,
          error: `Path does not exist: ${filePath}`
        };
      }

      // Check if it's a file (not directory)
      const stats = fs.statSync(absolutePath);
      if (!stats.isFile()) {
        return {
          valid: false,
          error: `Path is not a file: ${filePath}`
        };
      }

      return {
        valid: true,
        absolutePath
      };
    } catch (error) {
      return {
        valid: false,
        error: `Path validation failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Validate a directory path
   */
  validateDirectory(dirPath: string): PathValidationResult {
    try {
      const absolutePath = path.resolve(this.repositoryRoot, dirPath);

      if (!absolutePath.startsWith(this.repositoryRoot)) {
        return {
          valid: false,
          error: `Path outside repository: ${dirPath}`
        };
      }

      if (!fs.existsSync(absolutePath)) {
        return {
          valid: false,
          error: `Directory does not exist: ${dirPath}`
        };
      }

      const stats = fs.statSync(absolutePath);
      if (!stats.isDirectory()) {
        return {
          valid: false,
          error: `Path is not a directory: ${dirPath}`
        };
      }

      return {
        valid: true,
        absolutePath
      };
    } catch (error) {
      return {
        valid: false,
        error: `Directory validation failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Get repository root
   */
  getRepositoryRoot(): string {
    return this.repositoryRoot;
  }
}
