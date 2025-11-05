/**
 * Core Module Discovery System
 * 
 * Universal module iterator and dependency management used everywhere:
 * - Testing framework (IntelligentModularTestRunner)
 * - Integration modules (Academy, etc.)
 * - Build systems and validation
 * - Runtime module loading
 */

import * as path from 'path';
import * as fs from 'fs';

/**
 * Standard module types in the Continuum system
 */
export type ModuleType = 'widget' | 'daemon' | 'command' | 'integration' | 'browser-daemon';

/**
 * Module metadata structure (used by testing and runtime)
 */
export interface ModuleInfo {
  name: string;
  path: string;
  type: ModuleType;
  hasPackageJson: boolean;
  hasMainFile: boolean;
  hasTestDir: boolean;
  packageData?: any;
  compliance?: {
    score: number;
    issues: string[];
    warnings: string[];
  };
}

/**
 * Module dependency specification
 */
export interface ModuleDependency {
  name: string;
  type: ModuleType;
  required: boolean;
  config?: Record<string, any>;
  healthCheck?: string;
}

/**
 * Core Module Discovery Engine
 * Used by testing, integrations, and runtime systems
 */
export class ModuleDiscovery {
  private static instance: ModuleDiscovery;
  private rootDir: string;
  private cache: Map<string, ModuleInfo[]> = new Map();

  private constructor(rootDir?: string) {
    this.rootDir = rootDir || this.findProjectRoot();
  }

  static getInstance(rootDir?: string): ModuleDiscovery {
    if (!ModuleDiscovery.instance) {
      ModuleDiscovery.instance = new ModuleDiscovery(rootDir);
    }
    return ModuleDiscovery.instance;
  }

  /**
   * Get base paths for module types (same as testing system)
   */
  getBasePaths(type: ModuleType): string[] {
    switch (type) {
      case 'widget':
        return ['src/ui/components'];
      case 'daemon':
        return ['src/daemons'];
      case 'command':
        return ['src/commands'];
      case 'integration':
        return ['src/integrations'];
      case 'browser-daemon':
        return ['src/ui/browser'];
      default:
        return [];
    }
  }

  /**
   * Discover all modules of a specific type
   */
  async discoverModules(type: ModuleType, useCache: boolean = true): Promise<ModuleInfo[]> {
    const cacheKey = type;
    
    if (useCache && this.cache.has(cacheKey)) {
      return this.cache.get(cacheKey)!;
    }

    const modules: ModuleInfo[] = [];
    const basePaths = this.getBasePaths(type);

    for (const basePath of basePaths) {
      const fullPath = path.join(this.rootDir, basePath);
      if (fs.existsSync(fullPath)) {
        const discoveredModules = await this.scanDirectory(fullPath, type);
        modules.push(...discoveredModules);
      }
    }

    this.cache.set(cacheKey, modules);
    return modules;
  }

  /**
   * Get specific module by name and type
   */
  async getModule(name: string, type: ModuleType): Promise<ModuleInfo | null> {
    const modules = await this.discoverModules(type);
    return modules.find(m => m.name === name) || null;
  }

  /**
   * Get all modules across all types
   */
  async getAllModules(): Promise<Record<ModuleType, ModuleInfo[]>> {
    const types: ModuleType[] = ['widget', 'daemon', 'command', 'integration', 'browser-daemon'];
    const result: Record<string, ModuleInfo[]> = {};

    for (const type of types) {
      result[type] = await this.discoverModules(type);
    }

    return result as Record<ModuleType, ModuleInfo[]>;
  }

  /**
   * Get module dependencies for a specific integration
   */
  async getModuleDependencies(
    dependencySpecs: Record<string, Omit<ModuleDependency, 'name'>>
  ): Promise<Record<string, ModuleInfo | null>> {
    const dependencies: Record<string, ModuleInfo | null> = {};

    for (const [name, spec] of Object.entries(dependencySpecs)) {
      const module = await this.getModule(name, spec.type);
      dependencies[name] = module;
    }

    return dependencies;
  }

