/**
 * Browser-Safe Configuration - No Node.js Dependencies
 * 
 * This module provides configuration defaults that work in both browser and server environments.
 * It contains no file system operations or Node.js-specific code.
 */

import type { 
  JTAGServerConfiguration,
  JTAGClientConfiguration, 
  JTAGTestConfiguration
} from './SecureConfigTypes';

/**
 * Runtime environment detection
 */
export const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';
export const isNode = typeof process !== 'undefined' && process.versions && process.versions.node;

/**
 * Get environment variable safely (works in both browser and Node.js)
 */
function getEnvVar(name: string, defaultValue: string): string {
  if (typeof process !== 'undefined' && process.env) {
    return process.env[name] || defaultValue;
  }
  return defaultValue;
}

/**
 * Get environment variable as integer safely
 */
function getEnvInt(name: string, defaultValue: number): number {
  const value = getEnvVar(name, defaultValue.toString());
  const parsed = parseInt(value, 10);
  return isNaN(parsed) ? defaultValue : parsed;
}

/**
 * Browser-safe server configuration defaults
 */
export function getDefaultServerConfig(): JTAGServerConfiguration {
  return {
    server: {
      port: getEnvInt('JTAG_SERVER_PORT', 9001),
      host: getEnvVar('JTAG_SERVER_HOST', 'localhost'),
      protocol: 'ws',
      bind_interface: getEnvVar('JTAG_BIND_INTERFACE', '127.0.0.1'),
      max_connections: 100,
      enable_cors: false
    },
    paths: {
      logs: '.continuum/jtag/logs',
      screenshots: '.continuum/jtag/screenshots',
      data_directory: '.continuum/jtag/data',
      pid_file: '.continuum/jtag/server.pid'
    },
    security: {
      enable_authentication: getEnvVar('JTAG_AUTH_ENABLED', 'false') === 'true',
      session_timeout_ms: 3600000,
      rate_limiting: {
        enabled: false,
        requests_per_minute: 60
      }
    },
    environment: {
      log_level: 'info',
      debug_mode: false
    }
  };
}

/**
 * Browser-safe client configuration defaults
 */
export function getDefaultClientConfig(): JTAGClientConfiguration {
  return {
    client: {
      ui_port: getEnvInt('JTAG_UI_PORT', 9002),
      host: getEnvVar('JTAG_CLIENT_HOST', 'localhost'),
      protocol: 'http',
      auto_connect: true,
      reconnect_attempts: 3
    },
    browser: {
      headless: false,
      devtools: true,
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
 * Browser-safe test configuration defaults
 */
export function getDefaultTestConfig(): JTAGTestConfiguration {
  return {
    server: {
      port: getEnvInt('JTAG_TEST_SERVER_PORT', getEnvInt('JTAG_SERVER_PORT', 9001)),
      host: getEnvVar('JTAG_SERVER_HOST', 'localhost'),
      protocol: 'ws'
    },
    client: {
      ui_port: getEnvInt('JTAG_UI_PORT', 9002),
      host: getEnvVar('JTAG_CLIENT_HOST', 'localhost'),
      protocol: 'http'
    },
    test_settings: {
      timeout_ms: 30000,
      retry_attempts: 3,
      screenshot_on_failure: true,
      cleanup_after_test: true
    },
    environment: {
      test_mode: true,
      verbose_logging: true,
      isolated_sessions: true
    }
  };
}

/**
 * Check if running in test environment (browser-safe)
 */
export function isTestEnvironment(): boolean {
  // Check Node.js environment
  if (typeof process !== 'undefined' && process.env) {
    return process.env.NODE_ENV === 'test' || process.argv?.includes('--test') || false;
  }
  
  // Check browser environment
  if (typeof window !== 'undefined') {
    return window.location?.href?.includes('test') || 
           window.location?.port === '9002' || // Test bench port
           document?.title?.includes('Test') ||
           false;
  }
  
  return false;
}