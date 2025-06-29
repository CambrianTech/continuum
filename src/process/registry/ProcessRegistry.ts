/**
 * Process Registry Implementation
 * Auto-discovers daemon processes from their package.json configurations
 */

import { IProcessRegistry, RegistryEntry } from '../interfaces/IProcessRegistry.js';
import { ProcessConfig } from '../interfaces/IProcessCoordinator.js';
import { readdir, readFile, stat } from 'fs/promises';
import { join, resolve } from 'path';

export class ProcessRegistry implements IProcessRegistry {
  private registry = new Map<string, RegistryEntry>();
  private readonly searchPaths: string[];

  constructor(searchPaths: string[] = ['src/daemons', 'src/process/daemons']) {
    this.searchPaths = searchPaths;
  }

  /**
   * Auto-discover daemon processes by scanning directories
   * Each daemon must have a package.json with daemon configuration
   */
  async discoverProcesses(processDir: string): Promise<Map<string, RegistryEntry>> {
    this.log('🔍 Starting daemon discovery...');
    
    const allSearchPaths = [processDir, ...this.searchPaths];
    
    for (const searchPath of allSearchPaths) {
      try {
        await this.scanDirectory(searchPath);
      } catch (error) {
        this.log(`⚠️ Could not scan ${searchPath}: ${error.message}`, 'warn');
      }
    }

    this.log(`✅ Discovery complete: ${this.registry.size} daemons found`);
    return new Map(this.registry);
  }

  private async scanDirectory(dirPath: string): Promise<void> {
    try {
      const absolutePath = resolve(dirPath);
      const entries = await readdir(absolutePath);

      for (const entry of entries) {
        const entryPath = join(absolutePath, entry);
        const stats = await stat(entryPath);

        if (stats.isDirectory()) {
          await this.processDaemonDirectory(entryPath, entry);
        }
      }
    } catch (error) {
      this.log(`Directory scan error for ${dirPath}: ${error.message}`, 'warn');
    }
  }

  private async processDaemonDirectory(dirPath: string, dirName: string): Promise<void> {
    try {
      const packageJsonPath = join(dirPath, 'package.json');
      
      try {
        const packageData = JSON.parse(await readFile(packageJsonPath, 'utf8'));
        
        if (this.isDaemonPackage(packageData)) {
          await this.registerDaemon(dirPath, dirName, packageData);
        }
      } catch {
        // No package.json or invalid JSON - skip this directory
      }
    } catch (error) {
      this.log(`Error processing daemon directory ${dirPath}: ${error.message}`, 'warn');
    }
  }

  private isDaemonPackage(packageData: any): boolean {
    return packageData?.daemon && 
           packageData?.daemon?.type && 
           packageData?.daemon?.capabilities;
  }

  private async registerDaemon(dirPath: string, dirName: string, packageData: any): Promise<void> {
    const daemonConfig = packageData.daemon;
    const daemonType = daemonConfig.type || dirName;

    // Find entry point
    const entryPoint = await this.findEntryPoint(dirPath, daemonConfig.entryPoint);
    if (!entryPoint) {
      this.log(`⚠️ No valid entry point found for daemon: ${daemonType}`, 'warn');
      return;
    }

    const config: ProcessConfig = {
      type: daemonType,
      entryPoint,
      capabilities: daemonConfig.capabilities || [],
      packagePath: dirPath,
      maxMemory: daemonConfig.maxMemory,
      maxCpu: daemonConfig.maxCpu,
      restartOnCrash: daemonConfig.restartOnCrash !== false
    };

    if (this.validateConfig(config)) {
      const entry: RegistryEntry = {
        type: daemonType,
        config,
        discovered: new Date(),
        packageJson: packageData
      };

      this.registry.set(daemonType, entry);
      this.log(`📦 Registered daemon: ${daemonType} (${config.capabilities.join(', ')})`);
    }
  }

  private async findEntryPoint(dirPath: string, configuredEntry?: string): Promise<string | null> {
    const candidates = [
      configuredEntry,
      'index.ts',
      'index.js', 
      'daemon.ts',
      'daemon.js',
      `${dirPath.split('/').pop()}.ts`,
      `${dirPath.split('/').pop()}.js`
    ].filter(Boolean);

    for (const candidate of candidates) {
      try {
        const entryPath = join(dirPath, candidate!);
        await stat(entryPath);
        return entryPath;
      } catch {
        continue;
      }
    }

    return null;
  }

  // Configuration access
  getAvailable(): Map<string, RegistryEntry> {
    return new Map(this.registry);
  }

  getConfig(processType: string): ProcessConfig | null {
    const entry = this.registry.get(processType);
    return entry ? entry.config : null;
  }

  // Validation
  validateConfig(config: ProcessConfig): boolean {
    if (!config.type || !config.entryPoint || !config.capabilities) {
      this.log(`❌ Invalid daemon config: missing required fields`, 'error');
      return false;
    }

    if (!Array.isArray(config.capabilities)) {
      this.log(`❌ Invalid daemon config: capabilities must be an array`, 'error');
      return false;
    }

    return true;
  }

  // System queries
  getCapabilities(): Map<string, string[]> {
    const capabilities = new Map<string, string[]>();
    
    for (const [type, entry] of this.registry) {
      capabilities.set(type, entry.config.capabilities);
    }
    
    return capabilities;
  }

  findByCapability(capability: string): string[] {
    const matches: string[] = [];
    
    for (const [type, entry] of this.registry) {
      if (entry.config.capabilities.includes(capability)) {
        matches.push(type);
      }
    }
    
    return matches;
  }

  private log(message: string, level: 'info' | 'warn' | 'error' = 'info'): void {
    const timestamp = new Date().toISOString();
    const prefix = `[${timestamp}] [ProcessRegistry]`;
    
    switch (level) {
      case 'error':
        console.error(`${prefix} ❌ ${message}`);
        break;
      case 'warn':
        console.warn(`${prefix} ⚠️ ${message}`);
        break;
      default:
        console.log(`${prefix} ${message}`);
    }
  }
}