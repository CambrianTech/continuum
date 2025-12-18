/**
 * Browser-Safe Configuration - DEPRECATED
 *
 * This module is deprecated. Use shared/config.ts instead (generated at build time).
 * Kept temporarily for backward compatibility during migration.
 */

import type {
  JTAGServerConfiguration,
  JTAGClientConfiguration,
  JTAGTestConfiguration,
  JTAGConfig,
  InstanceConfiguration,
  InstanceConfigData
} from './SecureConfigTypes';

import { DEFAULT_STORAGE_CONFIG } from './SecureConfigTypes';
import { EXAMPLE_CONFIG, HTTP_PORT, WS_PORT } from '../../shared/config';

/**
 * Runtime environment detection
 */
export const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';
export const isNode = typeof process !== 'undefined' && process.versions && process.versions.node;

/**
 * Load instance configuration from generated config
 * @deprecated Use shared/config.ts directly instead
 */
export function loadInstanceConfigForContext(): InstanceConfiguration {
  return {
    name: EXAMPLE_CONFIG.name,
    description: EXAMPLE_CONFIG.description,
    ports: EXAMPLE_CONFIG.ports,
    paths: EXAMPLE_CONFIG.paths,
    capabilities: {}
  };
}

/**
 * Create complete JTAGConfig for context - Android-style configuration
 */
export function createJTAGConfig(): JTAGConfig {
  const instance = loadInstanceConfigForContext();
  
  return {
    instance,
    server: getDefaultServerConfig(instance),
    client: getDefaultClientConfig(instance),
    test: getDefaultTestConfig(instance)
  };
}

/**
 * Browser-safe server configuration using instance config  
 * NO ENVIRONMENT VARIABLES - API level only uses context.config
 */
export function getDefaultServerConfig(instance: InstanceConfiguration): JTAGServerConfiguration {
  return {
    server: {
      port: instance.ports.websocket_server,
      host: 'localhost',
      protocol: 'ws',
      bind_interface: '127.0.0.1',
      max_connections: 100,
      enable_cors: false
    },
    paths: {
      logs: `.continuum/jtag/logs`,
      screenshots: `.continuum/jtag/screenshots`,
      data_directory: `.continuum/jtag/data`,
      pid_file: `.continuum/jtag/server.pid`
    },
    security: {
      enable_authentication: false,
      session_timeout_ms: 3600000,
      rate_limiting: {
        enabled: false,
        requests_per_minute: 60
      }
    },
    environment: {
      log_level: 'info',
      debug_mode: false
    },
    storage: DEFAULT_STORAGE_CONFIG
  };
}

/**
 * Browser-safe client configuration using instance config
 */
export function getDefaultClientConfig(instance: InstanceConfiguration): JTAGClientConfiguration {
  return {
    client: {
      ui_port: instance.ports.http_server,
      host: 'localhost',
      protocol: 'http',
      auto_connect: true,
      reconnect_attempts: 3
    },
    browser: {
      headless: false,
      devtools: true, // Always enabled - no artificial limitations
      width: 1200,
      height: 800,
      user_agent: 'JTAG-TestBrowser/1.0'
    },
    ui: {
      theme: 'dark',
      enable_animations: true,
      show_debug_panel: false
    }
  };
}

/**
 * Browser-safe test configuration using instance config
 * NO ENVIRONMENT VARIABLES - API level only uses context.config
 */
export function getDefaultTestConfig(instance: InstanceConfiguration): JTAGTestConfiguration {
  return {
    server: {
      port: instance.ports.websocket_server,
      host: 'localhost',
      protocol: 'ws'
    },
    client: {
      ui_port: instance.ports.http_server,
      host: 'localhost',
      protocol: 'http'
    },
    test_settings: {
      timeout_ms: 30000,
      retry_attempts: 3,
      screenshot_on_failure: true, // Always enabled - no artificial limitations
      cleanup_after_test: true
    },
    environment: {
      test_mode: true,
      verbose_logging: true,
      isolated_sessions: true
    }
  };
}

// Backward compatibility functions - deprecated
export function getDefaultServerConfigLegacy(): JTAGServerConfiguration {
  const instance = loadInstanceConfigForContext();
  return getDefaultServerConfig(instance);
}

export function getDefaultClientConfigLegacy(): JTAGClientConfiguration {
  const instance = loadInstanceConfigForContext();
  return getDefaultClientConfig(instance);
}

export function getDefaultTestConfigLegacy(): JTAGTestConfiguration {
  const instance = loadInstanceConfigForContext();
  return getDefaultTestConfig(instance);
}

/**
 * Check if running in test environment (browser-safe)
 */
export function isTestEnvironment(): boolean {
  // Check Node.js environment
  if (typeof process !== 'undefined' && process.env) {
    return process.env.NODE_ENV === 'test' || process.argv?.includes('--test') || false;
  }
  
  // Check browser environment - detect test mode without hardcoded ports
  if (typeof window !== 'undefined') {
    return window.location?.href?.includes('test') || 
           window.location?.pathname?.includes('test-bench') || 
           document?.title?.includes('Test') ||
           false;
  }
  
  return false;
}