  /**
   * Create dependency iterator for modules
   */
  createDependencyIterator<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): {
    names: (keyof T)[];
    required: (keyof T)[];
    configs: Record<keyof T, any>;
    forEach: (callback: (name: keyof T, spec: ModuleDependency) => void) => void;
    map: <R>(callback: (name: keyof T, spec: ModuleDependency) => R) => R[];
    filter: (predicate: (name: keyof T, spec: ModuleDependency) => boolean) => [keyof T, ModuleDependency][];
  } {
    const names = Object.keys(dependencies) as (keyof T)[];
    const required = names.filter(name => dependencies[name].required);
    const configs = Object.fromEntries(
      names.map(name => [name, { ...dependencies[name].config }])
    ) as Record<keyof T, any>;

    return {
      names,
      required,
      configs,
      forEach: (callback) => {
        for (const [name, spec] of Object.entries(dependencies)) {
          callback(name as keyof T, spec as ModuleDependency);
        }
      },
      map: <R>(callback: (name: keyof T, spec: ModuleDependency) => R): R[] => {
        return names.map(name => callback(name, dependencies[name]));
      },
      filter: (predicate) => {
        return Object.entries(dependencies)
          .filter(([name, spec]) => predicate(name as keyof T, spec as ModuleDependency))
          .map(([name, spec]) => [name as keyof T, spec as ModuleDependency]);
      }
    };
  }

  /**
   * Clear module cache (useful for testing)
   */
  clearCache(): void {
    this.cache.clear();
  }

  /**
   * Find project root by looking for package.json or .continuum
   */
  private findProjectRoot(): string {
    let currentDir = process.cwd();
    
    while (currentDir !== path.dirname(currentDir)) {
      if (fs.existsSync(path.join(currentDir, 'package.json')) ||
          fs.existsSync(path.join(currentDir, '.continuum'))) {
        return currentDir;
      }
      currentDir = path.dirname(currentDir);
    }
    
    return process.cwd();
  }

  /**
   * Scan directory for modules (reuses testing logic)
   */
  private async scanDirectory(dirPath: string, type: string): Promise<ModuleInfo[]> {
    const modules: ModuleInfo[] = [];
    
    try {
      const entries = fs.readdirSync(dirPath, { withFileTypes: true });
      
      for (const entry of entries) {
        if (entry.isDirectory()) {
          const modulePath = path.join(dirPath, entry.name);
          const moduleInfo = await this.assessModule(entry.name, modulePath, type as ModuleType);
          if (moduleInfo) {
            modules.push(moduleInfo);
          }
        }
      }
    } catch (error) {
      console.warn(`Failed to scan directory ${dirPath}:`, error);
    }
    
    return modules;
  }

  /**
   * Assess individual module compliance
   */
  private async assessModule(name: string, modulePath: string, type: ModuleType): Promise<ModuleInfo | null> {
    const packageJsonPath = path.join(modulePath, 'package.json');
    const hasPackageJson = fs.existsSync(packageJsonPath);
    
    let packageData: any = null;
    if (hasPackageJson) {
      try {
        packageData = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
      } catch (error) {
        console.warn(`Failed to parse package.json for ${name}:`, error);
      }
    }

    // Detect main file (multiple possible patterns)
    const mainFilePatterns = [
      `${name}.ts`,
      `${name}.js`, 
      `index.ts`,
      `index.js`,
      `${name}Command.ts`,
      `${name}Daemon.ts`,
      `${name}Widget.ts`,
      `${name}Integration.ts`
    ];
    
    const hasMainFile = mainFilePatterns.some(pattern => 
      fs.existsSync(path.join(modulePath, pattern))
    );

    const hasTestDir = fs.existsSync(path.join(modulePath, 'test'));

    return {
      name,
      path: modulePath,
      type,
      hasPackageJson,
      hasMainFile,
      hasTestDir,
      packageData
    };
  }
}

/**
 * Utility functions for common module operations
 */
export class ModuleUtils {
  /**
   * Create mock instances for testing using spread patterns
   */
  static createMockInstances<T extends Record<string, ModuleDependency>>(
    dependencies: T,
    overrides: Partial<Record<keyof T, any>> = {}
  ): Record<keyof T, any> {
    const mocks: Record<string, any> = {};
    
    for (const [name, spec] of Object.entries(dependencies)) {
      mocks[name] = {
        name,
        type: spec.type,
        start: async () => {},
        stop: async () => {},
        sendMessage: async () => ({ success: true, data: {} }),
        ...overrides[name as keyof T] // Spread override for specific mocks
      };
    }
    
    return mocks as Record<keyof T, any>;
  }

  /**
   * Calculate startup order based on dependencies
   */
  static calculateStartupOrder<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): (keyof T)[] {
    // Simple implementation - can be enhanced with dependency graph analysis
    const order: (keyof T)[] = [];
    const names = Object.keys(dependencies) as (keyof T)[];
    
    // Add required dependencies first
    const required = names.filter(name => dependencies[name].required);
    order.push(...required);
    
    // Add optional dependencies
    const optional = names.filter(name => !dependencies[name].required);
    order.push(...optional);
    
    return order;
  }

  /**
   * Get shutdown order (reverse of startup)
   */
  static calculateShutdownOrder<T extends Record<string, ModuleDependency>>(
    dependencies: T
  ): (keyof T)[] {
    return ModuleUtils.calculateStartupOrder(dependencies).reverse();
  }
}

// Singleton instance for global access
export const moduleDiscovery = ModuleDiscovery.getInstance();