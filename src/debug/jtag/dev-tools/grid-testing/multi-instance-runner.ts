#!/usr/bin/env tsx
/**
 * Parameterized Multi-Instance Test Runner
 * 
 * Enables npm test to launch multiple Continuum instances for Grid P2P testing.
 * Usage:
 *   npm run test:multi-instance
 *   npm run test:multi-instance -- --config=grid-p2p
 *   npm run test:multi-instance -- --instances=test-bench,widget-ui --ports=9001,9101
 */

import { readFileSync } from 'fs';
import { join } from 'path';
import type { 
  MultiInstanceTestConfig,
  ContinuumInstanceConfig,
  InstanceEnvironment,
  InstanceFeatures
} from './multi-instance/MultiInstanceTestTypes';
import { MultiInstanceTestRunner } from './multi-instance/MultiInstanceTestRunner';
import type { ExamplePortConfiguration } from '../../examples/shared/ExampleConfigTypes';

// Command line argument parsing
interface TestArgs {
  config?: string;           // Pre-defined config name
  instances?: string[];      // List of example names to test  
  ports?: number[];          // Base port numbers for instances
  profile?: 'grid-p2p' | 'integration' | 'load-testing' | 'comprehensive';
  timeout?: number;          // Global timeout in seconds
  parallel?: boolean;        // Run instances in parallel
  cleanup?: boolean;         // Cleanup after tests
  verbose?: boolean;         // Verbose output
}

function parseArguments(): TestArgs {
  const args = process.argv.slice(2);
  const parsed: TestArgs = {
    parallel: true,
    cleanup: true,
    verbose: false,
    timeout: 300 // 5 minutes default
  };

  for (const arg of args) {
    if (arg.startsWith('--config=')) {
      parsed.config = arg.split('=')[1];
    } else if (arg.startsWith('--instances=')) {
      parsed.instances = arg.split('=')[1].split(',');
    } else if (arg.startsWith('--ports=')) {
      parsed.ports = arg.split('=')[1].split(',').map(p => parseInt(p));
    } else if (arg.startsWith('--profile=')) {
      parsed.profile = arg.split('=')[1] as any;
    } else if (arg.startsWith('--timeout=')) {
      parsed.timeout = parseInt(arg.split('=')[1]);
    } else if (arg === '--sequential') {
      parsed.parallel = false;
    } else if (arg === '--no-cleanup') {
      parsed.cleanup = false;
    } else if (arg === '--verbose') {
      parsed.verbose = true;
    }
  }

  return parsed;
}

/**
 * Load pre-defined configuration by name
 */
async function loadPreDefinedConfig(name: string): Promise<MultiInstanceTestConfig> {
  switch (name) {
    case 'grid-p2p':
      const { GRID_P2P_TEST_CONFIG } = await import('./multi-instance/MultiInstanceTestTypes');
      return GRID_P2P_TEST_CONFIG;
      
    default:
      throw new Error(`Unknown pre-defined configuration: ${name}`);
  }
}

/**
 * Generate dynamic configuration from command line arguments
 */
function generateDynamicConfig(args: TestArgs): MultiInstanceTestConfig {
  if (!args.instances || args.instances.length === 0) {
    throw new Error('Must specify --instances when not using --config');
  }

  const basePorts = args.ports || [9001];
  const instances: ContinuumInstanceConfig[] = [];

  // Generate instance configurations
  for (let i = 0; i < args.instances.length; i++) {
    const exampleName = args.instances[i];
    const basePort = basePorts[i % basePorts.length];
    const portOffset = i * 100;

    const instanceConfig: ContinuumInstanceConfig = {
      instanceId: `${exampleName}-${i}`,
      workingDir: `examples/${exampleName}`,
      ports: {
        websocket_server: basePort + portOffset,
        http_server: basePort + portOffset + 1
      },
      environment: {
        JTAG_ACTIVE_EXAMPLE: exampleName,
        NODE_ENV: 'test',
        JTAG_VERBOSE: args.verbose
      },
      features: {
        enableBrowser: exampleName === 'widget-ui', // Only widget-ui needs browser
        enableP2PDiscovery: true,
        enableGridRouting: true,
        enableCrossInstanceMessaging: true,
        enablePersonaSystem: false,
        timeoutMs: (args.timeout || 300) * 1000
      }
    };

    instances.push(instanceConfig);
  }

  // Build the complete test configuration
  const config: MultiInstanceTestConfig = {
    instances,
    testSuite: {
      name: `Dynamic Multi-Instance Test (${args.instances.join(', ')})`,
      profile: args.profile || 'integration',
      tests: [
        {
          name: 'Instance Health Check',
          category: 'p2p-discovery',
          requiredInstances: instances.map(i => i.instanceId),
          testFunction: 'tests/multi-instance/health-check.test.ts',
          timeout: 30000,
          retryAttempts: 3
        }
      ],
      globalTimeout: args.timeout! * 1000,
      parallelExecution: args.parallel!,
      requireAllInstancesHealthy: true
    },
    coordination: {
      startupSequence: args.parallel ? 'parallel' : 'sequential',
      healthCheckInterval: 5000,
      crossInstancePingTimeout: 10000,
      sharedResources: {
        lockDirectory: '.continuum/multi-instance/locks'
      }
    },
    cleanup: {
      cleanupOnFailure: args.cleanup!,
      cleanupOnSuccess: args.cleanup!,
      preserveLogs: true,
      preserveScreenshots: true,
      killTimeout: 10000
    }
  };

  return config;
}

/**
 * Display configuration summary before execution
 */
