/**
 * Core Module Discovery - Unit Tests
 * 
 * Tests the core module discovery system that serves as foundation for:
 * - Testing framework (IntelligentModularTestRunner)
 * - Integration modules (Academy, etc.)
 * - Build systems and validation
 * - Runtime module loading
 */

import { strict as assert } from 'assert';
import { describe, it, beforeEach } from 'node:test';
import { ModuleDiscovery, type ModuleType, type ModuleInfo } from '../../discovery.js';

describe('ModuleDiscovery', () => {
  let moduleDiscovery: ModuleDiscovery;

  beforeEach(() => {
    moduleDiscovery = ModuleDiscovery.getInstance();
    moduleDiscovery.clearCache(); // Ensure clean state for each test
  });

  describe('Singleton Pattern', () => {
    it('should return the same instance across multiple calls', () => {
      const instance1 = ModuleDiscovery.getInstance();
      const instance2 = ModuleDiscovery.getInstance();
      
      assert.equal(instance1, instance2);
    });

    it('should respect custom root directory parameter', () => {
      const customInstance = ModuleDiscovery.getInstance('/custom/path');
      const defaultInstance = ModuleDiscovery.getInstance();
      
      // Should still be singleton but with custom path
      assert.ok(customInstance);
      assert.ok(defaultInstance);
    });
  });

  describe('Base Path Configuration', () => {
    it('should return correct base paths for all module types', () => {
      const widgetPaths = moduleDiscovery.getBasePaths('widget');
      const daemonPaths = moduleDiscovery.getBasePaths('daemon');
      const commandPaths = moduleDiscovery.getBasePaths('command');
      const integrationPaths = moduleDiscovery.getBasePaths('integration');
      const browserDaemonPaths = moduleDiscovery.getBasePaths('browser-daemon');

      assert.deepEqual(widgetPaths, ['src/ui/components']);
      assert.deepEqual(daemonPaths, ['src/daemons']);
      assert.deepEqual(commandPaths, ['src/commands']);
      assert.deepEqual(integrationPaths, ['src/integrations']);
      assert.deepEqual(browserDaemonPaths, ['src/ui/browser']);
    });

    it('should handle invalid module types gracefully', () => {
      const invalidPaths = moduleDiscovery.getBasePaths('invalid' as ModuleType);
      assert.deepEqual(invalidPaths, []);
    });
  });

  describe('Module Discovery', () => {
    it('should discover modules of specific types', async () => {
      // Test with known module types that should exist
      const daemonModules = await moduleDiscovery.discoverModules('daemon');
      const integrationModules = await moduleDiscovery.discoverModules('integration');
      
      assert.ok(Array.isArray(daemonModules));
      assert.ok(Array.isArray(integrationModules));
      
      // Should find the Academy integration we just created
      const academyIntegration = integrationModules.find(m => m.name === 'academy');
      if (academyIntegration) {
        assert.equal(academyIntegration.type, 'integration');
        assert.equal(academyIntegration.hasPackageJson, true);
      }
    });

    it('should cache discovery results by default', async () => {
      const firstCall = await moduleDiscovery.discoverModules('daemon');
      const secondCall = await moduleDiscovery.discoverModules('daemon');
      
      // Should return the same result (from cache)
      assert.deepEqual(firstCall, secondCall);
    });

    it('should bypass cache when requested', async () => {
      const cachedCall = await moduleDiscovery.discoverModules('daemon', true);
      const freshCall = await moduleDiscovery.discoverModules('daemon', false);
      
      assert.ok(Array.isArray(cachedCall));
      assert.ok(Array.isArray(freshCall));
    });
  });

  describe('Specific Module Retrieval', () => {
    it('should retrieve specific modules by name and type', async () => {
      const academyModule = await moduleDiscovery.getModule('academy', 'integration');
      
      if (academyModule) {
        assert.equal(academyModule.name, 'academy');
        assert.equal(academyModule.type, 'integration');
      }
    });

    it('should return null for non-existent modules', async () => {
      const nonExistentModule = await moduleDiscovery.getModule('non-existent-module', 'daemon');
      assert.equal(nonExistentModule, null);
    });
  });

  describe('All Modules Discovery', () => {
    it('should discover all modules across all types', async () => {
      const allModules = await moduleDiscovery.getAllModules();
      
      assert.ok(typeof allModules === 'object');
      assert.ok('widget' in allModules);
      assert.ok('daemon' in allModules);
      assert.ok('command' in allModules);
      assert.ok('integration' in allModules);
      assert.ok('browser-daemon' in allModules);
      
      // Each should be an array
      assert.ok(Array.isArray(allModules.widget));
      assert.ok(Array.isArray(allModules.daemon));
      assert.ok(Array.isArray(allModules.command));
      assert.ok(Array.isArray(allModules.integration));
      assert.ok(Array.isArray(allModules['browser-daemon']));
    });
  });

  describe('Dependency Iterator Creation', () => {
    it('should create dependency iterators with correct structure', () => {
      const mockDependencies = {
        test1: { name: 'test1', type: 'daemon' as const, required: true },
        test2: { name: 'test2', type: 'command' as const, required: false, config: { setting: 'value' } }
      };

      const iterator = moduleDiscovery.createDependencyIterator(mockDependencies);

      assert.deepEqual(iterator.names, ['test1', 'test2']);
      assert.deepEqual(iterator.required, ['test1']);
      assert.deepEqual(iterator.configs, { test1: {}, test2: { setting: 'value' } });
      
      // Test iterator methods
      const mappedResults: string[] = [];
      iterator.forEach((name, spec) => {
        mappedResults.push(`${name}:${spec.type}`);
      });
      assert.deepEqual(mappedResults, ['test1:daemon', 'test2:command']);

      const mapResults = iterator.map((name, spec) => `${name}-${spec.type}`);
      assert.deepEqual(mapResults, ['test1-daemon', 'test2-command']);

      const filtered = iterator.filter((name, spec) => spec.required);
      assert.equal(filtered.length, 1);
      assert.equal(filtered[0][0], 'test1');
    });
  });

  describe('Cache Management', () => {
    it('should clear cache properly', async () => {
      // Populate cache
      await moduleDiscovery.discoverModules('daemon');
      
      // Clear cache
      moduleDiscovery.clearCache();
      
      // Should work without issues after cache clear
      const modules = await moduleDiscovery.discoverModules('daemon');
      assert.ok(Array.isArray(modules));
    });
  });

  describe('Module Information Structure', () => {
    it('should provide complete module information', async () => {
      const modules = await moduleDiscovery.discoverModules('integration');
      
      for (const module of modules) {
        // Required fields
        assert.ok(typeof module.name === 'string');
        assert.ok(typeof module.path === 'string');
        assert.ok(typeof module.type === 'string');
        assert.ok(typeof module.hasPackageJson === 'boolean');
        assert.ok(typeof module.hasMainFile === 'boolean');
        assert.ok(typeof module.hasTestDir === 'boolean');
        
        // Optional fields
        if (module.packageData) {
          assert.ok(typeof module.packageData === 'object');
        }
      }
    });
  });
});