/**
 * Unit tests for SelfValidatingModule using object-oriented validation
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { SelfValidatingModule } from '../../SelfValidatingModule.js';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test('SelfValidatingModule validates itself using OOP approach', async () => {
  const modulePath = path.join(__dirname, '../..');
  const result = await SelfValidatingModule.validateSelf(modulePath);
  
  // Should successfully validate itself
  assert.equal(result.moduleId, 'self-validating-module');
  assert.equal(result.configType, 'module');
  assert.equal(typeof result.testsGenerated, 'number');
  assert.equal(typeof result.testsPassed, 'number');
  assert.equal(typeof result.isCompliant, 'boolean');
  
  // Should have structure tests from the module's own validate() method
  assert.equal(Array.isArray(result.structureTests), true);
  assert.equal(result.structureTests.length > 0, true);
  
  console.log(`✅ Self-validation: ${result.testsPassed}/${result.testsGenerated} checks passed`);
  console.log(`   Compliance: ${result.isCompliant ? 'COMPLIANT' : 'NON-COMPLIANT'}`);
});

test('SelfValidatingModule creates correct module instances', async () => {
  // Test that it creates appropriate module types
  const testConfigs = [
    {
      modulePath: './test-command',
      config: { command: 'test', category: 'Testing', capabilities: [], dependencies: [], interfaces: [], permissions: [] }
    },
    {
      modulePath: './test-daemon', 
      config: { daemon: 'test', category: 'Core', capabilities: [], dependencies: [], interfaces: [], permissions: [] }
    },
    {
      modulePath: './test-module',
      config: { module: 'test', category: 'Testing', capabilities: [], dependencies: [], interfaces: [], permissions: [] }
    }
  ];
  
  for (const testConfig of testConfigs) {
    const instance = SelfValidatingModule['createModuleInstance'](testConfig.modulePath, testConfig.config);
    assert.notEqual(instance, null);
    assert.equal(typeof instance.validate, 'function');
    assert.equal(typeof instance.migrate, 'function');
  }
});

test('SelfValidatingModule handles validation errors gracefully', async () => {
  const invalidPath = '/non/existent/path';
  const result = await SelfValidatingModule.validateSelf(invalidPath);
  
  assert.equal(result.isCompliant, false);
  assert.equal(result.validationErrors.length > 0, true);
  assert.equal(result.moduleId, 'unknown');
});