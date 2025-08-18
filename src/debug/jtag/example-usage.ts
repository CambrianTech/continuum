/**
 * Example: How to properly use typed configuration
 */

import { getSystemConfig } from './system/shared/Config';
import { getTestConfig } from './tests/shared/TestConfig';
import type { JTAGConfiguration } from './system/shared/ConfigTypes';

// System component usage - reads config.json
function startSystemServer() {
  const config = getSystemConfig();
  
  // Clean, direct access to typed values
  const port = config.server.port;          // 9001
  const host = config.server.host;          // 'localhost' 
  const protocol = config.server.protocol;  // 'ws'
  const testBench = config.paths.test_bench; // 'test-bench'
  
  console.log(`Starting server on ${protocol}://${host}:${port}`);
  console.log(`Test bench: ${testBench}`);
  console.log(`Browser: ${config.browser.width}x${config.browser.height}`);
}

// Test usage - reads tests.json OR uses custom config
function runTests(customConfig?: JTAGConfiguration) {
  const config = customConfig || getTestConfig();
  
  // Same clean access - same types
  const port = config.server.port;
  const host = config.server.host;
  const isTestMode = config.environment.test_mode;
  
  console.log(`Test server: ${host}:${port}`);
  console.log(`Test mode: ${isTestMode}`);
}

// Custom test with different values
function runCustomTest() {
  const customConfig: JTAGConfiguration = {
    server: { port: 8001, host: '127.0.0.1', protocol: 'ws' },
    client: { ui_port: 8002, host: '127.0.0.1', protocol: 'http' },
    paths: { test_bench: 'custom-bench', logs: '/tmp/logs', screenshots: '/tmp/shots' },
    browser: { headless: true, devtools: false, width: 800, height: 600 },
    environment: { test_mode: true, verbose_logging: true }
  };
  
  runTests(customConfig);
}

export { startSystemServer, runTests, runCustomTest };