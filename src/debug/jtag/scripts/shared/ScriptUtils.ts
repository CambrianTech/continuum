/**
 * Shared Utilities for Script Architecture
 * 
 * Common utility functions used across all configuration and automation scripts.
 * Promotes DRY principles and consistent behavior across the script ecosystem.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { 
  FileDiscoveryOptions, 
  DirectoryTraversalResult,
  ProcessingStats,
  LogLevel,
  LogEntry,
  ScriptError,
  UnifiedConfig,
  PathMapping
} from '@scripts/shared/ScriptTypes';

// ============================================================================
// File System Utilities
// ============================================================================

export class FileSystemUtils {
  static async exists(filePath: string): Promise<boolean> {
    try {
      await fs.promises.access(filePath);
      return true;
    } catch {
      return false;
    }
  }

  static async readJsonFile<T>(filePath: string): Promise<T> {
    try {
      const content = await fs.promises.readFile(filePath, 'utf-8');
      return JSON.parse(content);
    } catch (error) {
      throw new Error(`Failed to read JSON file ${filePath}: ${error}`);
    }
  }

  static async writeJsonFile(filePath: string, data: any, pretty: boolean = true): Promise<void> {
    try {
      const content = pretty ? JSON.stringify(data, null, 2) : JSON.stringify(data);
      await fs.promises.writeFile(filePath, content, 'utf-8');
    } catch (error) {
      throw new Error(`Failed to write JSON file ${filePath}: ${error}`);
    }
  }

  static async discoverFiles(
    rootPath: string, 
    options: Partial<FileDiscoveryOptions> = {}
  ): Promise<DirectoryTraversalResult> {
    const opts: FileDiscoveryOptions = {
      extensions: ['.ts', '.js'],
      excludedDirs: ['node_modules', 'dist', '.git'],
      excludedPatterns: ['.bak', '.tmp'],
      includeHidden: false,
      ...options
    };

    const result: DirectoryTraversalResult = {
      files: [],
      directories: [],
      errors: []
    };

    await this.traverseDirectory(rootPath, '', opts, result);
    return result;
  }

  private static async traverseDirectory(
    rootPath: string,
    currentPath: string,
    options: FileDiscoveryOptions,
    result: DirectoryTraversalResult
  ): Promise<void> {
    try {
      const fullPath = path.resolve(rootPath, currentPath);
      const items = await fs.promises.readdir(fullPath, { withFileTypes: true });

      for (const item of items) {
        const itemPath = currentPath ? `${currentPath}/${item.name}` : item.name;

        if (item.isDirectory()) {
          if (this.shouldProcessDirectory(item.name, options)) {
            result.directories.push(itemPath);
            await this.traverseDirectory(rootPath, itemPath, options, result);
          }
        } else if (this.shouldIncludeFile(item.name, options)) {
          result.files.push(itemPath);
        }
      }
    } catch (error) {
      result.errors.push(`Failed to read directory ${currentPath}: ${error}`);
    }
  }

  private static shouldProcessDirectory(dirName: string, options: FileDiscoveryOptions): boolean {
    if (!options.includeHidden && dirName.startsWith('.')) return false;
    if (options.excludedDirs.includes(dirName)) return false;
    if (options.excludedPatterns.some(pattern => dirName.includes(pattern))) return false;
    return true;
  }

  private static shouldIncludeFile(fileName: string, options: FileDiscoveryOptions): boolean {
    if (!options.includeHidden && fileName.startsWith('.')) return false;
    if (options.excludedPatterns.some(pattern => fileName.includes(pattern))) return false;
    if (fileName.endsWith('.d.ts')) return false; // Exclude type definition files
    return options.extensions.some(ext => fileName.endsWith(ext));
  }
}

// ============================================================================
// Path Resolution Utilities
// ============================================================================

export class PathUtils {
  static normalizePath(inputPath: string): string {
    return inputPath.replace(/\\/g, '/');
  }

  static getRelativeFromRoot(rootPath: string, targetPath: string): string {
    const relative = path.relative(rootPath, targetPath);
    return this.normalizePath(relative);
  }

  static resolveFromCurrent(currentFilePath: string, relativePath: string): string {
    const currentDir = path.dirname(currentFilePath);
    const resolved = path.resolve(currentDir, relativePath);
    return this.normalizePath(resolved);
  }

  static generateAliasFromPath(inputPath: string): string {
    const parts = inputPath.split('/');
    const relevantParts = parts.length === 1 ? [parts[0]] : parts.slice(-2);
    
    const camelCase = relevantParts.map((part, index) => {
      if (index === 0) return part;
      return part.charAt(0).toUpperCase() + part.slice(1).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    }).join('');
    
    return `@${camelCase}`;
  }

  static findBestAliasMatch(targetPath: string, pathMappings: { [key: string]: PathMapping }): string | null {
    // Exact match first
    const exactMatch = Object.entries(pathMappings).find(([_, mapping]) => mapping.relativePath === targetPath);
    if (exactMatch) return exactMatch[0];

    // Find longest matching prefix
    let bestMatch: { alias: string; pathLength: number } | null = null;

    for (const [alias, mapping] of Object.entries(pathMappings)) {
      if (targetPath.startsWith(mapping.relativePath)) {
        if (!bestMatch || mapping.relativePath.length > bestMatch.pathLength) {
          bestMatch = { alias, pathLength: mapping.relativePath.length };
        }
      }
    }

    if (bestMatch) {
      const mappingPath = pathMappings[bestMatch.alias].relativePath;
      if (targetPath.length > mappingPath.length) {
        const remainingPath = targetPath.substring(mappingPath.length + 1);
        return `${bestMatch.alias}/${remainingPath}`;
      }
      return bestMatch.alias;
    }

    return null;
  }
}

// ============================================================================
// Processing Statistics Utilities
// ============================================================================

export class StatsUtils {
  static createEmptyStats(): ProcessingStats {
    return {
      filesProcessed: 0,
      filesModified: 0,
      totalReplacements: 0,
      startTime: new Date()
    };
  }

  static finalizeStats(stats: ProcessingStats): ProcessingStats {
    return {
      ...stats,
      endTime: new Date(),
      duration: stats.startTime ? Date.now() - stats.startTime.getTime() : 0
    };
  }

  static formatDuration(milliseconds: number): string {
    if (milliseconds < 1000) return `${milliseconds}ms`;
    if (milliseconds < 60000) return `${(milliseconds / 1000).toFixed(1)}s`;
    return `${Math.floor(milliseconds / 60000)}m ${Math.floor((milliseconds % 60000) / 1000)}s`;
  }

  static calculatePercentage(completed: number, total: number): number {
    return total === 0 ? 0 : Math.round((completed / total) * 100);
  }
}

// ============================================================================
// Logging Utilities
// ============================================================================

export class Logger {
  private static logs: LogEntry[] = [];

  static log(level: LogLevel, message: string, context?: string, metadata?: any): void {
    const entry: LogEntry = {
      level,
      message,
      timestamp: new Date(),
      context,
      metadata
    };

    this.logs.push(entry);
    this.printLog(entry);
  }

  static debug(message: string, context?: string, metadata?: any): void {
    this.log('debug', message, context, metadata);
  }

  static info(message: string, context?: string, metadata?: any): void {
    this.log('info', message, context, metadata);
  }

  static warn(message: string, context?: string, metadata?: any): void {
    this.log('warn', message, context, metadata);
  }

  static error(message: string, context?: string, metadata?: any): void {
    this.log('error', message, context, metadata);
  }

  static success(message: string, context?: string, metadata?: any): void {
    this.log('success', message, context, metadata);
  }

  private static printLog(entry: LogEntry): void {
    const icons = {
      debug: '🔍',
      info: 'ℹ️',
      warn: '⚠️',
      error: '❌',
      success: '✅'
    };

    const icon = icons[entry.level];
    const contextStr = entry.context ? ` [${entry.context}]` : '';
    console.log(`${icon} ${entry.message}${contextStr}`);
  }

  static getLogs(): LogEntry[] {
    return [...this.logs];
  }

  static clearLogs(): void {
    this.logs = [];
  }
}

// ============================================================================
// Error Handling Utilities
// ============================================================================

export class ErrorUtils {
  static createScriptError(
    type: ScriptError['type'],
    message: string,
    file?: string,
    line?: number,
    details?: any
  ): ScriptError {
    return {
      type,
      message,
      file,
      line,
      details,
      timestamp: new Date()
    };
  }

  static formatError(error: ScriptError): string {
    let formatted = `[${error.type.toUpperCase()}] ${error.message}`;
    if (error.file) formatted += ` (${error.file}`;
    if (error.line) formatted += `:${error.line}`;
    if (error.file) formatted += ')';
    return formatted;
  }

  static isScriptError(obj: any): obj is ScriptError {
    return obj && 
           typeof obj.type === 'string' &&
           typeof obj.message === 'string' &&
           obj.timestamp instanceof Date;
  }
}

// ============================================================================
// Configuration Loading Utilities
// ============================================================================

export class ConfigLoader {
  static async loadUnifiedConfig(rootPath: string): Promise<UnifiedConfig> {
    const configPath = path.join(rootPath, 'unified-config.json');
    return await FileSystemUtils.readJsonFile<UnifiedConfig>(configPath);
  }

  static async saveUnifiedConfig(rootPath: string, config: UnifiedConfig): Promise<void> {
    const configPath = path.join(rootPath, 'unified-config.json');
    await FileSystemUtils.writeJsonFile(configPath, config);
  }

  static validateConfig(config: any): config is UnifiedConfig {
    return config &&
           typeof config.projectName === 'string' &&
           typeof config.version === 'string' &&
           config.pathMappings &&
           typeof config.pathMappings === 'object';
  }
}