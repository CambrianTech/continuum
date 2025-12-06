/**
 * EntryExtractor - Extract class and entry information from TypeScript files
 *
 * Extracted from generate-structure.ts for Phase 2 refactoring.
 * Parses TypeScript files to extract class names, imports, and entry names.
 */

import { readFileSync } from 'fs';
import { join, relative } from 'path';

/**
 * Information extracted from a TypeScript file containing an exported class
 */
export interface EntryInfo {
  name: string;        // Entry name (e.g., "ping", "data/list")
  className: string;   // Class name (e.g., "PingCommand", "DataListCommand")
  importPath: string;  // Relative import path (e.g., "./commands/ping/shared/PingCommand")
}

/**
 * Strategy function to extract entry name from file path and class name
 */
export type NameExtractor = (filePath: string, className: string) => string;

export class EntryExtractor {
  private rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Extract entry information from a TypeScript file
   *
   * @param filePath - Absolute path to the TypeScript file
   * @param outputPath - Path where generated registry file will be written (for relative imports)
   * @param nameExtractor - Strategy function to extract entry name
   * @param type - Type description for logging (e.g., "command", "widget")
   * @returns EntryInfo if successful, null if file doesn't export expected class
   */
  extractEntryInfo(
    filePath: string,
    outputPath: string,
    nameExtractor: NameExtractor,
    type: string = 'entry'
  ): EntryInfo | null {
    try {
      const content = readFileSync(filePath, 'utf8');
      const filename = filePath.split('/').pop()!;
      const className = filename.replace('.ts', '');

      // Check if file exports the expected class
      const exportPattern = new RegExp(`export\\s+class\\s+${className}\\b`);
      if (!exportPattern.test(content)) {
        console.warn(`⚠️  Skipping ${filePath}: No export for class ${className}`);
        return null;
      }

      // Extract name using provided strategy
      const name = nameExtractor(filePath, className);

      // Calculate proper relative import path
      const outputDir = join(this.rootPath, outputPath).replace(/[^/]*$/, '');
      const relativeImportPath = relative(outputDir, filePath)
        .replace('.ts', '')
        .replace(/\\/g, '/');

      return {
        name,
        className,
        importPath: `./${relativeImportPath}`
      };
    } catch (e) {
      console.warn(`Ignoring ${type} info from ${filePath}:`, e);
      return null;
    }
  }

  /**
   * Extract multiple entries from a list of file paths
   *
   * @param filePaths - Array of absolute paths to TypeScript files
   * @param outputPath - Path where generated registry file will be written
   * @param nameExtractor - Strategy function to extract entry names
   * @param type - Type description for logging
   * @returns Array of EntryInfo objects (null entries are filtered out)
   */
  extractMultiple(
    filePaths: string[],
    outputPath: string,
    nameExtractor: NameExtractor,
    type: string = 'entry'
  ): EntryInfo[] {
    return filePaths
      .map(filePath => this.extractEntryInfo(filePath, outputPath, nameExtractor, type))
      .filter((entry): entry is EntryInfo => entry !== null);
  }
}
