/**
 * Path Utilities
 * 
 * Shared path manipulation utilities for the generator system.
 * Handles alias generation, path resolution, and conflict detection.
 */

import { relative, join, dirname } from 'path';
import type { PathMapping, AliasStrategy } from '../types/GeneratorTypes';

// ============================================================================
// Semantic Alias Strategy (Clean, Human-Readable)
// ============================================================================

export class SemanticAliasStrategy implements AliasStrategy {
  name = 'semantic';
  description = 'Generates clean, semantic aliases like @core, @shared, @daemons';

  private static readonly SEMANTIC_MAPPINGS: Record<string, string> = {
    'system/core': '@core',
    'system/core/types': '@types', 
    'system/core/client': '@client',
    'system/core/router': '@router',
    'system/core/system': '@system',
    'system/transports': '@transports',
    'system/events': '@events',
    'daemons': '@daemons',
    'commands': '@commands',
    'shared': '@shared',
    'scripts': '@scripts',
    'tests': '@tests',
    'widgets': '@widgets',
    'templates': '@templates'
  };

  generateAlias(relativePath: string): string {
    // Check for exact semantic matches first
    if (SemanticAliasStrategy.SEMANTIC_MAPPINGS[relativePath]) {
      return SemanticAliasStrategy.SEMANTIC_MAPPINGS[relativePath];
    }

    // Check for semantic prefix matches
    for (const [path, alias] of Object.entries(SemanticAliasStrategy.SEMANTIC_MAPPINGS)) {
      if (relativePath.startsWith(path + '/')) {
        const suffix = relativePath.slice(path.length + 1);
        return `${alias}/${suffix}`;
      }
    }

    // Not a semantic path - return null to indicate no alias needed
    return null;
  }
}

// ============================================================================
// Path Resolution Utilities
// ============================================================================

export class PathResolver {
  private rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Calculate relative import path from output file to target file
   */
  calculateImportPath(outputFile: string, targetFile: string): string {
    const outputDir = dirname(join(this.rootPath, outputFile));
    const targetPath = join(this.rootPath, targetFile);
    const relativePath = relative(outputDir, targetPath)
      .replace('.ts', '')
      .replace(/\\/g, '/');
    
    return relativePath.startsWith('.') ? relativePath : `./${relativePath}`;
  }

  /**
   * Resolve absolute path from root
   */
  resolve(relativePath: string): string {
    return join(this.rootPath, relativePath);
  }

  /**
   * Get relative path from root
   */
  relativeTo(absolutePath: string): string {
    return relative(this.rootPath, absolutePath);
  }
}

// ============================================================================
// Path Mapping Generation
// ============================================================================

export class PathMappingGenerator {
  private strategy: AliasStrategy;
  private resolver: PathResolver;

  constructor(strategy: AliasStrategy, resolver: PathResolver) {
    this.strategy = strategy;
    this.resolver = resolver;
  }

  /**
   * Generate essential path mappings for TypeScript/linter usage
   */
  generateEssentialMappings(directories: string[]): Record<string, PathMapping> {
    const mappings: Record<string, PathMapping> = {};
    const conflicts: Set<string> = new Set();

    // First pass: generate aliases and detect conflicts
    const aliasToPath: Record<string, string[]> = {};
    
    for (const dir of directories) {
      const alias = this.strategy.generateAlias(dir);
      if (alias) {
        if (!aliasToPath[alias]) {
          aliasToPath[alias] = [];
        }
        aliasToPath[alias].push(dir);
      }
    }

    // Second pass: create mappings, skip conflicts
    for (const [alias, paths] of Object.entries(aliasToPath)) {
      if (paths.length === 1) {
        mappings[alias] = {
          alias,
          relativePath: paths[0],
          description: `Essential alias for ${paths[0]}`,
          essential: true
        };
      } else {
        conflicts.add(alias);
        console.warn(`⚠️  Alias conflict for "${alias}": ${paths.join(', ')} - skipping`);
      }
    }

    return mappings;
  }

  /**
   * Create TypeScript paths object from mappings
   */
  createTypeScriptPaths(mappings: Record<string, PathMapping>): Record<string, string[]> {
    const paths: Record<string, string[]> = {};
    
    for (const mapping of Object.values(mappings)) {
      paths[mapping.alias] = [mapping.relativePath];
      paths[`${mapping.alias}/*`] = [`${mapping.relativePath}/*`];
    }

    return paths;
  }

  /**
   * Validate that all essential directories have clean aliases
   */
  validateMappings(mappings: Record<string, PathMapping>, requiredPaths: string[]): {
    valid: boolean;
    missing: string[];
    conflicts: string[];
  } {
    const covered = new Set(Object.values(mappings).map(m => m.relativePath));
    const missing = requiredPaths.filter(path => !covered.has(path));
    
    return {
      valid: missing.length === 0,
      missing,
      conflicts: [] // Already handled during generation
    };
  }
}

// ============================================================================
// Directory Discovery Utilities  
// ============================================================================

export class DirectoryScanner {
  private resolver: PathResolver;

  constructor(resolver: PathResolver) {
    this.resolver = resolver;
  }

  /**
   * Find essential directories that should have aliases
   */
  findEssentialDirectories(): string[] {
    const essential = [
      'system/core',
      'system/core/types',
      'system/core/client', 
      'system/core/router',
      'system/core/system',
      'system/transports',
      'system/events',
      'daemons',
      'commands',
      'shared',
      'scripts',
      'tests',
      'widgets',
      'templates'
    ];

    // Filter to only directories that actually exist
    return essential.filter(dir => {
      try {
        const stats = require('fs').statSync(this.resolver.resolve(dir));
        return stats.isDirectory();
      } catch {
        return false;
      }
    });
  }

  /**
   * Check if directory should be excluded from processing
   */
  isExcluded(dirPath: string, excludePatterns: string[]): boolean {
    for (const pattern of excludePatterns) {
      if (pattern === '**/*.bak' && dirPath.includes('.bak')) return true;
      if (pattern === '**/*.bak/**/*' && dirPath.includes('.bak/')) return true;
      if (pattern === '**/node_modules/**/*' && dirPath.includes('node_modules/')) return true;
      if (pattern === '**/dist/**/*' && dirPath.includes('dist/')) return true;
    }
    return false;
  }
}