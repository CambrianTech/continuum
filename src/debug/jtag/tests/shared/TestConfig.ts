/**
 * Test Configuration Loader
 * 
 * Uses the same JTAGConfiguration types as the system, but loads from tests.json.
 * Tests can use this for consistent configuration access.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { JTAGConfiguration } from '../../system/shared/ConfigTypes';

interface TestsFile {
  config: JTAGConfiguration;
  test_settings: {
    timeout_ms: number;
    retry_attempts: number;
    screenshot_on_failure: boolean;
  };
}

class TestConfigManager {
  private static _config: JTAGConfiguration | null = null;
  
  /**
   * Load test configuration from tests.json
   */
  static load(): JTAGConfiguration {
    if (TestConfigManager._config) {
      return TestConfigManager._config;
    }

    try {
      const configPath = path.resolve(__dirname, '../tests.json');
      const configData = fs.readFileSync(configPath, 'utf8');
      const testsFile = JSON.parse(configData) as TestsFile;
      TestConfigManager._config = testsFile.config;
      return TestConfigManager._config;
    } catch (error) {
      // Fallback to default test config
      console.warn(`Could not load tests.json, using defaults: ${error}`);
      return TestConfigManager.getDefaults();
    }
  }

  /**
   * Get test configuration
   */
  static get(): JTAGConfiguration {
    return TestConfigManager._config || TestConfigManager.load();
  }

  /**
   * Default test configuration (same structure as system config)
   * Uses dynamic port resolution instead of hardcoded values
   */
  private static getDefaults(): JTAGConfiguration {
    // Get dynamic ports from active example configuration
    const { getActivePortsSync } = require('../../examples/server/ExampleConfigServer');
    const activePorts = getActivePortsSync();
    
    return {
      server: {
        port: activePorts.websocket_server,
        host: 'localhost',
        protocol: 'ws'
      },
      client: {
        ui_port: activePorts.http_server,
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
   * Reset for testing
   */
  static reset(): void {
    TestConfigManager._config = null;
  }
}

// Export the test configuration manager
export const TestConfig = TestConfigManager;

/**
 * Get test configuration - just the typed config object
 * Use config.server.port, config.client.ui_port etc. directly
 */
export function getTestConfig(): JTAGConfiguration {
  return TestConfig.get();
}