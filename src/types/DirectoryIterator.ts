/**
 * Directory-Level Iterator - validate() and migrate() at group level
 * Commands directory can validate all commands, etc.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { ValidationResult, MigrationResult } from './BaseModule.js';

export class DirectoryIterator {
  protected directoryPath: string;

  constructor(directoryPath: string) {
    this.directoryPath = directoryPath;
  }

  /**
   * Validate all modules in this directory
   */
  async validate(): Promise<DirectoryValidationResult> {
    const modules = await this.discoverModules();
    const results: ValidationResult[] = [];
    
    console.log(`🔍 Validating ${modules.length} modules in ${path.basename(this.directoryPath)}/`);

    // Validate each module
    for (const modulePath of modules) {
      const module = await this.createModuleInstance(modulePath);
      const result = await module.validate();
      results.push(result);
    }

    const compliant = results.filter(r => r.isValid).length;
    const nonCompliant = results.length - compliant;

    return {
      directoryPath: this.directoryPath,
      totalModules: results.length,
      compliantModules: compliant,
      nonCompliantModules: nonCompliant,
      complianceRate: ((compliant / results.length) * 100).toFixed(1),
      moduleResults: results,
      isValid: nonCompliant === 0
    };
  }

  /**
   * Migrate all modules in this directory
   */
  async migrate(): Promise<DirectoryMigrationResult> {
    const modules = await this.discoverModules();
    const results: MigrationResult[] = [];
    
    console.log(`🔄 Migrating ${modules.length} modules in ${path.basename(this.directoryPath)}/`);

    // Migrate each module
    for (const modulePath of modules) {
      const module = await this.createModuleInstance(modulePath);
      const result = await module.migrate();
      results.push(result);
    }

    const migrated = results.filter(r => r.migrated).length;
    const failed = results.filter(r => r.errors.length > 0).length;
    const unchanged = results.length - migrated - failed;

    return {
      directoryPath: this.directoryPath,
      totalModules: results.length,
      migratedModules: migrated,
      failedModules: failed,
      unchangedModules: unchanged,
      moduleResults: results,
      success: failed === 0
    };
  }

  /**
   * Iterator pattern - apply function to all modules
   */
  async forEach(fn: (module: any) => Promise<void>): Promise<void> {
    const modules = await this.discoverModules();
    
    for (const modulePath of modules) {
      const module = await this.createModuleInstance(modulePath);
      await fn(module);
    }
  }

  /**
   * Map pattern - transform all modules
   */
  async map<T>(fn: (module: any) => Promise<T>): Promise<T[]> {
    const modules = await this.discoverModules();
    const results: T[] = [];
    
    for (const modulePath of modules) {
      const module = await this.createModuleInstance(modulePath);
      const result = await fn(module);
      results.push(result);
    }
    
    return results;
  }

  /**
   * Filter pattern - find modules matching criteria
   */
  async filter(predicate: (module: any) => Promise<boolean>): Promise<any[]> {
    const modules = await this.discoverModules();
    const filtered: any[] = [];
    
    for (const modulePath of modules) {
      const module = await this.createModuleInstance(modulePath);
      if (await predicate(module)) {
        filtered.push(module);
      }
    }
    
    return filtered;
  }

  /**
   * Discover immediate child modules (not recursive)
   */
  protected async discoverModules(): Promise<string[]> {
    const modules: string[] = [];
    
    try {
      const entries = await fs.readdir(this.directoryPath, { withFileTypes: true });
      
      for (const entry of entries) {
        if (entry.isDirectory()) {
          const fullPath = path.join(this.directoryPath, entry.name);
          
          // Check if this directory has package.json (is a module)
          try {
            await fs.access(path.join(fullPath, 'package.json'));
            modules.push(fullPath);
          } catch {
            // Not a module, skip
          }
        }
      }
    } catch {
      // Directory doesn't exist or not accessible
    }
    
    return modules;
  }

  protected async createModuleInstance(modulePath: string) {
    const { BaseModule } = await import('./BaseModule.js');
    return new BaseModule(modulePath);
  }
}

export interface DirectoryValidationResult {
  directoryPath: string;
  totalModules: number;
  compliantModules: number;
  nonCompliantModules: number;
  complianceRate: string;
  moduleResults: ValidationResult[];
  isValid: boolean;
}

export interface DirectoryMigrationResult {
  directoryPath: string;
  totalModules: number;
  migratedModules: number;
  failedModules: number;
  unchangedModules: number;
  moduleResults: MigrationResult[];
  success: boolean;
}