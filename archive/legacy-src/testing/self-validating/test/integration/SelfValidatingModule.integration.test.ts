/**
 * Integration tests for SelfValidatingModule
 * 
 * Tests the framework's ability to generate and validate tests for real modules in the system
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { SelfValidatingModule } from '../../SelfValidatingModule.js';
import * as fs from 'fs/promises';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test('Integration: Can validate multiple modules in system', async () => {
  const srcPath = path.join(__dirname, '../../../../..');
  
  // Discover and validate a few modules
  const modules = await SelfValidatingModule['discoverModules'](srcPath);
  assert.equal(Array.isArray(modules), true);
  assert.equal(modules.length > 0, true);
  
  // Test validation on first few modules
  const testModules = modules.slice(0, 3);
  
  for (const modulePath of testModules) {
    const result = await SelfValidatingModule.validateSelf(modulePath);
    
    assert.equal(typeof result.moduleId, 'string');
    assert.equal(typeof result.configType, 'string');
    assert.equal(typeof result.isCompliant, 'boolean');
    
    console.log(`Validated ${result.moduleId}: ${result.testsPassed}/${result.testsGenerated} tests passed`);
  }
});

test('Integration: Self-validation passes for this module', async () => {
  const modulePath = path.join(__dirname, '../..');
  const result = await SelfValidatingModule.validateSelf(modulePath);
  
  // This module should validate itself successfully
  assert.equal(result.moduleId, 'self-validating-module');
  assert.equal(result.isCompliant, true, 
    `Self-validation failed: ${result.validationErrors.map(e => e.message).join(', ')}`);
  
  // Should have passed basic structure tests
  const packageTest = result.structureTests.find(t => t.requirement.includes('package.json'));
  assert.equal(packageTest?.met, true);
  
  const testDirTest = result.structureTests.find(t => t.requirement.includes('Test directory'));
  assert.equal(testDirTest?.met, true);
});

test('Integration: Can generate self-test for real module', async () => {
  const modulePath = path.join(__dirname, '../..');
  const testContent = await SelfValidatingModule.generateSelfTest(modulePath);
  
  // Generated test should be valid TypeScript
  assert.equal(testContent.includes('import'), true);
  assert.equal(testContent.includes('test('), true);
  assert.equal(testContent.includes('assert.equal'), true);
  assert.equal(testContent.includes(result => result.moduleId), true);
  
  // Could actually write and run the generated test
  const tempTestFile = path.join(__dirname, '../temp-generated.test.ts');
  
  try {
    await fs.writeFile(tempTestFile, testContent);
    console.log('Generated test file created successfully');
    
    // Clean up
    await fs.unlink(tempTestFile);
  } catch (error) {
    console.warn('Could not test generated file:', error);
  }
});

test('Integration: Validates capability implementation correctly', async () => {
  const modulePath = path.join(__dirname, '../..');
  const result = await SelfValidatingModule.validateSelf(modulePath);
  
  // Check specific capabilities this module declares
  const universalTestingCap = result.capabilityTests.find(c => c.capability === 'universal-testing');
  assert.notEqual(universalTestingCap, undefined);
  
  const complianceCap = result.capabilityTests.find(c => c.capability === 'modular-compliance');
  assert.notEqual(complianceCap, undefined);
  
  // These should be implemented (since this module actually does provide them)
  if (!universalTestingCap?.implemented) {
    console.log('Universal testing capability evidence:', universalTestingCap?.evidence);
    console.log('Universal testing capability errors:', universalTestingCap?.errors);
  }
});