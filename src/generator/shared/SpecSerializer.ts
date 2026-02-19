/**
 * Spec Serializer
 *
 * JSON serialization and deserialization for command specifications.
 * Handles file I/O and error handling.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { CommandSpec } from './specs/CommandSpec';
import { CommandSpecHelper } from './specs/CommandSpec';

/**
 * Serializer for command specifications
 */
export class SpecSerializer {
  /**
   * Convert spec to JSON string
   * @param spec The command spec to serialize
   * @param pretty Whether to use pretty printing (indentation)
   * @returns JSON string representation
   */
  static toJSON(spec: CommandSpec, pretty: boolean = false): string {
    return CommandSpecHelper.toJSON(spec, pretty);
  }

  /**
   * Parse JSON string to spec
   * @param json JSON string to parse
   * @returns Parsed command spec
   * @throws Error if JSON is invalid or missing required fields
   */
  static fromJSON(json: string): CommandSpec {
    try {
      return CommandSpecHelper.fromJSON(json);
    } catch (error) {
      throw new Error(`Failed to parse JSON spec: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Write spec to file
   * @param spec The command spec to write
   * @param filePath Path to write the file (absolute or relative)
   * @param pretty Whether to use pretty printing (default: true for files)
   */
  static toFile(spec: CommandSpec, filePath: string, pretty: boolean = true): void {
    try {
      // Ensure directory exists
      const dir = path.dirname(filePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }

      // Write JSON to file
      const json = this.toJSON(spec, pretty);
      fs.writeFileSync(filePath, json, 'utf-8');
    } catch (error) {
      throw new Error(
        `Failed to write spec to file '${filePath}': ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  /**
   * Read spec from file
   * @param filePath Path to read from (absolute or relative)
   * @returns Parsed command spec
   * @throws Error if file doesn't exist or JSON is invalid
   */
  static fromFile(filePath: string): CommandSpec {
    try {
      // Check if file exists
      if (!fs.existsSync(filePath)) {
        throw new Error(`File does not exist: ${filePath}`);
      }

      // Read and parse JSON
      const json = fs.readFileSync(filePath, 'utf-8');
      return this.fromJSON(json);
    } catch (error) {
      throw new Error(
        `Failed to read spec from file '${filePath}': ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  /**
   * Read spec from stdin (synchronous)
   * Used for piping JSON into commands
   * @returns Parsed command spec
   * @throws Error if stdin is empty or JSON is invalid
   */
  static fromStdin(): CommandSpec {
    try {
      // Read all stdin synchronously (for CLI usage)
      const stdin = fs.readFileSync(0, 'utf-8');

      if (!stdin || stdin.trim() === '') {
        throw new Error('No data received from stdin');
      }

      return this.fromJSON(stdin);
    } catch (error) {
      throw new Error(
        `Failed to read spec from stdin: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  /**
   * Write spec to stdout (for piping to other commands)
   * @param spec The command spec to write
   * @param pretty Whether to use pretty printing (default: false for stdout)
   */
  static toStdout(spec: CommandSpec, pretty: boolean = false): void {
    const json = this.toJSON(spec, pretty);
    console.log(json);
  }

  /**
   * Check if a file contains a valid spec (without throwing)
   * @param filePath Path to check
   * @returns Object with valid flag and error message
   */
  static validateFile(filePath: string): { valid: boolean; error?: string } {
    try {
      this.fromFile(filePath);
      return { valid: true };
    } catch (error) {
      return {
        valid: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Batch read multiple spec files from a directory
   * @param dirPath Directory containing spec files
   * @param pattern Glob pattern for matching files (default: "*.json")
   * @returns Array of specs with their file paths
   */
  static fromDirectory(
    dirPath: string,
    pattern: string = '*.json'
  ): Array<{ spec: CommandSpec; filePath: string; error?: string }> {
    const results: Array<{ spec: CommandSpec; filePath: string; error?: string }> = [];

    try {
      if (!fs.existsSync(dirPath)) {
        throw new Error(`Directory does not exist: ${dirPath}`);
      }

      const files = fs.readdirSync(dirPath);
      const jsonFiles = files.filter(file => {
        // Simple pattern matching (*.json)
        if (pattern === '*.json') {
          return file.endsWith('.json');
        }
        return file.match(pattern);
      });

      for (const file of jsonFiles) {
        const filePath = path.join(dirPath, file);
        try {
          const spec = this.fromFile(filePath);
          results.push({ spec, filePath });
        } catch (error) {
          results.push({
            spec: {} as CommandSpec, // Placeholder
            filePath,
            error: error instanceof Error ? error.message : String(error)
          });
        }
      }
    } catch (error) {
      throw new Error(
        `Failed to read specs from directory '${dirPath}': ${error instanceof Error ? error.message : String(error)}`
      );
    }

    return results;
  }

  /**
   * Batch write multiple specs to a directory
   * @param specs Array of specs to write
   * @param dirPath Directory to write files to
   * @param pretty Whether to use pretty printing
   */
  static toDirectory(
    specs: Array<{ spec: CommandSpec; fileName: string }>,
    dirPath: string,
    pretty: boolean = true
  ): void {
    try {
      // Ensure directory exists
      if (!fs.existsSync(dirPath)) {
        fs.mkdirSync(dirPath, { recursive: true });
      }

      // Write each spec
      for (const { spec, fileName } of specs) {
        const filePath = path.join(dirPath, fileName);
        this.toFile(spec, filePath, pretty);
      }
    } catch (error) {
      throw new Error(
        `Failed to write specs to directory '${dirPath}': ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }
}
