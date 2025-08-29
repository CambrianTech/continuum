/**
 * Multi-Instance Test Configuration Types
 * 
 * Strongly typed configuration for testing multiple Continuum instances
 * in parallel - essential for Grid P2P backbone integration tests.
 */

import type { ExamplePortConfiguration } from '../../../examples/shared/ExampleConfigTypes';

// Multi-instance test configuration
export interface MultiInstanceTestConfig {
  readonly instances: readonly ContinuumInstanceConfig[];
  readonly testSuite: MultiInstanceTestSuite;
  readonly coordination: InstanceCoordinationConfig;
  readonly cleanup: CleanupConfig;
}

// Individual Continuum instance configuration  
export interface ContinuumInstanceConfig {
  readonly instanceId: string;              // Unique identifier for this instance
  readonly workingDir: string;              // examples/widget-ui, examples/test-bench, etc.
  readonly ports: ExamplePortConfiguration; // Port configuration from examples.json
  readonly environment: InstanceEnvironment; // Additional environment variables
  readonly features: InstanceFeatures;      // Feature flags for this instance
  readonly persona?: PersonaConfig;         // AI persona for this instance (future Grid feature)
}

// Environment configuration for each instance
export interface InstanceEnvironment {
  readonly JTAG_ACTIVE_EXAMPLE: string;
  readonly JTAG_INSTANCE_ID?: string;       // For P2P identification
  readonly JTAG_VERBOSE?: boolean;
  readonly NODE_ENV?: 'development' | 'test' | 'production';
  readonly [key: string]: string | boolean | number | undefined;
}

// Feature toggles for each instance
export interface InstanceFeatures {
  readonly enableBrowser: boolean;          // Launch browser for this instance
  readonly enableP2PDiscovery: boolean;     // Enable UDP multicast discovery  
  readonly enableGridRouting: boolean;      // Enable Grid routing backbone
  readonly enableCrossInstanceMessaging: boolean; // Enable inter-instance communication
  readonly enablePersonaSystem: boolean;    // Enable AI persona (future)
  readonly timeoutMs: number;              // Instance startup timeout
}

// Test suite configuration for multi-instance tests
export interface MultiInstanceTestSuite {
  readonly name: string;
  readonly profile: 'grid-p2p' | 'load-testing' | 'integration' | 'comprehensive';
  readonly tests: readonly MultiInstanceTest[];
  readonly globalTimeout: number;
  readonly parallelExecution: boolean;
  readonly requireAllInstancesHealthy: boolean;
}

// Individual test definition
export interface MultiInstanceTest {
  readonly name: string;
  readonly category: TestCategory;
  readonly requiredInstances: readonly string[]; // Instance IDs that must be running
  readonly testFunction: string;                 // Path to test file
  readonly timeout: number;
  readonly retryAttempts: number;
  readonly dependencies?: readonly string[];     // Other tests that must pass first
}

// Test categories for organization
export type TestCategory = 
  | 'p2p-discovery'
  | 'grid-routing' 
  | 'cross-instance-messaging'
  | 'load-balancing'
  | 'failover'
  | 'persona-coordination'
  | 'distributed-commands';

// Coordination configuration between instances
export interface InstanceCoordinationConfig {
  readonly startupSequence: 'parallel' | 'sequential' | 'staggered';
  readonly healthCheckInterval: number;
  readonly crossInstancePingTimeout: number;
  readonly coordinatorInstance?: string;  // Primary coordinator instance ID
  readonly sharedResources: SharedResourceConfig;
}

// Shared resources between instances
export interface SharedResourceConfig {
  readonly sharedDatabase?: string;       // Path to shared SQLite database
  readonly sharedFilesystem?: string;     // Shared directory for file exchange
  readonly messageQueue?: string;         // Inter-instance message queue
  readonly lockDirectory?: string;        // Directory for coordination locks
}

// Cleanup configuration
export interface CleanupConfig {
  readonly cleanupOnFailure: boolean;
  readonly cleanupOnSuccess: boolean;
  readonly preserveLogs: boolean;
  readonly preserveScreenshots: boolean;
  readonly killTimeout: number; // Time to wait for graceful shutdown
}

// Future: AI Persona configuration for Grid testing
export interface PersonaConfig {
  readonly name: string;
  readonly model: 'openai' | 'deepseek' | 'anthropic';
  readonly specialization: 'general' | 'coding' | 'testing' | 'monitoring';
  readonly personality: string; // Personality prompt
  readonly capabilities: readonly string[]; // Available commands/functions
}

