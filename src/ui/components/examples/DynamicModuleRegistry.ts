/**
 * Dynamic Module Registry - Runtime Module Discovery & Loading
 * ===========================================================
 * Discovers and loads modules dynamically while preserving strong types
 */


export interface ModuleDefinition {
  readonly name: string;
  readonly version: string;
  readonly type: 'widget' | 'processor' | 'handler' | 'validator';
  readonly capabilities: readonly string[];
  readonly dependencies: readonly string[];
  readonly entry: string; // Module entry point
}

export interface DynamicModule<T = unknown> {
  readonly definition: ModuleDefinition;
  readonly exports: T;
  readonly isLoaded: boolean;
}

export type ModuleLoader<T> = () => Promise<T>;

export class DynamicModuleRegistry {
  private readonly modules = new Map<string, DynamicModule>();
  private readonly loaders = new Map<string, ModuleLoader<unknown>>();

  /**
   * Register a module loader (happens at build time or startup)
   */
  registerModuleLoader<T>(name: string, loader: ModuleLoader<T>): void {
    this.loaders.set(name, loader);
  }

  /**
   * Dynamically discover modules from package.json files
   */
  async discoverModules(searchPaths: readonly string[]): Promise<ModuleDefinition[]> {
    const discovered: ModuleDefinition[] = [];

    for (const path of searchPaths) {
      try {
        const packagePath = `${path}/package.json`;
        const response = await fetch(packagePath);
        
        if (response.ok) {
          const packageJson = await response.json();
          
          if (packageJson.continuum?.type && packageJson.continuum?.widgets) {
            const moduleDefinition = this.extractModuleDefinition(packageJson, path);
            if (moduleDefinition) {
              discovered.push(moduleDefinition);
            }
          }
        }
      } catch (error) {
        console.warn(`Failed to discover module at ${path}:`, error);
      }
    }

    return discovered;
  }

  /**
   * Load module dynamically with strong typing
   */
  async loadModule<T>(name: string): Promise<DynamicModule<T> | null> {
    const existing = this.modules.get(name);
    if (existing?.isLoaded) {
      return existing as DynamicModule<T>;
    }

    const loader = this.loaders.get(name);
    if (!loader) {
      console.warn(`No loader registered for module: ${name}`);
      return null;
    }

    try {
      const exports = await loader();
      const module: DynamicModule<T> = {
        definition: this.getModuleDefinition(name),
        exports: exports as T,
        isLoaded: true
      };

      this.modules.set(name, module);
      return module;
    } catch (error) {
      console.error(`Failed to load module ${name}:`, error);
      return null;
    }
  }

  /**
   * Get modules by capability (for composition)
   */
  getModulesByCapability(capability: string): string[] {
    return Array.from(this.modules.values())
      .filter(module => module.definition.capabilities.includes(capability))
      .map(module => module.definition.name);
  }

  /**
   * Get modules by type
   */
  getModulesByType(type: ModuleDefinition['type']): string[] {
    return Array.from(this.modules.values())
      .filter(module => module.definition.type === type)
      .map(module => module.definition.name);
  }

  /**
   * Check if module is loaded
   */
  isModuleLoaded(name: string): boolean {
    return this.modules.get(name)?.isLoaded ?? false;
  }

  /**
   * Get all loaded modules
   */
  getLoadedModules(): readonly DynamicModule[] {
    return Array.from(this.modules.values()).filter(m => m.isLoaded);
  }

  private extractModuleDefinition(packageJson: any, path: string): ModuleDefinition | null {
    const continuum = packageJson.continuum;
    if (!continuum) return null;

    return {
      name: packageJson.name || path.split('/').pop() || 'unknown',
      version: packageJson.version || '0.0.0',
      type: continuum.type,
      capabilities: continuum.capabilities || [],
      dependencies: packageJson.dependencies ? Object.keys(packageJson.dependencies) : [],
      entry: continuum.main || packageJson.main || 'index.ts'
    };
  }

  private getModuleDefinition(name: string): ModuleDefinition {
    const existing = this.modules.get(name);
    if (existing?.definition) {
      return existing.definition;
    }

    // Fallback definition
    return {
      name,
      version: '0.0.0',
      type: 'widget',
      capabilities: [],
      dependencies: [],
      entry: 'index.ts'
    };
  }
}

// Global registry instance
export const moduleRegistry = new DynamicModuleRegistry();