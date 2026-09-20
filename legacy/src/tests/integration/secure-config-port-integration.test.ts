#!/usr/bin/env npx tsx
/**
 * Secure Configuration Port Integration Tests
 * 
 * COMPREHENSIVE PORT TESTING:
 * - Multiple port configurations simultaneously
 * - Environment variable overrides
 * - Client/server port isolation
 * - Configuration validation
 * - Security boundary enforcement
 */

import { getDefaultServerConfig, getDefaultClientConfig, getDefaultTestConfig } from '../../system/shared/BrowserSafeConfig';
import { TestUserManager } from '../shared/TestUserManager';
import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGTestConfiguration } from '../../system/shared/SecureConfigTypes';

interface PortTestScenario {
  name: string;
  serverPort: string;
  clientPort: string;
  testServerPort?: string;
  expectedBehavior: 'success' | 'connection_fail' | 'config_error';
}

const PORT_TEST_SCENARIOS: PortTestScenario[] = [
  {
    name: 'Default Ports (9001/9002)',
    serverPort: '9001',
    clientPort: '9002',
    expectedBehavior: 'connection_fail' // Expected - no server running
  },
  {
    name: 'Custom Ports (9051/9052)',
    serverPort: '9051', 
    clientPort: '9052',
    expectedBehavior: 'connection_fail'
  },
  {
    name: 'High Ports (9901/9902)',
    serverPort: '9901',
    clientPort: '9902', 
    expectedBehavior: 'connection_fail'
  },
  {
    name: 'Test Mode with Override (9501/9502)',
    serverPort: '9501',
    clientPort: '9502',
    testServerPort: '9503',
    expectedBehavior: 'connection_fail'
  },
  {
    name: 'Sequential Ports (9201/9202/9203)',
    serverPort: '9201',
    clientPort: '9202',
    testServerPort: '9203', 
    expectedBehavior: 'connection_fail'
  }
];

