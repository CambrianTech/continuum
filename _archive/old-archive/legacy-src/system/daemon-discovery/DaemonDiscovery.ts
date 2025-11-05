/**
 * DaemonDiscovery - Automatic daemon discovery and loading
 * 
 * Discovers daemons by scanning the daemons directory for package.json files
 * with continuum.type = "daemon", similar to how commands are discovered.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { DaemonInfo } from '../../types/DaemonTypes';

export class DaemonDiscovery {
  private daemonsPath: string;
  
  constructor(basePath?: string) {
    if (basePath) {
      this.daemonsPath = path.join(basePath, 'src/daemons');
    } else {
      // Auto-detect project root
      const currentDir = path.dirname(fileURLToPath(import.meta.url));
      this.daemonsPath = path.join(currentDir, '../../daemons');
    }
  }
  
  /**
   * Discover all daemons in the daemons directory
   */
  async discoverDaemons(): Promise<DaemonInfo[]> {
    const daemons: DaemonInfo[] = [];
    
    try {
      const entries = await fs.readdir(this.daemonsPath, { withFileTypes: true });
      
      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        
        const daemonPath = path.join(this.daemonsPath, entry.name);
        const packageJsonPath = path.join(daemonPath, 'package.json');
        
        try {
          const packageJsonContent = await fs.readFile(packageJsonPath, 'utf-8');
          const packageJson = JSON.parse(packageJsonContent);
          
          // Check if this is a daemon package
          if (packageJson.continuum?.type === 'daemon') {
            const className = this.inferClassName(entry.name, packageJson);
            
            daemons.push({
              name: entry.name,
              path: daemonPath,
              packageJson,
              className
            });
          }
        } catch (error) {
          // Skip directories without package.json or invalid JSON
          continue;
        }
      }
    } catch (error) {
      console.error('Error discovering daemons:', error);
    }
    
    return daemons;
  }
  
  /**
   * Load and instantiate all discovered daemons
   */
  async loadDaemons(): Promise<Map<string, BaseDaemon>> {
    const discoveredDaemons = await this.discoverDaemons();
    const loadedDaemons = new Map<string, BaseDaemon>();
    
    for (const daemonInfo of discoveredDaemons) {
      try {
        const daemon = await this.loadDaemon(daemonInfo);
        if (daemon) {
          loadedDaemons.set(daemonInfo.name, daemon);
        }
      } catch (error) {
        console.error(`Failed to load daemon ${daemonInfo.name}:`, error);
      }
    }
    
    return loadedDaemons;
  }
  
  /**
   * Load a single daemon
   */
  private async loadDaemon(daemonInfo: DaemonInfo): Promise<BaseDaemon | null> {
    try {
      // Determine the main file to import
      const mainFile = daemonInfo.packageJson.main || `${daemonInfo.className}.ts`;
      const modulePath = path.join(daemonInfo.path, mainFile.replace('.js', '.ts'));
      
      // Dynamic import
      const module = await import(modulePath);
      
      // Find the daemon class - could be default export or named export
      const DaemonClass = module.default || module[daemonInfo.className];
      
      if (!DaemonClass) {
        console.error(`No daemon class found in ${modulePath}`);
        return null;
      }
      
      // Instantiate the daemon
      const daemon = new DaemonClass();
      
      // Verify it's a daemon
      if (!(daemon instanceof BaseDaemon)) {
        console.error(`${daemonInfo.className} is not a BaseDaemon`);
        return null;
      }
      
      return daemon;
    } catch (error) {
      console.error(`Error loading daemon ${daemonInfo.name}:`, error);
      return null;
    }
  }
  
  /**
   * Infer the class name from the directory name
   */
  private inferClassName(dirName: string, packageJson: any): string {
    // Check if className is specified in package.json
    if (packageJson.continuum?.className) {
      return packageJson.continuum.className;
    }
    
    // Convert kebab-case to PascalCase and append "Daemon"
    const pascalCase = dirName
      .split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join('');
    
    return pascalCase + 'Daemon';
  }
}

// Export singleton instance
export const daemonDiscovery = new DaemonDiscovery();