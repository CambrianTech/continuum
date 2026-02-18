/**
 * Import Path Resolution Utilities
 * 
 * Modular utilities for resolving alias imports to relative paths.
 * Handles complex nested alias patterns dynamically.
 */

import * as path from 'path';
import * as fs from 'fs';

export interface AliasMapping {
  [alias: string]: string;
}

export interface ResolvedImport {
  alias: string;
  relativePath: string;
  remainingPath: string;
}

export interface ImportStatement {
  fullMatch: string;
  importPath: string;
  quote: string;
}

/**
 * Extracts import statements from code content
 */
export class ImportExtractor {
  private static readonly IMPORT_REGEX = /from (['"])([^'"]+)\1/g;

  static extractImports(content: string): ImportStatement[] {
    const imports: ImportStatement[] = [];
    let match;
    
    // Reset regex state
    this.IMPORT_REGEX.lastIndex = 0;
    
    while ((match = this.IMPORT_REGEX.exec(content)) !== null) {
      imports.push({
        fullMatch: match[0],
        importPath: match[2],
        quote: match[1]
      });
    }
    
    return imports;
  }
}

/**
 * Resolves alias patterns to actual paths
 */
export class AliasResolver {
  constructor(private aliasMappings: AliasMapping) {}

  /**
   * Finds the best matching alias for an import path
   * Uses longest-match-first to handle nested patterns correctly
   */
  resolveAlias(importPath: string): ResolvedImport | null {
    let bestMatch: ResolvedImport | null = null;
    
    for (const [alias, relativePath] of Object.entries(this.aliasMappings)) {
      if (importPath === alias) {
        // Exact match - highest priority
        return { alias, relativePath, remainingPath: '' };
      } else if (importPath.startsWith(alias + '/')) {
        // Prefix match - choose longest alias
        const remainingPath = importPath.substring(alias.length + 1);
        if (!bestMatch || alias.length > bestMatch.alias.length) {
          bestMatch = { alias, relativePath, remainingPath };
        }
      }
    }
    
    return bestMatch;
  }
}

/**
 * Calculates relative paths from source to target
 */
export class RelativePathCalculator {
  constructor(private distPath: string) {}

  /**
   * Calculates relative import path from source file to target
   */
  calculateRelativePath(
    sourceFile: string, 
    targetRelativePath: string, 
    remainingPath: string = ''
  ): string {
    const sourceDir = path.dirname(sourceFile);
    
    let targetPath: string;
    if (remainingPath) {
      targetPath = path.join(this.distPath, targetRelativePath, remainingPath);
    } else {
      // Direct import - check for index.js or .js file
      const basePath = path.join(this.distPath, targetRelativePath);
      const indexPath = path.join(basePath, 'index.js');
      const directFilePath = basePath + '.js';
      targetPath = fs.existsSync(indexPath) ? indexPath : directFilePath;
    }
    
    const relativeImport = path.relative(sourceDir, targetPath).replace(/\\/g, '/');
    return relativeImport.startsWith('.') ? relativeImport : `./${relativeImport}`;
  }
}

/**
 * Main import path resolver combining all utilities
 */
export class ImportPathResolver {
  private aliasResolver: AliasResolver;
  private pathCalculator: RelativePathCalculator;

  constructor(aliasMappings: AliasMapping, distPath: string) {
    this.aliasResolver = new AliasResolver(aliasMappings);
    this.pathCalculator = new RelativePathCalculator(distPath);
  }

  /**
   * Resolves all alias imports in content to relative paths
   */
  resolveImportsInContent(content: string, sourceFilePath: string): {
    content: string;
    replacements: Array<{ from: string; to: string }>;
  } {
    const imports = ImportExtractor.extractImports(content);
    const replacements: Array<{ from: string; to: string }> = [];
    let modifiedContent = content;

    for (const importStatement of imports) {
      const resolved = this.aliasResolver.resolveAlias(importStatement.importPath);
      if (!resolved) continue;

      const relativePath = this.pathCalculator.calculateRelativePath(
        sourceFilePath,
        resolved.relativePath,
        resolved.remainingPath
      );

      const newImportStatement = importStatement.fullMatch.replace(
        importStatement.importPath,
        relativePath
      );

      modifiedContent = modifiedContent.replace(
        importStatement.fullMatch,
        newImportStatement
      );

      const displayPath = resolved.remainingPath 
        ? `${resolved.alias}/${resolved.remainingPath}` 
        : resolved.alias;
      
      replacements.push({ from: displayPath, to: relativePath });
    }

    return { content: modifiedContent, replacements };
  }
}