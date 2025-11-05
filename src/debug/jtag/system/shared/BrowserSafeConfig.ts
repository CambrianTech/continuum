/**
 * Browser-Safe Configuration - No Node.js Dependencies
 * 
 * This module provides configuration defaults that work in both browser and server environments.
 * Uses package.json-based configuration discovery pattern (NPM standard).
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

/**
 * Runtime environment detection
 */
export const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';
export const isNode = typeof process !== 'undefined' && process.versions && process.versions.node;


/**
 * Load active instance configuration using package.json discovery pattern
 * Server uses package.json files, browser derives from current port
 * NO ENVIRONMENT VARIABLES - Uses NPM-standard configuration discovery
 */
export function loadInstanceConfigForContext(): InstanceConfiguration {
  // Server context - delegate to ExampleConfig
  if (isNode) {
    const { getActiveExample } = eval('require')('../../examples/shared/ExampleConfig');
    const example = getActiveExample();
    
    return {
      name: example.name,
      description: example.description,
      ports: example.ports,
      paths: example.paths,
      capabilities: {}
    };
  }

  // Browser context - use ConfigurationFactory if initialized, otherwise derive from port
  if (isBrowser) {
    try {
      // Try to use ConfigurationFactory if it was initialized
      const { ConfigurationFactory } = eval('require')('./ConfigurationFactory');
      const factory = ConfigurationFactory.getInstance();
      const context = factory.getContext();
      return context.instance;
    } catch {
      // Fallback: derive from current port (requires configuration passing in the future)
      const currentPort = parseInt(window.location.port);
      const websocketPort = currentPort - 1;
      
      // Determine configuration based on port without hardcoded values
      const isWidgetUI = window.location.pathname.includes('widget') || 
                        document.title?.includes('Widget') ||
                        currentPort > 9002; // Widget-UI typically uses higher ports
      
      return {
        name: isWidgetUI ? 'JTAG Widget Development UI' : 'JTAG Test Bench',
        description: isWidgetUI ? 'Focused widget development environment' : 'Full-featured testing environment',
        ports: { http_server: currentPort, websocket_server: websocketPort },
        paths: { 
          directory: isWidgetUI ? 'examples/widget-ui' : 'examples/test-bench', 
          html_file: isWidgetUI ? 'index.html' : 'public/demo.html',
          build_output: 'dist'
        },
        capabilities: {}
      };
    }
  }

  throw new Error('Cannot determine environment');
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