/**
 * Test Configuration Types
 * 
 * Provides typed configuration for JTAG test environments
 * Uses dynamic port resolution instead of hardcoded values
 */

import type { ConnectionConfig } from './ConnectionConfig';

export interface TestServerConfig {
  /** WebSocket server host */
  host: string;
  /** Protocol for WebSocket (ws/wss) */
  protocol: 'ws' | 'wss';
}

export interface TestClientConfig {
  /** HTTP server host */
  host: string;
  /** Protocol for HTTP (http/https) */
  protocol: 'http' | 'https';
}

export interface TestPathConfig {
  /** Path to test bench directory (relative to JTAG root) */
  test_bench: string;
  /** Path to log directory (relative to working directory) */
  logs: string;
  /** Path to screenshots directory (relative to working directory) */
  screenshots: string;
}

export interface BrowserConfig {
  /** Run browser in headless mode */
  headless: boolean;
  /** Open browser devtools */
  devtools: boolean;
  /** Browser window width */
  width: number;
  /** Browser window height */
  height: number;
}

export interface TestSettings {
  /** Test timeout in milliseconds */
  timeout_ms: number;
  /** Number of retry attempts for failed tests */
  retry_attempts: number;
  /** Take screenshot when test fails */
  screenshot_on_failure: boolean;
  /** Clean up test artifacts after completion */
  cleanup_after_test: boolean;
}

export interface TestEnvironmentConfig {
  /** Enable test mode behaviors */
  test_mode: boolean;
  /** Enable verbose logging */
  verbose_logging: boolean;
  /** Use isolated test sessions */
  isolated_sessions?: boolean;
}

/**
 * Complete test configuration combining static and dynamic elements
 */
export interface TestConfig {
  /** Server configuration (ports resolved dynamically) */
  server: TestServerConfig;
  /** Client configuration (ports resolved dynamically) */
  client: TestClientConfig;
  /** Path configuration */
  paths: TestPathConfig;
  /** Browser configuration */
  browser: BrowserConfig;
  /** Test-specific settings */
  test_settings?: TestSettings;
  /** Environment configuration */
  environment: TestEnvironmentConfig;
}

/**
 * Runtime test configuration with resolved ports
 */
export interface ResolvedTestConfig extends TestConfig {
  /** Connection configuration with actual port numbers */
  connection: ConnectionConfig;
}

/**
 * Factory for creating test configurations with dynamic port resolution
 */
export class TestConfigFactory {
  /**
   * Create a test configuration with ports resolved from active example
   */
  static async createResolvedConfig(baseConfig: TestConfig): Promise<ResolvedTestConfig> {
    const { getActivePorts } = require('../../examples/shared/ExampleConfig');
    const activePorts = await getActivePorts();
    
    return {
      ...baseConfig,
      connection: {
        websocketPort: activePorts.websocket_server,
        httpPort: activePorts.http_server,
        workingDir: process.cwd(),
        exampleName: 'test-environment'
      }
    };
  }
  
  /**
   * Create default test configuration
   */
  static createDefault(): TestConfig {
    return {
      server: {
        host: 'localhost',
        protocol: 'ws'
      },
      client: {
        host: 'localhost',
        protocol: 'http'
      },
      paths: {
        test_bench: 'test-bench',
        logs: '.continuum/jtag/logs',
        screenshots: '.continuum/jtag/screenshots'
      },
      browser: {
        headless: false,
        devtools: true,
        width: 1200,
        height: 800
      },
      environment: {
        test_mode: true,
        verbose_logging: false
      }
    };
  }
  
  /**
   * Create test configuration with advanced test settings
   */
  static createWithTestSettings(): TestConfig {
    const base = this.createDefault();
    return {
      ...base,
      test_settings: {
        timeout_ms: 30000,
        retry_attempts: 3,
        screenshot_on_failure: true,
        cleanup_after_test: true
      },
      environment: {
        ...base.environment,
        verbose_logging: true,
        isolated_sessions: true
      }
    };
  }
}