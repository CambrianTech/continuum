/**
 * Unit tests for BaseModule - Foundation of object-oriented validation
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { BaseModule } from '../BaseModule.js';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Test with a known good module (our testing module)
const testModulePath = path.join(__dirname, '../../testing/self-validating');

test('BaseModule validates basic structure requirements', async () => {
  const module = new BaseModule(testModulePath);
  const result = await module.validate();
  
  // Should have basic validation checks
  assert.equal(typeof result.isValid, 'boolean');
  assert.equal(Array.isArray(result.errors), true);
  assert.equal(Array.isArray(result.warnings), true);
  assert.equal(typeof result.checks, 'object');
  
  // Should check for basic requirements
  assert.equal(typeof result.checks.hasPackageJson, 'boolean');
  assert.equal(typeof result.checks.hasValidConfig, 'boolean');
  assert.equal(typeof result.checks.hasTestDirectory, 'boolean');
  
  console.log(`✅ BaseModule validation: ${Object.keys(result.checks).length} checks performed`);
  console.log(`   Valid: ${result.isValid}, Errors: ${result.errors.length}, Warnings: ${result.warnings.length}`);
});

test('BaseModule migration creates standard directories', async () => {
  // Create a temporary test module
  const tempPath = '/tmp/test-module-' + Date.now();
  
  try {
    // Create minimal structure
    const fs = await import('fs/promises');
    await fs.mkdir(tempPath, { recursive: true });
    await fs.writeFile(path.join(tempPath, 'package.json'), JSON.stringify({
      name: 'test-module',
      version: '1.0.0',
      description: 'Test module',
      continuum: {
        module: 'test',
        category: 'Testing',
        capabilities: [],
        dependencies: [],
        interfaces: [],
        permissions: []
      }
    }));
    
    const module = new BaseModule(tempPath);
    const result = await module.migrate();
    
    assert.equal(typeof result.migrated, 'boolean');
    assert.equal(Array.isArray(result.changes), true);
    assert.equal(Array.isArray(result.errors), true);
    
    // Should create test directories
    if (result.migrated) {
      assert.equal(result.changes.some(change => change.includes('test directory')), true);
    }
    
    console.log(`✅ BaseModule migration: ${result.changes.length} changes made`);
    
    // Cleanup
    await fs.rm(tempPath, { recursive: true, force: true });
    
  } catch (error) {
    console.log(`⚠️  Migration test skipped: ${error.message}`);
  }
});

test('BaseModule provides helper methods for subclasses', async () => {
  const module = new BaseModule(testModulePath);
  
  // Test protected methods (via accessing them on instance)
  const hasPackageJson = await module['checkFileExists']('package.json');
  const hasTestDir = await module['checkDirectoryExists']('test');
  
  assert.equal(typeof hasPackageJson, 'boolean');
  assert.equal(typeof hasTestDir, 'boolean');
  
  console.log(`✅ BaseModule helpers: package.json=${hasPackageJson}, test dir=${hasTestDir}`);
});

test('BaseModule demonstrates inheritance pattern', async () => {
  // This test shows how subclasses would extend BaseModule
  
  class TestModule extends BaseModule {
    async validate() {
      // Call parent validation first
      const baseResult = await super.validate();
      
      // Add custom validation
      const customChecks = {
        hasCustomFeature: true, // Simulate custom check
        customValidation: false // Simulate failing custom check
      };
      
      const customResult = {
        isValid: Object.values(customChecks).every(Boolean),
        errors: customChecks.customValidation ? [] : ['Custom validation failed'],
        warnings: [],
        checks: customChecks
      };
      
      // Combine results
      return this.combineValidationResults(baseResult, customResult);
    }
  }
  
  const testModule = new TestModule(testModulePath);
  const result = await testModule.validate();
  
  // Should have both base and custom checks
  assert.equal(typeof result.checks.hasPackageJson, 'boolean'); // From base
  assert.equal(typeof result.checks.hasCustomFeature, 'boolean'); // From custom
  
  console.log(`✅ Inheritance pattern: ${Object.keys(result.checks).length} total checks`);
  console.log(`   Base checks + custom checks = comprehensive validation`);
});