// Test execution results
export interface MultiInstanceTestResult {
  readonly configId: string;
  readonly startTime: string;
  endTime: string;              // Mutable - updated during execution
  duration: number;             // Mutable - updated during execution
  success: boolean;             // Mutable - updated during execution
  readonly instanceResults: Record<string, InstanceTestResult>;
  readonly testResults: Record<string, TestExecutionResult>;
  errors: string[];             // Mutable - errors added during execution
  warnings: string[];           // Mutable - warnings added during execution
  metrics: TestMetrics;         // Mutable - metrics updated during execution
}

// Results for individual instance
export interface InstanceTestResult {
  readonly instanceId: string;
  healthy: boolean;                                                    // Mutable
  startupTime: number;                                                 // Mutable
  finalStatus: 'healthy' | 'degraded' | 'unhealthy' | 'error';       // Mutable
  ports: number[];                                                     // Mutable
  readonly tmuxSession: string;
  readonly processId: number;
  errors: string[];                                                    // Mutable
}

// Results for individual test
export interface TestExecutionResult {
  readonly testName: string;
  success: boolean;             // Mutable
  duration: number;             // Mutable
  error?: string;               // Mutable
  metrics?: Record<string, number>; // Mutable
}

// Test metrics
export interface TestMetrics {
  readonly totalInstances: number;
  healthyInstances: number;     // Mutable
  readonly totalTests: number;
  passedTests: number;          // Mutable
  averageStartupTime: number;   // Mutable
  peakMemoryUsage: number;      // Mutable
  messageLatency?: number;      // Mutable - For P2P tests
  throughput?: number;          // Mutable - Messages/second
}

// Pre-defined test configurations for common scenarios
export const GRID_P2P_TEST_CONFIG: MultiInstanceTestConfig = {
  instances: [
    {
      instanceId: 'node-alpha',
      workingDir: 'examples/test-bench',
      ports: { websocket_server: 9001, http_server: 9002 },
      environment: { JTAG_ACTIVE_EXAMPLE: 'test-bench' },
      features: {
        enableBrowser: false,
        enableP2PDiscovery: true,
        enableGridRouting: true,
        enableCrossInstanceMessaging: true,
        enablePersonaSystem: false,
        timeoutMs: 60000
      }
    },
    {
      instanceId: 'node-beta',
      workingDir: 'examples/widget-ui',
      ports: { websocket_server: 9101, http_server: 9102 },
      environment: { JTAG_ACTIVE_EXAMPLE: 'widget-ui' },
      features: {
        enableBrowser: true,
        enableP2PDiscovery: true,
        enableGridRouting: true,
        enableCrossInstanceMessaging: true,
        enablePersonaSystem: false,
        timeoutMs: 60000
      }
    },
    {
      instanceId: 'node-gamma',
      workingDir: 'examples/test-bench',
      ports: { websocket_server: 9201, http_server: 9202 },
      environment: { JTAG_ACTIVE_EXAMPLE: 'test-bench' },
      features: {
        enableBrowser: false,
        enableP2PDiscovery: true,
        enableGridRouting: true,
        enableCrossInstanceMessaging: true,
        enablePersonaSystem: false,
        timeoutMs: 60000
      }
    }
  ],
  testSuite: {
    name: 'Grid P2P Backbone Integration',
    profile: 'grid-p2p',
    tests: [
      {
        name: 'UDP Multicast Discovery',
        category: 'p2p-discovery',
        requiredInstances: ['node-alpha', 'node-beta', 'node-gamma'],
        testFunction: 'tests/grid/udp-multicast-discovery.test.ts',
        timeout: 30000,
        retryAttempts: 3
      },
      {
        name: 'Grid Routing Table Formation',
        category: 'grid-routing',
        requiredInstances: ['node-alpha', 'node-beta', 'node-gamma'],
        testFunction: 'tests/grid/routing-table-formation.test.ts',
        timeout: 45000,
        retryAttempts: 2,
        dependencies: ['UDP Multicast Discovery']
      },
      {
        name: 'Cross-Instance Command Execution',
        category: 'distributed-commands',
        requiredInstances: ['node-alpha', 'node-beta'],
        testFunction: 'tests/grid/cross-instance-commands.test.ts',
        timeout: 30000,
        retryAttempts: 3,
        dependencies: ['Grid Routing Table Formation']
      }
    ],
    globalTimeout: 300000, // 5 minutes total
    parallelExecution: true,
    requireAllInstancesHealthy: true
  },
  coordination: {
    startupSequence: 'staggered',
    healthCheckInterval: 5000,
    crossInstancePingTimeout: 10000,
    coordinatorInstance: 'node-alpha',
    sharedResources: {
      lockDirectory: '.continuum/multi-instance/locks',
      messageQueue: '.continuum/multi-instance/messages'
    }
  },
  cleanup: {
    cleanupOnFailure: true,
    cleanupOnSuccess: false,
    preserveLogs: true,
    preserveScreenshots: true,
    killTimeout: 10000
  }
} as const;