/**
 * Package.json-based Test Configuration Loader
 * 
 * Discovers and loads EnvironmentTestConfig from command package.json files
 * instead of generated configuration files.
 */

import { promises as fs } from 'fs';
import { join } from 'path';
import type { EnvironmentTestConfig } from './CrossEnvironmentTestUtils';

export interface CommandPackageConfig {
  name: string;
  version: string;
  description: string;
  config?: {
    category?: string;
  };
  environmentTestConfig?: EnvironmentTestConfig & {
    testScenarios?: Array<{
      name: string;
      description: string;
      params: Record<string, unknown>;
    }>;
  };
}

export interface DiscoveredCommand {
  name: string;
  path: string; 
  packageConfig: CommandPackageConfig;
  environmentTestConfig: EnvironmentTestConfig;
  category: string;
}

/**
 * Recursively discover all command package.json files
 */
async function discoverCommandPackageFiles(basePath: string): Promise<string[]> {
  const packageFiles: string[] = [];
  
  try {
    const entries = await fs.readdir(basePath, { withFileTypes: true });
    
    for (const entry of entries) {
      const fullPath = join(basePath, entry.name);
      
      if (entry.isDirectory()) {
        // Check if this directory has a package.json
        const packagePath = join(fullPath, 'package.json');
        try {
          await fs.access(packagePath);
          packageFiles.push(packagePath);
        } catch {
          // No package.json, continue recursing
        }
        
        // Recurse into subdirectories
        const subPackages = await discoverCommandPackageFiles(fullPath);
        packageFiles.push(...subPackages);
      }
    }
  } catch (error) {
    console.warn(`Failed to discover package files in ${basePath}:`, error instanceof Error ? error.message : String(error));
  }
  
  return packageFiles;
}

/**
 * Load and validate a command package.json file
 */
async function loadCommandPackage(packagePath: string): Promise<DiscoveredCommand | null> {
  try {
    const content = await fs.readFile(packagePath, 'utf8');
    const packageConfig: CommandPackageConfig = JSON.parse(content);
    
    // Skip if no environmentTestConfig
    if (!packageConfig.environmentTestConfig) {
      return null;
    }
    
    // Extract command name from path or package name
    const pathParts = packagePath.split('/');
    const commandsIndex = pathParts.findIndex(part => part === 'commands');
    const commandPathParts = pathParts.slice(commandsIndex + 1, -1); // Remove 'package.json'
    const commandName = commandPathParts.join('/');
    
    return {
      name: commandName,
      path: packagePath.replace('/package.json', ''),
      packageConfig,
      environmentTestConfig: packageConfig.environmentTestConfig,
      category: packageConfig.config?.category || 'core'
    };
  } catch (error) {
    console.warn(`Failed to load package ${packagePath}:`, error instanceof Error ? error.message : String(error));
    return null;
  }
}

/**
 * Discover all commands with package.json-based test configurations
 */
export async function discoverCommandConfigs(commandsBasePath: string): Promise<DiscoveredCommand[]> {
  console.log(`🔍 Discovering command configs from package.json files in ${commandsBasePath}...`);
  
  const packageFiles = await discoverCommandPackageFiles(commandsBasePath);
  console.log(`📦 Found ${packageFiles.length} package.json files`);
  
  const commands: DiscoveredCommand[] = [];
  
  for (const packageFile of packageFiles) {
    const command = await loadCommandPackage(packageFile);
    if (command) {
      commands.push(command);
      console.log(`✅ Loaded config for ${command.name} (${command.category})`);
    }
  }
  
  console.log(`🎯 Discovered ${commands.length} commands with test configurations`);
  return commands;
}

/**
 * Get all environment test configs for testing
 */
export async function getAllEnvironmentTestConfigs(commandsBasePath: string): Promise<EnvironmentTestConfig[]> {
  const commands = await discoverCommandConfigs(commandsBasePath);
  return commands.map(cmd => cmd.environmentTestConfig);
}

/**
 * Get test config for a specific command
 */
export async function getCommandTestConfig(commandsBasePath: string, commandName: string): Promise<EnvironmentTestConfig | null> {
  const commands = await discoverCommandConfigs(commandsBasePath);
  const command = commands.find(cmd => cmd.name === commandName);
  return command?.environmentTestConfig || null;
}