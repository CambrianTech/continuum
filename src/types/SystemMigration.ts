/**
 * System-Level Mass Migration - handles bulk operations
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { MigrationResult } from './BaseModule';

export class SystemMigration {
  
  /**
   * Discover and migrate all modules of a specific type
   */
  static async migrateAllModules(rootPath: string = './src'): Promise<MigrationResult[]> {
    console.log('🚀 Starting system-wide mass migration...');
    
    const modules = await this.discoverAllModules(rootPath);
    const results: MigrationResult[] = [];
    
    console.log(`Found ${modules.length} modules to migrate\n`);
    
    // Migrate in parallel for speed
    const migrationPromises = modules.map(async (modulePath) => {
      const module = await this.createModuleInstance(modulePath);
      return await module.migrate();
    });
    
    const migrationResults = await Promise.all(migrationPromises);
    results.push(...migrationResults);
    
    const successful = results.filter(r => r.migrated).length;
    const failed = results.filter(r => r.errors.length > 0).length;
    
    console.log(`\n📊 Mass Migration Complete:`);
    console.log(`   ✅ ${successful} modules migrated successfully`);
    console.log(`   ❌ ${failed} modules had errors`);
    console.log(`   📋 ${modules.length - successful - failed} modules already current`);
    
    return results;
  }

  /**
   * Migrate specific module categories (commands, daemons, widgets)
   */
  static async migrateByCategory(category: 'commands' | 'daemons' | 'ui', rootPath: string = './src'): Promise<MigrationResult[]> {
    console.log(`🎯 Migrating all ${category} modules...`);
    
    const categoryPath = path.join(rootPath, category);
    return await this.migrateAllModules(categoryPath);
  }

  /**
   * Discover all modules in a directory tree
   */
  static async discoverAllModules(startPath: string): Promise<string[]> {
    const modules: string[] = [];
    
    async function scan(dir: string) {
      try {
        const entries = await fs.readdir(dir, { withFileTypes: true });
        
        for (const entry of entries) {
          const fullPath = path.join(dir, entry.name);
          
          if (entry.isDirectory()) {
            // If this directory has package.json, it's a module
            try {
              await fs.access(path.join(fullPath, 'package.json'));
              modules.push(fullPath);
            } catch {
              // No package.json, scan subdirectories
              await scan(fullPath);
            }
          }
        }
      } catch {
        // Skip inaccessible directories
      }
    }
    
    await scan(startPath);
    return modules;
  }

  /**
   * Create appropriate module instance based on type
   */
  static async createModuleInstance(modulePath: string) {
    // Import using dynamic ES module import to avoid circular dependency
    const { BaseModule } = await import('./BaseModule.js');
    return new BaseModule(modulePath);
  }
}