/**
 * ModuleGenerator - Base class for all module generators (commands, daemons, widgets, etc.)
 *
 * Provides common functionality for:
 * - Checking existing modules
 * - Creating backups
 * - Writing files
 * - Force overwrite logic
 *
 * Subclasses implement module-specific logic:
 * - getModuleType(): 'command' | 'daemon' | 'widget'
 * - getOutputDir(spec): where to write files
 * - renderTemplates(spec): render module-specific templates
 * - writeFiles(baseDir, rendered): write rendered templates to disk
 */

import * as fs from 'fs';
import * as path from 'path';

export interface GenerateOptions {
  force?: boolean;
  backup?: boolean;
}

export interface FileToWrite {
  path: string;
  content: string;
}

/**
 * Base class for all module generators
 * Uses template method pattern to allow subclasses to customize generation
 */
export abstract class ModuleGenerator<TSpec> {
  protected rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Generate module files from a spec
   *
   * Template method that orchestrates the generation process
   */
  generate(spec: TSpec, outputDir?: string, options?: GenerateOptions): void {
    const moduleName = this.getModuleName(spec);
    console.log(`📝 Generating ${this.getModuleType()} module: ${moduleName}`);

    // Determine output directory
    const baseDir = outputDir ?? this.getOutputDir(spec);

    // Check if module already exists
    this.checkExisting(baseDir, options);

    // Render templates
    const rendered = this.renderTemplates(spec);

    // Write files
    this.writeFiles(baseDir, rendered);

    console.log(`\n✨ ${this.getModuleType()} module generated successfully: ${moduleName}`);
  }

  /**
   * Check if module already exists and handle accordingly
   */
  protected checkExisting(baseDir: string, options?: GenerateOptions): void {
    if (fs.existsSync(baseDir)) {
      if (options?.force) {
        console.log(`⚠️  Module already exists at: ${baseDir}`);

        if (options?.backup) {
          this.createBackup(baseDir);
        }

        console.log(`🔄 Overwriting existing module (--force)`);
      } else {
        console.error(`\n❌ ERROR: Module already exists at: ${baseDir}`);
        console.error(`\nOptions:`);
        console.error(`  1. Use --force to overwrite existing module`);
        console.error(`  2. Use --force --backup to backup before overwriting`);
        console.error(`  3. Specify different output directory:`);
        console.error(`     npx tsx generator/generate-structure.ts spec.json /tmp/output`);
        console.error(`  4. Delete existing module first:`);
        console.error(`     rm -rf ${baseDir}`);
        process.exit(1);
      }
    }
  }

  /**
   * Create backup of existing module
   */
  protected createBackup(baseDir: string): void {
    const backupDir = `${baseDir}.backup.${Date.now()}`;
    console.log(`📦 Creating backup: ${backupDir}`);
    fs.cpSync(baseDir, backupDir, { recursive: true });
  }

  /**
   * Write a single file to disk, creating directories as needed
   */
  protected writeFile(filePath: string, content: string): void {
    const dir = path.dirname(filePath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    fs.writeFileSync(filePath, content, 'utf-8');
    console.log(`✅ Created: ${filePath}`);
  }

  /**
   * Write multiple files to disk
   */
  protected writeMultipleFiles(files: FileToWrite[]): void {
    for (const file of files) {
      this.writeFile(file.path, file.content);
    }
  }

  // ==================== Abstract methods - subclasses must implement ====================

  /**
   * Get module type ('command', 'daemon', 'widget', etc.)
   */
  protected abstract getModuleType(): string;

  /**
   * Get module name from spec (for logging)
   */
  protected abstract getModuleName(spec: TSpec): string;

  /**
   * Get output directory for this module
   */
  protected abstract getOutputDir(spec: TSpec): string;

  /**
   * Render all templates for this module
   * Returns rendered content for each file
   */
  protected abstract renderTemplates(spec: TSpec): Record<string, string>;

  /**
   * Write rendered templates to disk
   *
   * @param baseDir - Base directory for module
   * @param rendered - Rendered templates (keyed by file type)
   */
  protected abstract writeFiles(baseDir: string, rendered: Record<string, string>): void;
}
