/**
 * File Management Utilities
 * 
 * Safe file operations with backup creation, atomic updates, and validation.
 * Handles JSON reading/writing with proper error handling.
 */

import { readFileSync, writeFileSync, existsSync, copyFileSync, mkdirSync, statSync, renameSync, readdirSync, unlinkSync } from 'fs';
import { dirname, basename, join } from 'path';
import type { FileUpdate, GeneratorLogger, TypeScriptConfig, PathMappingsConfig, StructureConfig, PathMapping, StructureTarget } from '../types/GeneratorTypes';

// ============================================================================
// Safe File Operations
// ============================================================================

export class FileManager {
  private logger: GeneratorLogger;
  private dryRun: boolean;
  private maxBackups: number;

  constructor(logger: GeneratorLogger, dryRun = false, maxBackups = 3) {
    this.logger = logger;
    this.dryRun = dryRun;
    this.maxBackups = maxBackups;
  }

  /**
   * Read JSON file with error handling and type safety
   */
  readJSON<T>(filePath: string): T | null {
    try {
      if (!existsSync(filePath)) {
        this.logger.debug(`File does not exist: ${filePath}`);
        return null;
      }

      const content = readFileSync(filePath, 'utf8');
      return JSON.parse(content) as T;
    } catch (error) {
      this.logger.error(`Failed to read JSON from ${filePath}:`, error);
      return null;
    }
  }

  /**
   * Write JSON file with backup and atomic operation
   */
  writeJSON<T>(filePath: string, data: T, reason: string): FileUpdate {
    const backupPath = this.createBackup(filePath);
    const content = JSON.stringify(data, null, 2) + '\n';

    if (this.dryRun) {
      this.logger.info(`[DRY RUN] Would write ${filePath}: ${reason}`);
      return {
        filePath,
        backupPath,
        content,
        reason
      };
    }

    try {
      // Ensure directory exists
      const dir = dirname(filePath);
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }

      // Atomic write
      const tempPath = `${filePath}.tmp`;
      writeFileSync(tempPath, content, 'utf8');
      
      // Rename temp file to final location (atomic on most filesystems)
      renameSync(tempPath, filePath);
      
      this.logger.info(`✅ Updated ${filePath}: ${reason}`);
      
      return {
        filePath,
        backupPath,
        content,
        reason
      };
    } catch (error) {
      this.logger.error(`Failed to write ${filePath}:`, error);
      throw error;
    }
  }

  /**
   * Create backup of existing file with automatic cleanup
   */
  private createBackup(filePath: string): string | undefined {
    if (!existsSync(filePath)) {
      return undefined;
    }

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupPath = `${filePath}.backup-${timestamp}`;

    try {
      copyFileSync(filePath, backupPath);
      this.logger.debug(`📋 Created backup: ${backupPath}`);
      
      // Clean up old backups to prevent pollution
      this.cleanupOldBackups(filePath);
      
      return backupPath;
    } catch (error) {
      this.logger.warn(`Failed to create backup for ${filePath}:`, error);
      return undefined;
    }
  }

  /**
   * Clean up old backup files, keeping only the most recent ones
   */
  private cleanupOldBackups(originalFilePath: string): void {
    try {
      const dir = dirname(originalFilePath);
      const filename = basename(originalFilePath);
      const backupPattern = `${filename}.backup-`;

      // Find all backup files for this original file
      const backupFiles = readdirSync(dir)
        .filter(file => file.startsWith(backupPattern))
        .map(file => ({
          name: file,
          path: join(dir, file),
          mtime: statSync(join(dir, file)).mtime
        }))
        .sort((a, b) => b.mtime.getTime() - a.mtime.getTime()); // Sort by modification time, newest first

      // Keep only the most recent backups
      const filesToDelete = backupFiles.slice(this.maxBackups);
      
      if (filesToDelete.length > 0) {
        this.logger.debug(`🧹 Cleaning up ${filesToDelete.length} old backup files for ${filename}`);
        
        for (const fileToDelete of filesToDelete) {
          try {
            unlinkSync(fileToDelete.path);
            this.logger.debug(`   Deleted old backup: ${fileToDelete.name}`);
          } catch (error) {
            this.logger.warn(`Failed to delete backup ${fileToDelete.name}:`, error);
          }
        }
      }
    } catch (error) {
      this.logger.warn(`Failed to cleanup old backups for ${originalFilePath}:`, error);
    }
  }

  /**
   * Read TypeScript/JavaScript file content
   */
  readSourceFile(filePath: string): string | null {
    try {
      if (!existsSync(filePath)) {
        return null;
      }
      return readFileSync(filePath, 'utf8');
    } catch (error) {
      this.logger.error(`Failed to read source file ${filePath}:`, error);
      return null;
    }
  }

  /**
   * Write TypeScript/JavaScript file with header comment
   */
  writeSourceFile(filePath: string, content: string, reason: string): FileUpdate {
    const backupPath = this.createBackup(filePath);

    if (this.dryRun) {
      this.logger.info(`[DRY RUN] Would write ${filePath}: ${reason}`);
      return {
        filePath,
        backupPath,
        content,
        reason
      };
    }

    try {
      // Ensure directory exists
      const dir = dirname(filePath);
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }

      writeFileSync(filePath, content, 'utf8');
      this.logger.info(`✅ Generated ${filePath}: ${reason}`);
      
      return {
        filePath,
        backupPath,
        content,
        reason
      };
    } catch (error) {
      this.logger.error(`Failed to write source file ${filePath}:`, error);
      throw error;
    }
  }

  /**
   * Check if file exists and is readable
   */
  exists(filePath: string): boolean {
    return existsSync(filePath);
  }

  /**
   * Get file modification time for change detection
   */
  getModificationTime(filePath: string): Date | null {
    try {
      if (!existsSync(filePath)) {
        return null;
      }
      const stats = statSync(filePath);
      return stats.mtime;
    } catch {
      return null;
    }
  }

  /**
   * Clean up all existing backup files in a directory
   */
  cleanupAllBackups(directory: string): void {
    try {
      if (!existsSync(directory)) {
        return;
      }

      const files = readdirSync(directory);
      const backupFiles = files
        .filter(file => file.includes('.backup-'))
        .map(file => join(directory, file));

      if (backupFiles.length > 0) {
        this.logger.info(`🧹 Found ${backupFiles.length} backup files in ${directory}`);
        
        let deletedCount = 0;
        for (const backupFile of backupFiles) {
          try {
            unlinkSync(backupFile);
            deletedCount++;
            this.logger.debug(`   Deleted: ${basename(backupFile)}`);
          } catch (error) {
            this.logger.warn(`Failed to delete ${basename(backupFile)}:`, error);
          }
        }
        
        this.logger.info(`✅ Cleaned up ${deletedCount} backup files`);
      }
    } catch (error) {
      this.logger.error(`Failed to cleanup backups in ${directory}:`, error);
    }
  }
}

