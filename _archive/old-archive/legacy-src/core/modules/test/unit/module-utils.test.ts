/**
 * Core Module Utils - Unit Tests
 * 
 * Tests the module utility functions used across:
 * - Academy Integration (dependency management)
 * - Testing systems (mock creation)
 * - Build and validation systems
 */

import { strict as assert } from 'assert';
import { describe, it } from 'node:test';
import { ModuleUtils } from '../../utils.js';
import type { ModuleDependency } from '../../discovery.js';

describe('ModuleUtils', () => {
  const mockDependencies = {
    daemon1: {
      name: 'daemon1',
      type: 'daemon' as const,
      required: true,
      healthCheck: 'get_capabilities',
      config: { port: 3000, host: 'localhost' }
    },
    daemon2: {
      name: 'daemon2', 
      type: 'daemon' as const,
      required: false,
      healthCheck: 'ping',
      config: { timeout: 5000 }
    },
    command1: {
      name: 'command1',
      type: 'command' as const,
      required: true,
      config: {}
    }
  } as const satisfies Record<string, ModuleDependency>;

  describe('Mock Instance Creation', () => {
    it('should create basic mock instances for all dependencies', () => {
      const mocks = ModuleUtils.createMockInstances(mockDependencies);
      
      assert.ok(typeof mocks === 'object');
      assert.ok('daemon1' in mocks);
      assert.ok('daemon2' in mocks);
      assert.ok('command1' in mocks);
      
      // Check mock structure
      assert.equal(mocks.daemon1.name, 'daemon1');
      assert.equal(mocks.daemon1.type, 'daemon');
      assert.equal(typeof mocks.daemon1.start, 'function');
      assert.equal(typeof mocks.daemon1.stop, 'function');
      assert.equal(typeof mocks.daemon1.sendMessage, 'function');
    });

    it('should apply overrides using spread pattern', () => {
      const overrides = {
        daemon1: {
          customMethod: () => 'custom',
          start: async () => { throw new Error('Custom start error'); }
        },
        daemon2: {
          port: 4000
        }
      };

      const mocks = ModuleUtils.createMockInstances(mockDependencies, overrides);
      
      // Check overrides applied
      assert.equal(typeof mocks.daemon1.customMethod, 'function');
      assert.equal(mocks.daemon1.customMethod(), 'custom');
      assert.equal(mocks.daemon2.port, 4000);
      
      // Original properties should still exist
      assert.equal(mocks.daemon1.name, 'daemon1');
      assert.equal(typeof mocks.daemon1.stop, 'function');
    });

    it('should handle async mock methods correctly', async () => {
      const mocks = ModuleUtils.createMockInstances(mockDependencies);
      
      // Default mocks should return successful responses
      const response = await mocks.daemon1.sendMessage();
      assert.deepEqual(response, { success: true, data: {} });
      
      // Start/stop should complete without errors
      await mocks.daemon1.start();
      await mocks.daemon1.stop();
    });
  });

  describe('Startup and Shutdown Order Calculation', () => {
    it('should calculate startup order with required dependencies first', () => {
      const order = ModuleUtils.calculateStartupOrder(mockDependencies);
      
      assert.ok(Array.isArray(order));
      assert.ok(order.includes('daemon1'));
      assert.ok(order.includes('daemon2'));
      assert.ok(order.includes('command1'));
      
      // Required dependencies should come before optional ones
      const daemon1Index = order.indexOf('daemon1');
      const command1Index = order.indexOf('command1');
      const daemon2Index = order.indexOf('daemon2');
      
      assert.ok(daemon1Index < daemon2Index, 'Required daemon1 should come before optional daemon2');
      assert.ok(command1Index < daemon2Index, 'Required command1 should come before optional daemon2');
    });

    it('should calculate shutdown order as reverse of startup', () => {
      const startupOrder = ModuleUtils.calculateStartupOrder(mockDependencies);
      const shutdownOrder = ModuleUtils.calculateShutdownOrder(mockDependencies);
      
      assert.deepEqual(shutdownOrder, startupOrder.reverse());
    });
  });

  describe('Dependency Filtering', () => {
    it('should filter dependencies by type', () => {
      const daemonDeps = ModuleUtils.filterByType(mockDependencies, 'daemon');
      const commandDeps = ModuleUtils.filterByType(mockDependencies, 'command');
      
      assert.equal(Object.keys(daemonDeps).length, 2);
      assert.ok('daemon1' in daemonDeps);
      assert.ok('daemon2' in daemonDeps);
      
      assert.equal(Object.keys(commandDeps).length, 1);
      assert.ok('command1' in commandDeps);
    });

    it('should get required dependencies only', () => {
      const required = ModuleUtils.getRequired(mockDependencies);
      
      assert.equal(Object.keys(required).length, 2);
      assert.ok('daemon1' in required);
      assert.ok('command1' in required);
      assert.ok(!('daemon2' in required));
    });

    it('should get optional dependencies only', () => {
      const optional = ModuleUtils.getOptional(mockDependencies);
      
      assert.equal(Object.keys(optional).length, 1);
      assert.ok('daemon2' in optional);
      assert.ok(!('daemon1' in optional));
      assert.ok(!('command1' in optional));
    });
  });

  describe('Configuration Merging', () => {
    it('should merge dependency configurations with overrides', () => {
      const overrides = {
        daemon1: { port: 4000, newSetting: 'test' },
        daemon2: { timeout: 10000 }
      };

      const merged = ModuleUtils.mergeConfigs(mockDependencies, overrides);
      
      assert.equal(merged.daemon1.port, 4000); // Overridden
      assert.equal(merged.daemon1.host, 'localhost'); // Original preserved  
      assert.equal(merged.daemon1.newSetting, 'test'); // New setting added
      
      assert.equal(merged.daemon2.timeout, 10000); // Overridden
      
      assert.deepEqual(merged.command1, {}); // No config or overrides
    });

    it('should handle empty configurations gracefully', () => {
      const singleDep = {
        test: {
          name: 'test',
          type: 'daemon' as const,
          required: true,
          config: {}
        }
      };

      const merged = ModuleUtils.mergeConfigs(singleDep, {});
      assert.deepEqual(merged.test, {});
    });
  });

  describe('Dependency Validation', () => {
    it('should validate correct dependency structures', () => {
      const result = ModuleUtils.validateDependencies(mockDependencies);
      
      assert.equal(result.valid, true);
      assert.equal(result.errors.length, 0);
    });

    it('should detect missing required fields', () => {
      const invalidDeps = {
        invalid1: {
          // Missing name
          type: 'daemon' as const,
          required: true
        },
        invalid2: {
          name: 'invalid2',
          // Missing type
          required: true
        },
        invalid3: {
          name: 'invalid3',
          type: 'daemon' as const
          // Missing required field
        }
      } as any;

      const result = ModuleUtils.validateDependencies(invalidDeps);
      
      assert.equal(result.valid, false);
      assert.ok(result.errors.length > 0);
      
      const errorMessages = result.errors.join(' ');
      assert.ok(errorMessages.includes('missing name field'));
      assert.ok(errorMessages.includes('missing type field'));
      assert.ok(errorMessages.includes('missing or invalid required field'));
    });

    it('should handle empty dependencies object', () => {
      const result = ModuleUtils.validateDependencies({});
      
      assert.equal(result.valid, true);
      assert.equal(result.errors.length, 0);
    });
  });

  describe('Edge Cases and Error Handling', () => {
    it('should handle dependencies with no configuration', () => {
      const noCofigDeps = {
        simple: {
          name: 'simple',
          type: 'command' as const,
          required: true
        }
      } as any;

      const mocks = ModuleUtils.createMockInstances(noCofigDeps);
      assert.ok(mocks.simple);
      assert.equal(mocks.simple.name, 'simple');
    });

    it('should preserve type safety with TypeScript patterns', () => {
      // This test validates TypeScript compile-time behavior
      const order = ModuleUtils.calculateStartupOrder(mockDependencies);
      const mocks = ModuleUtils.createMockInstances(mockDependencies);
      
      // These should be typed correctly
      const daemon1Mock = mocks.daemon1;
      const daemon2Mock = mocks.daemon2;
      const command1Mock = mocks.command1;
      
      assert.ok(daemon1Mock);
      assert.ok(daemon2Mock);
      assert.ok(command1Mock);
      assert.ok(order.includes('daemon1'));
    });
  });
});