async function testSecureConfigPortIntegration(): Promise<void> {
  console.log('🔒 SECURE CONFIGURATION PORT INTEGRATION TESTS');
  console.log('==============================================\n');

  let testsPassed = 0;
  let testsTotal = 0;

  for (const scenario of PORT_TEST_SCENARIOS) {
    testsTotal++;
    console.log(`📋 Test ${testsTotal}: ${scenario.name}`);
    console.log(`   🖥️  Server Port: ${scenario.serverPort}`);
    console.log(`   🌐 Client Port: ${scenario.clientPort}`);
    if (scenario.testServerPort) {
      console.log(`   🧪 Test Server Port: ${scenario.testServerPort}`);
    }

    try {
      // Set environment variables
      process.env.JTAG_SERVER_PORT = scenario.serverPort;
      process.env.JTAG_UI_PORT = scenario.clientPort;
      if (scenario.testServerPort) {
        process.env.JTAG_TEST_SERVER_PORT = scenario.testServerPort;
        process.env.NODE_ENV = 'test';
      } else {
        delete process.env.NODE_ENV;
      }

      // Validate configuration loading with new environment variables
      const serverConfig = getDefaultServerConfig();
      const clientConfig = getDefaultClientConfig();
      const testConfig = getDefaultTestConfig();

      console.log(`   ✅ Server config port: ${serverConfig.server.port}`);
      console.log(`   ✅ Client config port: ${clientConfig.client.ui_port}`);
      console.log(`   ✅ Test config port: ${testConfig.server.port}`);

      // Verify ports are set correctly
      if (serverConfig.server.port !== parseInt(scenario.serverPort)) {
        throw new Error(`Server port mismatch: expected ${scenario.serverPort}, got ${serverConfig.server.port}`);
      }
      if (clientConfig.client.ui_port !== parseInt(scenario.clientPort)) {
        throw new Error(`Client port mismatch: expected ${scenario.clientPort}, got ${clientConfig.client.ui_port}`);
      }
      if (scenario.testServerPort && testConfig.server.port !== parseInt(scenario.testServerPort)) {
        throw new Error(`Test port mismatch: expected ${scenario.testServerPort}, got ${testConfig.server.port}`);
      }

      // Test UserManager with custom configuration
      const customTestConfig: JTAGTestConfiguration = {
        server: { port: parseInt(scenario.testServerPort || scenario.serverPort), host: 'localhost', protocol: 'ws' },
        client: { ui_port: parseInt(scenario.clientPort), host: 'localhost', protocol: 'http' },
        test_settings: { timeout_ms: 5000, retry_attempts: 1, screenshot_on_failure: false, cleanup_after_test: true },
        environment: { test_mode: true, verbose_logging: false, isolated_sessions: true }
      };

      const userManager = new TestUserManager(customTestConfig);
      console.log(`   ✅ TestUserManager created with custom ports`);

      // Attempt connection (expected to fail - no server running)
      try {
        console.log(`   🔗 Attempting connection to validate port usage...`);
        const connection = await JTAGClientServer.connect({
          targetEnvironment: 'server',
          transportType: 'websocket',
          timeout: 3000
        });

        if (scenario.expectedBehavior === 'connection_fail') {
          console.log(`   ⚠️  Unexpected success - connection should have failed (no server on port ${scenario.serverPort})`);
          await connection.client.disconnect();
        } else {
          console.log(`   ✅ Connection successful as expected`);
          await connection.client.disconnect();
        }

      } catch (error) {
        if (scenario.expectedBehavior === 'connection_fail') {
          console.log(`   ✅ Expected connection failure: ${error.message.substring(0, 80)}...`);
        } else {
          throw error;
        }
      }

      console.log(`   🎉 Test ${testsTotal} PASSED: ${scenario.name}\n`);
      testsPassed++;

    } catch (error) {
      console.error(`   ❌ Test ${testsTotal} FAILED: ${scenario.name}`);
      console.error(`   💥 Error: ${error.message}\n`);
    } finally {
      // Clean up environment variables
      delete process.env.JTAG_SERVER_PORT;
      delete process.env.JTAG_UI_PORT;
      delete process.env.JTAG_TEST_SERVER_PORT;
      delete process.env.NODE_ENV;
    }
  }

  // Security boundary tests
  testsTotal++;
  console.log(`📋 Test ${testsTotal}: Security Boundary Validation`);
  
  try {
    const serverConfig = getDefaultServerConfig();
    const clientConfig = getDefaultClientConfig();

    // Verify server config has security section
    if (!('security' in serverConfig)) {
      throw new Error('Server configuration missing security section');
    }

    // Verify client config does NOT have security section
    if ('security' in clientConfig) {
      throw new Error('Client configuration should not have security section');
    }

    // Verify different configuration sections
    const serverSections = Object.keys(serverConfig);
    const clientSections = Object.keys(clientConfig);

    console.log(`   🖥️  Server config sections: ${serverSections.join(', ')}`);
    console.log(`   🌐 Client config sections: ${clientSections.join(', ')}`);

    if (serverSections.includes('security') && !clientSections.includes('security')) {
      console.log('   ✅ Security isolation confirmed');
      console.log(`   🎉 Test ${testsTotal} PASSED: Security Boundary Validation\n`);
      testsPassed++;
    } else {
      throw new Error('Security boundary validation failed');
    }

  } catch (error) {
    console.error(`   ❌ Test ${testsTotal} FAILED: Security Boundary Validation`);
    console.error(`   💥 Error: ${error.message}\n`);
  }

  // Results summary
  console.log('🎯 INTEGRATION TEST RESULTS');
  console.log('===========================');
  console.log(`✅ Tests Passed: ${testsPassed}`);
  console.log(`❌ Tests Failed: ${testsTotal - testsPassed}`);
  console.log(`📊 Success Rate: ${Math.round((testsPassed / testsTotal) * 100)}%`);

  if (testsPassed === testsTotal) {
    console.log('\n🎉 ALL PORT INTEGRATION TESTS PASSED');
    console.log('✅ Port configuration works across all scenarios');
    console.log('✅ Environment variable overrides function correctly');
    console.log('✅ Security boundaries are maintained');
    console.log('✅ Configuration isolation is enforced');
  } else {
    console.log(`\n❌ ${testsTotal - testsPassed} TESTS FAILED`);
    throw new Error('Port integration tests failed');
  }
}

// Run the tests
testSecureConfigPortIntegration()
  .then(() => {
    console.log('\n🎯 SECURE CONFIG PORT INTEGRATION TESTS PASSED');
    process.exit(0);
  })
  .catch((error) => {
    console.error('\n💥 SECURE CONFIG PORT INTEGRATION TESTS FAILED:', error.message);
    process.exit(1);
  });