// ============================================================================
// Configuration File Templates
// ============================================================================

export class ConfigTemplates {
  /**
   * Create minimal path mappings config
   */
  static createPathMappingsConfig(mappings: Record<string, PathMapping>): PathMappingsConfig {
    return {
      version: '2.0.0',
      generatedAt: new Date().toISOString(),
      description: 'Essential path mappings for TypeScript and linter support. Generated by clean modular generator system.',
      mappings
    };
  }

  /**
   * Create structure generation config
   */
  static createStructureConfig(targets: Record<string, StructureTarget>): StructureConfig {
    return {
      version: '2.0.0',  
      generatedAt: new Date().toISOString(),
      description: 'Structure generation configuration for daemon and command registry files.',
      targets
    };
  }

  /**
   * Create TypeScript config update
   */
  static createTypeScriptUpdate(existingConfig: TypeScriptConfig, paths: Record<string, string[]>, sourcePath: string): TypeScriptConfig {
    return {
      ...existingConfig,
      compilerOptions: {
        ...existingConfig.compilerOptions,
        paths
      },
      _pathsGenerated: {
        timestamp: new Date().toISOString(),
        source: sourcePath,
        pathCount: Object.keys(paths).length,
        note: 'Auto-generated from essential path mappings only'
      }
    };
  }
}

// ============================================================================
// Change Detection
// ============================================================================

export class ChangeDetector {
  private fileManager: FileManager;

  constructor(fileManager: FileManager) {
    this.fileManager = fileManager;
  }

  /**
   * Check if regeneration is needed based on file modification times
   */
  needsRegeneration(sourceFiles: string[], targetFile: string): boolean {
    const targetTime = this.fileManager.getModificationTime(targetFile);
    if (!targetTime) {
      return true; // Target doesn't exist
    }

    // Check if any source file is newer than target
    for (const sourceFile of sourceFiles) {
      const sourceTime = this.fileManager.getModificationTime(sourceFile);
      if (sourceTime && sourceTime > targetTime) {
        return true;
      }
    }

    return false;
  }

  /**
   * Get list of files that have changed since last generation
   */
  getChangedFiles(sourceFiles: string[], lastGeneration: Date): string[] {
    return sourceFiles.filter(file => {
      const modTime = this.fileManager.getModificationTime(file);
      return modTime && modTime > lastGeneration;
    });
  }
}