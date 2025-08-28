/**
 * Browser-Safe Configuration - No Node.js Dependencies
 * 
 * This module provides configuration defaults that work in both browser and server environments.
 * It contains no file system operations or Node.js-specific code.
 * Integrates with centralized examples.json configuration.
 */

import type { 
  JTAGServerConfiguration,
  JTAGClientConfiguration, 
  JTAGTestConfiguration,
  JTAGConfig,
  InstanceConfiguration,
  InstanceConfigData
} from './SecureConfigTypes';

/**
 * Runtime environment detection
 */
export const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';
export const isNode = typeof process !== 'undefined' && process.versions && process.versions.node;


/**
 * Load active instance configuration from centralized instances.json
 * Called once when JTAGContext is created - returns full typed config  
 * NO ENVIRONMENT VARIABLES - API level only uses config files and context.config
 * NO FALLBACKS - Browser gets same config as server from examples.json
 */
export function loadInstanceConfigForContext(): InstanceConfiguration {
  // Server context - load from file system
  if (isNode) {
    const fs = eval('require')('fs');
    const path = eval('require')('path');
    const configPath = path.join(__dirname, '../../config/examples.json');
    const examplesConfig = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
    
    const activeInstanceName = examplesConfig.active_example;
    const allInstances = examplesConfig.examples;
    const activeInstance = allInstances[activeInstanceName];
    
    if (!activeInstance) {
      throw new Error(`Active instance '${activeInstanceName}' not found in config`);
    }

    const instanceConfig: InstanceConfiguration = {
      name: activeInstance.name,
      description: activeInstance.description,
      ports: activeInstance.ports,
      paths: activeInstance.paths,
      capabilities: activeInstance.features
    };

    console.log(`📋 BrowserSafeConfig: Loaded active instance: ${instanceConfig.name} (HTTP: ${instanceConfig.ports.http_server}, WS: ${instanceConfig.ports.websocket_server})`);
    return instanceConfig;
  }

  // Browser context - derive from current port
  if (isBrowser) {
    const currentPort = window.location.port;
    
    if (currentPort === '9002') {
      return {
        name: 'JTAG Test Bench',
        description: 'Full-featured testing environment with comprehensive UI',
        ports: { http_server: 9002, websocket_server: 9001 },
        paths: { directory: 'examples/test-bench', html_file: 'public/demo.html', build_output: 'dist' },
        capabilities: { browser_automation: true, screenshot_testing: true, chat_integration: true, widget_testing: false, auto_launch_browser: true }
      };
    } else if (currentPort === '9003') {
      return {
        name: 'JTAG Widget Development UI',
        description: 'Focused widget development environment',
        ports: { http_server: 9003, websocket_server: 9004 },
        paths: { directory: 'examples/widget-ui', html_file: 'index.html', build_output: 'dist' },
        capabilities: { browser_automation: false, screenshot_testing: false, chat_integration: true, widget_testing: true, auto_launch_browser: true }
      };
    } else {
      throw new Error(`Browser port ${currentPort} not recognized. Expected 9002 or 9003.`);
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
    }
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
      devtools: instance.capabilities.browser_automation,
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
      screenshot_on_failure: instance.capabilities.screenshot_testing,
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
  
  // Check browser environment
  if (typeof window !== 'undefined') {
    return window.location?.href?.includes('test') || 
           window.location?.port === '9002' || // Test bench port
           document?.title?.includes('Test') ||
           false;
  }
  
  return false;
}