function displayConfigSummary(config: MultiInstanceTestConfig): void {
  console.log(`🌐 MULTI-INSTANCE CONTINUUM TEST CONFIGURATION`);
  console.log(`═══════════════════════════════════════════════`);
  console.log(`📋 Test Suite: ${config.testSuite.name}`);
  console.log(`🎯 Profile: ${config.testSuite.profile}`);
  console.log(`🔢 Instances: ${config.instances.length}`);
  console.log(`🧪 Tests: ${config.testSuite.tests.length}`);
  console.log(`⏱️  Timeout: ${config.testSuite.globalTimeout / 1000}s`);
  console.log(`🔄 Coordination: ${config.coordination.startupSequence}`);
  console.log(``);
  
  console.log(`📊 INSTANCE CONFIGURATIONS:`);
  for (const instance of config.instances) {
    console.log(`   🖥️  ${instance.instanceId}:`);
    console.log(`      📂 Working Dir: ${instance.workingDir}`);
    console.log(`      🌐 Ports: WS=${instance.ports.websocket_server}, HTTP=${instance.ports.http_server}`);
    console.log(`      🌟 Features: Browser=${instance.features.enableBrowser}, P2P=${instance.features.enableP2PDiscovery}`);
  }
  console.log(``);
}

/**
 * Main execution function
 */
async function main(): Promise<void> {
  try {
    const args = parseArguments();
    
    // Load or generate configuration
    let config: MultiInstanceTestConfig;
    if (args.config) {
      console.log(`📋 Loading pre-defined configuration: ${args.config}`);
      config = await loadPreDefinedConfig(args.config);
    } else {
      console.log(`🔧 Generating dynamic configuration from arguments`);
      config = generateDynamicConfig(args);
    }
    
    // Display configuration summary
    displayConfigSummary(config);
    
    // Confirm execution
    if (!args.verbose) {
      console.log(`⚠️  Add --verbose for detailed output`);
    }
    console.log(`🚀 Starting multi-instance test execution...`);
    console.log(``);
    
    // Execute the test suite
    const runner = new MultiInstanceTestRunner(config);
    const results = await runner.runTestSuite();
    
    // Display results summary
    console.log(``);
    console.log(`📊 MULTI-INSTANCE TEST RESULTS`);
    console.log(`═══════════════════════════════════════════`);
    console.log(`✅ Success: ${results.success ? 'YES' : 'NO'}`);
    console.log(`⏱️  Duration: ${(results.duration / 1000).toFixed(1)}s`);
    console.log(`🖥️  Healthy Instances: ${results.metrics.healthyInstances}/${results.metrics.totalInstances}`);
    console.log(`🧪 Passed Tests: ${results.metrics.passedTests}/${results.metrics.totalTests}`);
    console.log(`⚡ Avg Startup Time: ${results.metrics.averageStartupTime.toFixed(0)}ms`);
    
    if (results.errors.length > 0) {
      console.log(``);
      console.log(`❌ ERRORS:`);
      results.errors.forEach(error => console.log(`   • ${error}`));
    }
    
    if (results.warnings.length > 0) {
      console.log(``);
      console.log(`⚠️  WARNINGS:`);
      results.warnings.forEach(warning => console.log(`   • ${warning}`));
    }
    
    // Exit with appropriate code
    process.exit(results.success ? 0 : 1);
    
  } catch (error) {
    console.error(`💥 Fatal error in multi-instance test runner:`, error);
    process.exit(1);
  }
}

// Help text
function displayHelp(): void {
  console.log(`
🌐 MULTI-INSTANCE CONTINUUM TEST RUNNER

USAGE:
  npm run test:multi-instance [options]

PREDEFINED CONFIGURATIONS:
  npm run test:multi-instance -- --config=grid-p2p
    Launches 3-node Grid P2P backbone test suite

DYNAMIC CONFIGURATIONS:  
  npm run test:multi-instance -- --instances=test-bench,widget-ui
    Launch test-bench and widget-ui instances
    
  npm run test:multi-instance -- --instances=test-bench,widget-ui --ports=9001,9201
    Launch with specific base ports (auto-offset for each instance)

OPTIONS:
  --config=NAME          Use predefined configuration (grid-p2p)
  --instances=LIST       Comma-separated list of examples to test
  --ports=LIST           Base port numbers (default: 9001)
  --profile=TYPE         Test profile (grid-p2p, integration, load-testing, comprehensive)
  --timeout=SECONDS      Global timeout in seconds (default: 300)
  --sequential           Start instances sequentially instead of parallel
  --no-cleanup           Leave instances running after tests
  --verbose              Enable verbose output

EXAMPLES:
  # Grid P2P backbone test with 3 nodes
  npm run test:multi-instance -- --config=grid-p2p --verbose
  
  # Test two different examples with custom ports
  npm run test:multi-instance -- --instances=test-bench,widget-ui --ports=9001,9101
  
  # Load testing with 4 instances
  npm run test:multi-instance -- --instances=test-bench,test-bench,widget-ui,test-bench --profile=load-testing
  
  # Sequential startup for debugging
  npm run test:multi-instance -- --instances=test-bench,widget-ui --sequential --no-cleanup --verbose

GRID P2P TESTING:
  The multi-instance system is designed to test the Grid P2P backbone that enables
  distributed AI persona coordination across multiple Continuum nodes. Each instance
  runs its own set of commands, personas, and can participate in mesh networking.
`);
}

// Handle help
if (process.argv.includes('--help') || process.argv.includes('-h')) {
  displayHelp();
  process.exit(0);
}

// Execute main function
main();