/**
 * System Configuration Loader
 * 
 * Reads configuration from config.json for core system components.
 * Uses shared types that tests and other consumers can also use.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { JTAGConfiguration } from './ConfigTypes';

class ConfigManager {
  private static _config: JTAGConfiguration | null = null;
  
  /**
   * Load configuration from config.json (called once by system components)
   */
  static load(): JTAGConfiguration {
    if (ConfigManager._config) {
      return ConfigManager._config;
    }

    try {
      const configPath = path.resolve(__dirname, '../../config.json');
      const configData = fs.readFileSync(configPath, 'utf8');
      ConfigManager._config = JSON.parse(configData) as JTAGConfiguration;
      return ConfigManager._config;
    } catch (error) {
      throw new Error(`Failed to load JTAG configuration from config.json: ${error}`);
    }
  }

  /**
   * Get current configuration (loads if needed)
   */
  static get(): JTAGConfiguration {
    return ConfigManager._config || ConfigManager.load();
  }

  /**
   * Reset configuration (for testing)
   */
  static reset(): void {
    ConfigManager._config = null;
  }
}

// Export the configuration interface
export const Config = ConfigManager;

/**
 * Get system configuration - just the typed config object
 * Use config.server.port, config.client.ui_port etc. directly
 */
export function getSystemConfig(): JTAGConfiguration {
  return Config.get();
}