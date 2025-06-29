/**
 * Process Registry Interface
 * Auto-discovery and configuration management for daemon processes
 */

import { ProcessConfig } from './IProcessCoordinator.js';

export interface RegistryEntry {
  type: string;
  config: ProcessConfig;
  discovered: Date;
  packageJson: any;
}

/**
 * Registry for auto-discovering and managing daemon process configurations
 * NO manual registration - processes self-register via package.json + exports
 */
export interface IProcessRegistry {
  // Auto-discovery - scans directories for daemon packages
  discoverProcesses(processDir: string): Promise<Map<string, RegistryEntry>>;
  
  // Configuration access
  getAvailable(): Map<string, RegistryEntry>;
  getConfig(processType: string): ProcessConfig | null;
  
  // Validation
  validateConfig(config: ProcessConfig): boolean;
  
  // System queries
  getCapabilities(): Map<string, string[]>;
  findByCapability(capability: string): string[];
}