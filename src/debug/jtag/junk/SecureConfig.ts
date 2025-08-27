/**
 * Secure Configuration Manager - Security-First Design
 * 
 * SECURITY ARCHITECTURE:
 * - Server components only get server configuration
 * - Client components only get client-safe configuration  
 * - No cross-contamination of sensitive data
 * - Environment variable override for sensitive values
 * - Configuration validation with proper error handling
 */

import { readFileSync } from 'fs';
import { join } from 'path';
import type { 
  JTAGServerConfiguration,
  JTAGClientConfiguration, 
  JTAGTestConfiguration,
  validateServerConfig,
  validateClientConfig
} from './SecureConfigTypes';

class SecureConfigManager {
  private static instance: SecureConfigManager;
  private configCache = new Map<string, any>();
  private readonly configDir = join(__dirname, '../../config');

  private constructor() {}

  static getInstance(): SecureConfigManager {
    if (!SecureConfigManager.instance) {
      SecureConfigManager.instance = new SecureConfigManager();
    }
    return SecureConfigManager.instance;
  }

  /**
   * Get SERVER-ONLY configuration - NEVER expose to client
   */
  getServerConfig(): JTAGServerConfiguration {
    const cacheKey = 'server';
    
    if (!this.configCache.has(cacheKey)) {
      try {
        const configPath = join(this.configDir, 'server.json');
        const rawConfig = JSON.parse(readFileSync(configPath, 'utf-8'));
        
        // Apply environment variable overrides for sensitive values
        const config: JTAGServerConfiguration = {
          ...rawConfig,
          server: {
            ...rawConfig.server,
            port: parseInt(process.env.JTAG_SERVER_PORT || rawConfig.server.port.toString()),
            host: process.env.JTAG_SERVER_HOST || rawConfig.server.host,
            bind_interface: process.env.JTAG_BIND_INTERFACE || rawConfig.server.bind_interface
          },
          security: {
            ...rawConfig.security,
            enable_authentication: process.env.JTAG_AUTH_ENABLED === 'true' || rawConfig.security.enable_authentication
          }
        };

        this.configCache.set(cacheKey, config);
        console.log(`🔒 Server configuration loaded (port: ${config.server.port})`);
      } catch (error) {
        throw new Error(`Failed to load server configuration: ${error}`);
      }
    }

    return this.configCache.get(cacheKey);
  }

  /**
   * Get CLIENT-SAFE configuration - safe to send to browser
   */
  getClientConfig(): JTAGClientConfiguration {
    const cacheKey = 'client';
    
    if (!this.configCache.has(cacheKey)) {
      try {
        const configPath = join(this.configDir, 'client.json');
        const rawConfig = JSON.parse(readFileSync(configPath, 'utf-8'));
        
        // Apply environment variable overrides for client settings
        const config: JTAGClientConfiguration = {
          ...rawConfig,
          client: {
            ...rawConfig.client,
            ui_port: parseInt(process.env.JTAG_UI_PORT || rawConfig.client.ui_port.toString()),
            host: process.env.JTAG_CLIENT_HOST || rawConfig.client.host
          }
        };

        this.configCache.set(cacheKey, config);
        console.log(`🌐 Client configuration loaded (UI port: ${config.client.ui_port})`);
      } catch (error) {
        throw new Error(`Failed to load client configuration: ${error}`);
      }
    }

    return this.configCache.get(cacheKey);
  }

  /**
   * Get TEST configuration - for testing scenarios only
   */
  getTestConfig(): JTAGTestConfiguration {
    const cacheKey = 'test';
    
    if (!this.configCache.has(cacheKey)) {
      try {
        const configPath = join(this.configDir, 'test.json');
        const rawConfig = JSON.parse(readFileSync(configPath, 'utf-8'));
        
        // Apply test-specific environment overrides
        const config: JTAGTestConfiguration = {
          ...rawConfig,
          server: {
            ...rawConfig.server,
            port: parseInt(process.env.JTAG_TEST_SERVER_PORT || rawConfig.server.port.toString())
          },
          client: {
            ...rawConfig.client,
            ui_port: parseInt(process.env.JTAG_TEST_UI_PORT || rawConfig.client.ui_port.toString())
          }
        };

        this.configCache.set(cacheKey, config);
        console.log(`🧪 Test configuration loaded (server: ${config.server.port}, client: ${config.client.ui_port})`);
      } catch (error) {
        throw new Error(`Failed to load test configuration: ${error}`);
      }
    }

    return this.configCache.get(cacheKey);
  }

  /**
   * Clear configuration cache - useful for testing different configurations
   */
  clearCache(): void {
    this.configCache.clear();
    console.log('🔄 Configuration cache cleared');
  }

  /**
   * Get current environment - determines which config to use
   */
  getEnvironment(): 'development' | 'test' | 'production' {
    return (process.env.NODE_ENV as any) || 'development';
  }
}

// Public API - only expose what each component type needs
export function getServerConfig(): JTAGServerConfiguration {
  return SecureConfigManager.getInstance().getServerConfig();
}

export function getClientConfig(): JTAGClientConfiguration {
  return SecureConfigManager.getInstance().getClientConfig();
}

export function getTestConfig(): JTAGTestConfiguration {
  return SecureConfigManager.getInstance().getTestConfig();
}

export function clearConfigCache(): void {
  SecureConfigManager.getInstance().clearCache();
}

// Environment detection
export function isTestEnvironment(): boolean {
  return process.env.NODE_ENV === 'test' || process.argv.includes('--test');
}

export function isDevelopment(): boolean {
  return process.env.NODE_ENV === 'development' || !process.env.NODE_ENV;
}