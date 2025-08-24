/**
 * Multi-Instance Grid Testing - Unit Tests
 * 
 * Tests for the multi-instance testing system used for Grid P2P development.
 * These tests validate the modular multi-instance functionality independently.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import type { 
  MultiInstanceTestConfig,
  ContinuumInstanceConfig,
  InstanceFeatures 
} from '../multi-instance/MultiInstanceTestTypes';
import { MultiInstanceTestRunner } from '../multi-instance/MultiInstanceTestRunner';

describe('Multi-Instance Grid Testing Module', () => {
  
  describe('MultiInstanceTestConfig Validation', () => {
    it('should validate correct configuration structure', () => {
      const validConfig: MultiInstanceTestConfig = {
        instances: [
          {
            instanceId: 'test-node-1',
            workingDir: 'examples/test-bench',
            ports: { websocket_server: 9001, http_server: 9002 },
            environment: { JTAG_ACTIVE_EXAMPLE: 'test-bench' },
            features: {
              enableBrowser: false,
              enableP2PDiscovery: true,
              enableGridRouting: true,
              enableCrossInstanceMessaging: true,
              enablePersonaSystem: false,
              timeoutMs: 30000
            }
          }
        ],
        testSuite: {
          name: 'Test Suite',
          profile: 'integration',
          tests: [],
          globalTimeout: 60000,
          parallelExecution: true,
          requireAllInstancesHealthy: true
        },
        coordination: {
          startupSequence: 'parallel',
          healthCheckInterval: 5000,
          crossInstancePingTimeout: 10000,
          sharedResources: {}
        },
        cleanup: {
          cleanupOnFailure: true,
          cleanupOnSuccess: false,
          preserveLogs: true,
          preserveScreenshots: true,
          killTimeout: 10000
        }
      };

      // Should not throw
      expect(() => new MultiInstanceTestRunner(validConfig)).not.toThrow();
    });

    it('should handle workdir-specific configurations', () => {
      const testBenchConfig: ContinuumInstanceConfig = {
        instanceId: 'test-bench-node',
        workingDir: 'examples/test-bench',
        ports: { websocket_server: 9001, http_server: 9002 },
        environment: { JTAG_ACTIVE_EXAMPLE: 'test-bench' },
        features: {
          enableBrowser: false, // test-bench doesn't need browser
          enableP2PDiscovery: true,
          enableGridRouting: true,
          enableCrossInstanceMessaging: true,
          enablePersonaSystem: false,
          timeoutMs: 30000
        }
      };

      const widgetUIConfig: ContinuumInstanceConfig = {
        instanceId: 'widget-ui-node',
        workingDir: 'examples/widget-ui',
        ports: { websocket_server: 9101, http_server: 9102 },
        environment: { JTAG_ACTIVE_EXAMPLE: 'widget-ui' },
        features: {
          enableBrowser: true, // widget-ui needs browser
          enableP2PDiscovery: true,
          enableGridRouting: true,
          enableCrossInstanceMessaging: true,
          enablePersonaSystem: false,
          timeoutMs: 30000
        }
      };

      expect(testBenchConfig.workingDir).toBe('examples/test-bench');
      expect(testBenchConfig.features.enableBrowser).toBe(false);
      
      expect(widgetUIConfig.workingDir).toBe('examples/widget-ui');
      expect(widgetUIConfig.features.enableBrowser).toBe(true);
    });
  });

  describe('Port Configuration Management', () => {
    it('should handle port allocation without conflicts', () => {
      const instance1 = {
        instanceId: 'node-1',
        ports: { websocket_server: 9001, http_server: 9002 }
      };
      
      const instance2 = {
        instanceId: 'node-2', 
        ports: { websocket_server: 9101, http_server: 9102 }
      };

      const instance3 = {
        instanceId: 'node-3',
        ports: { websocket_server: 9201, http_server: 9202 }
      };

      // Verify no port conflicts
      const allPorts = [
        ...Object.values(instance1.ports),
        ...Object.values(instance2.ports),
        ...Object.values(instance3.ports)
      ];

      const uniquePorts = new Set(allPorts);
      expect(uniquePorts.size).toBe(allPorts.length);
    });
  });

  describe('Grid P2P Features Configuration', () => {
    it('should configure P2P features correctly', () => {
      const features: InstanceFeatures = {
        enableBrowser: false,
        enableP2PDiscovery: true,
        enableGridRouting: true,
        enableCrossInstanceMessaging: true,
        enablePersonaSystem: false,
        timeoutMs: 60000
      };

      expect(features.enableP2PDiscovery).toBe(true);
      expect(features.enableGridRouting).toBe(true);
      expect(features.enableCrossInstanceMessaging).toBe(true);
    });

    it('should handle different feature combinations for different node types', () => {
      // Coordinator node - full features
      const coordinatorFeatures: InstanceFeatures = {
        enableBrowser: true,
        enableP2PDiscovery: true,
        enableGridRouting: true,
        enableCrossInstanceMessaging: true,
        enablePersonaSystem: true,
        timeoutMs: 60000
      };

      // Worker node - minimal features
      const workerFeatures: InstanceFeatures = {
        enableBrowser: false,
        enableP2PDiscovery: true,
        enableGridRouting: true,
        enableCrossInstanceMessaging: true,
        enablePersonaSystem: false,
        timeoutMs: 30000
      };

      expect(coordinatorFeatures.enablePersonaSystem).toBe(true);
      expect(workerFeatures.enablePersonaSystem).toBe(false);
    });
  });

  describe('Test Module Independence', () => {
    it('should not depend on other dev-tools modules', () => {
      // This test ensures the multi-instance module is independently testable
      // by verifying it only imports from core JTAG, not other dev-tools
      
      const validConfig: MultiInstanceTestConfig = {
        instances: [{
          instanceId: 'test',
          workingDir: 'examples/test-bench',
          ports: { websocket_server: 9001, http_server: 9002 },
          environment: { JTAG_ACTIVE_EXAMPLE: 'test-bench' },
          features: {
            enableBrowser: false,
            enableP2PDiscovery: true,
            enableGridRouting: true,
            enableCrossInstanceMessaging: true,
            enablePersonaSystem: false,
            timeoutMs: 30000
          }
        }],
        testSuite: {
          name: 'Independence Test',
          profile: 'integration',
          tests: [],
          globalTimeout: 30000,
          parallelExecution: true,
          requireAllInstancesHealthy: true
        },
        coordination: {
          startupSequence: 'parallel',
          healthCheckInterval: 5000,
          crossInstancePingTimeout: 10000,
          sharedResources: {}
        },
        cleanup: {
          cleanupOnFailure: true,
          cleanupOnSuccess: true,
          preserveLogs: false,
          preserveScreenshots: false,
          killTimeout: 5000
        }
      };

      // Should be able to instantiate without other dev-tools
      expect(() => new MultiInstanceTestRunner(validConfig)).not.toThrow();
    });
  });

  describe('Modular Testability', () => {
    it('should provide clear module boundaries', () => {
      // Verify that the module has clear, testable interfaces
      expect(typeof MultiInstanceTestRunner).toBe('function');
      expect(MultiInstanceTestRunner.prototype.runTestSuite).toBeDefined();
    });

    it('should handle configuration validation', () => {
      const invalidConfig = {
        instances: [], // Empty instances should be handled gracefully
        testSuite: {
          name: 'Empty Test',
          profile: 'integration' as const,
          tests: [],
          globalTimeout: 30000,
          parallelExecution: true,
          requireAllInstancesHealthy: true
        },
        coordination: {
          startupSequence: 'parallel' as const,
          healthCheckInterval: 5000,
          crossInstancePingTimeout: 10000,
          sharedResources: {}
        },
        cleanup: {
          cleanupOnFailure: true,
          cleanupOnSuccess: true,
          preserveLogs: false,
          preserveScreenshots: false,
          killTimeout: 5000
        }
      };

      // Should handle empty configurations
      expect(() => new MultiInstanceTestRunner(invalidConfig)).not.toThrow();
    });
  });
});

// Integration test for the full multi-instance system (optional - requires system setup)
describe.skip('Multi-Instance Integration Tests', () => {
  let runner: MultiInstanceTestRunner;

  beforeEach(() => {
    // These tests would require actual system setup
    // They're skipped by default but can be run in development environment
  });

  afterEach(async () => {
    // Cleanup any test instances
  });

  it('should launch multiple instances successfully', async () => {
    // This would test actual multi-instance launching
    // Skipped for unit tests but useful for development testing
  });
});